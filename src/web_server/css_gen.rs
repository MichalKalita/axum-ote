use std::collections::HashSet;

const RESET: &str = "\
*,::before,::after{box-sizing:border-box;border-width:0;border-style:solid;border-color:currentColor}\
html{line-height:1.5;-webkit-text-size-adjust:100%;font-family:ui-sans-serif,system-ui,-apple-system,\"Segoe UI\",Roboto,\"Helvetica Neue\",Arial,sans-serif}\
body{margin:0;line-height:inherit}\
h1,h2,h3,h4,h5,h6{font-size:inherit;font-weight:inherit;margin:0}\
p,blockquote,pre,figure,dl,dd{margin:0}\
ul,ol{margin:0;padding:0;list-style:none}\
a{color:inherit;text-decoration:inherit}\
table{border-collapse:collapse;border-spacing:0;text-indent:0}\
button,input,select,textarea{font-family:inherit;font-size:100%;font-weight:inherit;line-height:inherit;color:inherit;margin:0;padding:0}\
button{background:transparent;cursor:pointer;border:0}\
img,svg,video,canvas{display:block;max-width:100%}\
code,kbd,samp,pre{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace}\
";

pub fn extract_classes_from_html(html: &str, out: &mut HashSet<String>) {
    let bytes = html.as_bytes();
    let needle = b"class=\"";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            i += needle.len();
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            for class in html[start..i].split_ascii_whitespace() {
                out.insert(class.to_string());
            }
            i += 1;
        } else {
            i += 1;
        }
    }
}

pub fn extract_classes_from_str(s: &str, out: &mut HashSet<String>) {
    for class in s.split_ascii_whitespace() {
        out.insert(class.to_string());
    }
}

pub fn generate_css(classes: &HashSet<String>) -> String {
    let mut base = Vec::new();
    let mut hover = Vec::new();
    let mut dark = Vec::new();
    for class in classes {
        match class_to_rule(class) {
            Some((RuleKind::Base, css)) => base.push(css),
            Some((RuleKind::Hover, css)) => hover.push(css),
            Some((RuleKind::Dark, css)) => dark.push(css),
            None => log::warn!("css_gen: unknown class: {}", class),
        }
    }
    base.sort();
    hover.sort();
    dark.sort();
    let mut out = String::new();
    out.push_str(RESET);
    for s in &base {
        out.push_str(s);
    }
    for s in &hover {
        out.push_str(s);
    }
    if !dark.is_empty() {
        out.push_str("@media (prefers-color-scheme:dark){");
        for s in &dark {
            out.push_str(s);
        }
        out.push('}');
    }
    out
}

enum RuleKind {
    Base,
    Hover,
    Dark,
}

fn class_to_rule(class: &str) -> Option<(RuleKind, String)> {
    let (kind, base) = if let Some(rest) = class.strip_prefix("dark:") {
        (RuleKind::Dark, rest)
    } else if let Some(rest) = class.strip_prefix("hover:") {
        (RuleKind::Hover, rest)
    } else {
        (RuleKind::Base, class)
    };
    let (suffix, body) = rule_body(base)?;
    let escaped = class.replace(':', "\\:");
    let pseudo = match kind {
        RuleKind::Hover => ":hover",
        _ => "",
    };
    Some((kind, format!(".{escaped}{pseudo}{suffix}{{{body}}}")))
}

fn rule_body(class: &str) -> Option<(&'static str, String)> {
    if let Some(body) = static_rule(class) {
        return Some(("", body.to_string()));
    }

    if let Some(rest) = class.strip_prefix("space-x-") {
        let rem = parse_rem(rest)?;
        return Some((" > * + *", format!("margin-left:{rem}")));
    }

    let single: &[(&str, &str)] = &[
        ("p-", "padding"),
        ("pt-", "padding-top"),
        ("pb-", "padding-bottom"),
        ("pl-", "padding-left"),
        ("pr-", "padding-right"),
        ("m-", "margin"),
        ("mt-", "margin-top"),
        ("mb-", "margin-bottom"),
        ("ml-", "margin-left"),
        ("mr-", "margin-right"),
        ("gap-", "gap"),
        ("w-", "width"),
        ("h-", "height"),
    ];
    for (prefix, prop) in single {
        if let Some(rest) = class.strip_prefix(prefix) {
            let rem = parse_rem(rest)?;
            return Some(("", format!("{prop}:{rem}")));
        }
    }

    let paired: &[(&str, [&str; 2])] = &[
        ("px-", ["padding-left", "padding-right"]),
        ("py-", ["padding-top", "padding-bottom"]),
        ("mx-", ["margin-left", "margin-right"]),
        ("my-", ["margin-top", "margin-bottom"]),
    ];
    for (prefix, props) in paired {
        if let Some(rest) = class.strip_prefix(prefix) {
            let rem = parse_rem(rest)?;
            return Some(("", format!("{}:{rem};{}:{rem}", props[0], props[1])));
        }
    }

    if let Some(rest) = class.strip_prefix("bg-") {
        return parse_color(rest).map(|hex| ("", format!("background-color:{hex}")));
    }
    if let Some(rest) = class.strip_prefix("fill-") {
        return parse_color(rest).map(|hex| ("", format!("fill:{hex}")));
    }
    if let Some(rest) = class.strip_prefix("text-") {
        return parse_color(rest).map(|hex| ("", format!("color:{hex}")));
    }
    if let Some(rest) = class.strip_prefix("outline-") {
        if let Ok(n) = rest.parse::<u32>() {
            return Some(("", format!("outline-width:{n}px;outline-style:solid")));
        }
        return parse_color(rest).map(|hex| ("", format!("outline-color:{hex}")));
    }

    None
}

fn static_rule(class: &str) -> Option<&'static str> {
    Some(match class {
        "flex" => "display:flex",
        "flex-row" => "flex-direction:row",
        "flex-col" => "flex-direction:column",
        "justify-center" => "justify-content:center",
        "items-center" => "align-items:center",
        "text-left" => "text-align:left",
        "text-center" => "text-align:center",
        "text-right" => "text-align:right",
        "text-xs" => "font-size:0.75rem;line-height:1rem",
        "text-sm" => "font-size:0.875rem;line-height:1.25rem",
        "text-base" => "font-size:1rem;line-height:1.5rem",
        "text-lg" => "font-size:1.125rem;line-height:1.75rem",
        "text-xl" => "font-size:1.25rem;line-height:1.75rem",
        "text-2xl" => "font-size:1.5rem;line-height:2rem",
        "text-3xl" => "font-size:1.875rem;line-height:2.25rem",
        "text-4xl" => "font-size:2.25rem;line-height:2.5rem",
        "font-bold" => "font-weight:700",
        "font-semibold" => "font-weight:600",
        "font-mono" => "font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace",
        "border" => "border-width:1px;border-style:solid",
        "rounded" => "border-radius:0.25rem",
        "underline" => "text-decoration:underline",
        "cursor-pointer" => "cursor:pointer",
        _ => return None,
    })
}

fn parse_rem(s: &str) -> Option<String> {
    let n: u32 = s.parse().ok()?;
    if n == 0 {
        return Some("0".into());
    }
    let whole = n / 4;
    let frac = n % 4;
    Some(match (whole, frac) {
        (0, 1) => "0.25rem".into(),
        (0, 2) => "0.5rem".into(),
        (0, 3) => "0.75rem".into(),
        (w, 0) => format!("{w}rem"),
        (w, 1) => format!("{w}.25rem"),
        (w, 2) => format!("{w}.5rem"),
        (w, 3) => format!("{w}.75rem"),
        _ => unreachable!(),
    })
}

fn parse_color(s: &str) -> Option<&'static str> {
    if s == "white" {
        return Some("#fff");
    }
    if s == "black" {
        return Some("#000");
    }
    let (color, shade) = s.rsplit_once('-')?;
    let shades: &[(&str, &str); 11] = match color {
        "red" => &[
            ("50", "#fef2f2"), ("100", "#fee2e2"), ("200", "#fecaca"),
            ("300", "#fca5a5"), ("400", "#f87171"), ("500", "#ef4444"),
            ("600", "#dc2626"), ("700", "#b91c1c"), ("800", "#991b1b"),
            ("900", "#7f1d1d"), ("950", "#450a0a"),
        ],
        "blue" => &[
            ("50", "#eff6ff"), ("100", "#dbeafe"), ("200", "#bfdbfe"),
            ("300", "#93c5fd"), ("400", "#60a5fa"), ("500", "#3b82f6"),
            ("600", "#2563eb"), ("700", "#1d4ed8"), ("800", "#1e40af"),
            ("900", "#1e3a8a"), ("950", "#172554"),
        ],
        "green" => &[
            ("50", "#f0fdf4"), ("100", "#dcfce7"), ("200", "#bbf7d0"),
            ("300", "#86efac"), ("400", "#4ade80"), ("500", "#22c55e"),
            ("600", "#16a34a"), ("700", "#15803d"), ("800", "#166534"),
            ("900", "#14532d"), ("950", "#052e16"),
        ],
        "gray" => &[
            ("50", "#f9fafb"), ("100", "#f3f4f6"), ("200", "#e5e7eb"),
            ("300", "#d1d5db"), ("400", "#9ca3af"), ("500", "#6b7280"),
            ("600", "#4b5563"), ("700", "#374151"), ("800", "#1f2937"),
            ("900", "#111827"), ("950", "#030712"),
        ],
        "neutral" => &[
            ("50", "#fafafa"), ("100", "#f5f5f5"), ("200", "#e5e5e5"),
            ("300", "#d4d4d4"), ("400", "#a3a3a3"), ("500", "#737373"),
            ("600", "#525252"), ("700", "#404040"), ("800", "#262626"),
            ("900", "#171717"), ("950", "#0a0a0a"),
        ],
        _ => return None,
    };
    shades.iter().find(|(s, _)| *s == shade).map(|(_, hex)| *hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(class: &str) -> String {
        let mut s = HashSet::new();
        s.insert(class.to_string());
        let css = generate_css(&s);
        css.strip_prefix(RESET).unwrap().to_string()
    }

    #[test]
    fn flex() {
        assert_eq!(one("flex"), ".flex{display:flex}");
    }

    #[test]
    fn padding_4_is_1rem() {
        assert_eq!(one("p-4"), ".p-4{padding:1rem}");
    }

    #[test]
    fn padding_right_10_is_2_5rem() {
        assert_eq!(one("pr-10"), ".pr-10{padding-right:2.5rem}");
    }

    #[test]
    fn margin_bottom_8_is_2rem() {
        assert_eq!(one("mb-8"), ".mb-8{margin-bottom:2rem}");
    }

    #[test]
    fn px_4_is_paired() {
        assert_eq!(
            one("px-4"),
            ".px-4{padding-left:1rem;padding-right:1rem}"
        );
    }

    #[test]
    fn width_16() {
        assert_eq!(one("w-16"), ".w-16{width:4rem}");
    }

    #[test]
    fn text_4xl_has_size_and_line_height() {
        let css = one("text-4xl");
        assert!(css.contains("font-size:2.25rem"));
        assert!(css.contains("line-height:2.5rem"));
    }

    #[test]
    fn bg_red_100() {
        assert_eq!(one("bg-red-100"), ".bg-red-100{background-color:#fee2e2}");
    }

    #[test]
    fn text_white() {
        assert_eq!(one("text-white"), ".text-white{color:#fff}");
    }

    #[test]
    fn text_color_with_shade() {
        assert_eq!(one("text-blue-600"), ".text-blue-600{color:#2563eb}");
    }

    #[test]
    fn fill_with_shade() {
        assert_eq!(one("fill-green-600"), ".fill-green-600{fill:#16a34a}");
    }

    #[test]
    fn dark_variant_wraps_in_media_query() {
        let css = one("dark:bg-gray-900");
        assert_eq!(
            css,
            "@media (prefers-color-scheme:dark){.dark\\:bg-gray-900{background-color:#111827}}"
        );
    }

    #[test]
    fn hover_variant_appends_pseudo() {
        assert_eq!(
            one("hover:text-red-400"),
            ".hover\\:text-red-400:hover{color:#f87171}"
        );
    }

    #[test]
    fn space_x_uses_child_combinator() {
        assert_eq!(
            one("space-x-2"),
            ".space-x-2 > * + *{margin-left:0.5rem}"
        );
    }

    #[test]
    fn outline_numeric_is_width() {
        assert_eq!(
            one("outline-2"),
            ".outline-2{outline-width:2px;outline-style:solid}"
        );
    }

    #[test]
    fn outline_color() {
        assert_eq!(
            one("outline-blue-500"),
            ".outline-blue-500{outline-color:#3b82f6}"
        );
    }

    #[test]
    fn unknown_class_is_skipped() {
        assert_eq!(one("does-not-exist-42"), "");
    }

    #[test]
    fn html_scan_picks_up_classes() {
        let mut classes = HashSet::new();
        extract_classes_from_html(
            r#"<div class="a b c"><span class="d">x</span></div>"#,
            &mut classes,
        );
        assert_eq!(classes.len(), 4);
        assert!(classes.contains("a"));
        assert!(classes.contains("d"));
    }

    #[test]
    fn html_scan_ignores_other_attributes() {
        let mut classes = HashSet::new();
        extract_classes_from_html(
            r#"<a href="x.html" title="hi" class="link"></a>"#,
            &mut classes,
        );
        assert_eq!(classes.len(), 1);
        assert!(classes.contains("link"));
    }

    #[test]
    fn html_scan_handles_unicode_in_other_attrs() {
        let mut classes = HashSet::new();
        extract_classes_from_html(
            r#"<a title="Něco česky" class="ok"></a>"#,
            &mut classes,
        );
        assert!(classes.contains("ok"));
    }

    #[test]
    fn dark_rules_share_one_media_block() {
        let mut classes = HashSet::new();
        classes.insert("dark:bg-gray-900".into());
        classes.insert("dark:text-gray-300".into());
        let css = generate_css(&classes);
        assert_eq!(css.matches("@media").count(), 1);
        assert!(css.contains(".dark\\:bg-gray-900"));
        assert!(css.contains(".dark\\:text-gray-300"));
    }

    #[test]
    fn base_hover_dark_ordering() {
        let mut classes = HashSet::new();
        classes.insert("flex".into());
        classes.insert("hover:text-red-400".into());
        classes.insert("dark:bg-gray-900".into());
        let css = generate_css(&classes);
        let base_pos = css.rfind(".flex{").unwrap();
        let hover_pos = css.find(":hover").unwrap();
        let dark_pos = css.find("@media").unwrap();
        assert!(base_pos < hover_pos);
        assert!(hover_pos < dark_pos);
    }

    #[test]
    fn reset_is_emitted_even_for_empty_set() {
        let css = generate_css(&HashSet::new());
        assert!(css.contains("box-sizing:border-box"));
        assert!(css.contains("body{margin:0"));
    }

    #[test]
    fn reset_includes_default_font() {
        let css = generate_css(&HashSet::new());
        assert!(css.contains("font-family:ui-sans-serif"));
    }
}
