package webserver

import (
	"fmt"
	"log"
	"sort"
	"strconv"
	"strings"
)

const cssReset = "" +
	"*,::before,::after{box-sizing:border-box;border-width:0;border-style:solid;border-color:currentColor}" +
	"html{line-height:1.5;-webkit-text-size-adjust:100%;font-family:ui-sans-serif,system-ui,-apple-system,\"Segoe UI\",Roboto,\"Helvetica Neue\",Arial,sans-serif}" +
	"body{margin:0;line-height:inherit}" +
	"h1,h2,h3,h4,h5,h6{font-size:inherit;font-weight:inherit;margin:0}" +
	"p,blockquote,pre,figure,dl,dd{margin:0}" +
	"ul,ol{margin:0;padding:0;list-style:none}" +
	"a{color:inherit;text-decoration:inherit}" +
	"table{border-collapse:collapse;border-spacing:0;text-indent:0}" +
	"button,input,select,textarea{font-family:inherit;font-size:100%;font-weight:inherit;line-height:inherit;color:inherit;margin:0;padding:0}" +
	"button{background:transparent;cursor:pointer;border:0}" +
	"img,svg,video,canvas{display:block;max-width:100%}" +
	"code,kbd,samp,pre{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace}"

// ExtractClassesFromHTML scans html for class="..." occurrences and adds them to the set.
func ExtractClassesFromHTML(html string, out map[string]struct{}) {
	needle := `class="`
	i := 0
	for i+len(needle) <= len(html) {
		if html[i:i+len(needle)] == needle {
			i += len(needle)
			start := i
			for i < len(html) && html[i] != '"' {
				i++
			}
			for _, c := range strings.Fields(html[start:i]) {
				out[c] = struct{}{}
			}
			i++
		} else {
			i++
		}
	}
}

// ExtractClassesFromStr adds whitespace-separated classes to the set.
func ExtractClassesFromStr(s string, out map[string]struct{}) {
	for _, c := range strings.Fields(s) {
		out[c] = struct{}{}
	}
}

type ruleKind int

const (
	ruleBase ruleKind = iota
	ruleHover
	ruleDark
)

// GenerateCSS turns the set of class names into a stylesheet.
func GenerateCSS(classes map[string]struct{}) string {
	var base, hover, dark []string
	for class := range classes {
		kind, css, ok := classToRule(class)
		if !ok {
			log.Printf("css_gen: unknown class: %s", class)
			continue
		}
		switch kind {
		case ruleBase:
			base = append(base, css)
		case ruleHover:
			hover = append(hover, css)
		case ruleDark:
			dark = append(dark, css)
		}
	}
	sort.Strings(base)
	sort.Strings(hover)
	sort.Strings(dark)

	var sb strings.Builder
	sb.WriteString(cssReset)
	for _, s := range base {
		sb.WriteString(s)
	}
	for _, s := range hover {
		sb.WriteString(s)
	}
	if len(dark) > 0 {
		sb.WriteString("@media (prefers-color-scheme:dark){")
		for _, s := range dark {
			sb.WriteString(s)
		}
		sb.WriteByte('}')
	}
	return sb.String()
}

func classToRule(class string) (ruleKind, string, bool) {
	var kind ruleKind
	base := class
	switch {
	case strings.HasPrefix(class, "dark:"):
		kind = ruleDark
		base = strings.TrimPrefix(class, "dark:")
	case strings.HasPrefix(class, "hover:"):
		kind = ruleHover
		base = strings.TrimPrefix(class, "hover:")
	default:
		kind = ruleBase
	}
	suffix, body, ok := ruleBody(base)
	if !ok {
		return 0, "", false
	}
	escaped := strings.ReplaceAll(class, ":", `\:`)
	pseudo := ""
	if kind == ruleHover {
		pseudo = ":hover"
	}
	return kind, fmt.Sprintf(".%s%s%s{%s}", escaped, pseudo, suffix, body), true
}

func ruleBody(class string) (string, string, bool) {
	if body, ok := staticRule(class); ok {
		return "", body, true
	}

	if rest, ok := stripPrefix(class, "space-x-"); ok {
		rem, ok := parseRem(rest)
		if !ok {
			return "", "", false
		}
		return " > * + *", "margin-left:" + rem, true
	}

	single := []struct {
		prefix string
		prop   string
	}{
		{"p-", "padding"},
		{"pt-", "padding-top"},
		{"pb-", "padding-bottom"},
		{"pl-", "padding-left"},
		{"pr-", "padding-right"},
		{"m-", "margin"},
		{"mt-", "margin-top"},
		{"mb-", "margin-bottom"},
		{"ml-", "margin-left"},
		{"mr-", "margin-right"},
		{"gap-", "gap"},
		{"w-", "width"},
		{"h-", "height"},
	}
	for _, p := range single {
		if rest, ok := stripPrefix(class, p.prefix); ok {
			rem, ok := parseRem(rest)
			if !ok {
				return "", "", false
			}
			return "", p.prop + ":" + rem, true
		}
	}

	paired := []struct {
		prefix string
		props  [2]string
	}{
		{"px-", [2]string{"padding-left", "padding-right"}},
		{"py-", [2]string{"padding-top", "padding-bottom"}},
		{"mx-", [2]string{"margin-left", "margin-right"}},
		{"my-", [2]string{"margin-top", "margin-bottom"}},
	}
	for _, p := range paired {
		if rest, ok := stripPrefix(class, p.prefix); ok {
			rem, ok := parseRem(rest)
			if !ok {
				return "", "", false
			}
			return "", fmt.Sprintf("%s:%s;%s:%s", p.props[0], rem, p.props[1], rem), true
		}
	}

	if rest, ok := stripPrefix(class, "bg-"); ok {
		if hex, ok := parseColor(rest); ok {
			return "", "background-color:" + hex, true
		}
		return "", "", false
	}
	if rest, ok := stripPrefix(class, "fill-"); ok {
		if hex, ok := parseColor(rest); ok {
			return "", "fill:" + hex, true
		}
		return "", "", false
	}
	if rest, ok := stripPrefix(class, "text-"); ok {
		if hex, ok := parseColor(rest); ok {
			return "", "color:" + hex, true
		}
		return "", "", false
	}
	if rest, ok := stripPrefix(class, "outline-"); ok {
		if n, err := strconv.ParseUint(rest, 10, 32); err == nil {
			return "", fmt.Sprintf("outline-width:%dpx;outline-style:solid", n), true
		}
		if hex, ok := parseColor(rest); ok {
			return "", "outline-color:" + hex, true
		}
		return "", "", false
	}

	return "", "", false
}

func stripPrefix(s, prefix string) (string, bool) {
	if strings.HasPrefix(s, prefix) {
		return s[len(prefix):], true
	}
	return "", false
}

func staticRule(class string) (string, bool) {
	switch class {
	case "flex":
		return "display:flex", true
	case "inline-flex":
		return "display:inline-flex", true
	case "flex-row":
		return "flex-direction:row", true
	case "flex-col":
		return "flex-direction:column", true
	case "justify-center":
		return "justify-content:center", true
	case "items-center":
		return "align-items:center", true
	case "text-left":
		return "text-align:left", true
	case "text-center":
		return "text-align:center", true
	case "text-right":
		return "text-align:right", true
	case "text-xs":
		return "font-size:0.75rem;line-height:1rem", true
	case "text-sm":
		return "font-size:0.875rem;line-height:1.25rem", true
	case "text-base":
		return "font-size:1rem;line-height:1.5rem", true
	case "text-lg":
		return "font-size:1.125rem;line-height:1.75rem", true
	case "text-xl":
		return "font-size:1.25rem;line-height:1.75rem", true
	case "text-2xl":
		return "font-size:1.5rem;line-height:2rem", true
	case "text-3xl":
		return "font-size:1.875rem;line-height:2.25rem", true
	case "text-4xl":
		return "font-size:2.25rem;line-height:2.5rem", true
	case "font-bold":
		return "font-weight:700", true
	case "font-semibold":
		return "font-weight:600", true
	case "font-mono":
		return "font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace", true
	case "border":
		return "border-width:1px;border-style:solid", true
	case "rounded":
		return "border-radius:0.25rem", true
	case "underline":
		return "text-decoration:underline", true
	case "cursor-pointer":
		return "cursor:pointer", true
	}
	return "", false
}

func parseRem(s string) (string, bool) {
	n, err := strconv.ParseUint(s, 10, 32)
	if err != nil {
		return "", false
	}
	if n == 0 {
		return "0", true
	}
	whole := n / 4
	frac := n % 4
	switch {
	case whole == 0 && frac == 1:
		return "0.25rem", true
	case whole == 0 && frac == 2:
		return "0.5rem", true
	case whole == 0 && frac == 3:
		return "0.75rem", true
	case frac == 0:
		return fmt.Sprintf("%drem", whole), true
	case frac == 1:
		return fmt.Sprintf("%d.25rem", whole), true
	case frac == 2:
		return fmt.Sprintf("%d.5rem", whole), true
	case frac == 3:
		return fmt.Sprintf("%d.75rem", whole), true
	}
	return "", false
}

func parseColor(s string) (string, bool) {
	if s == "white" {
		return "#fff", true
	}
	if s == "black" {
		return "#000", true
	}
	idx := strings.LastIndex(s, "-")
	if idx < 0 {
		return "", false
	}
	color := s[:idx]
	shade := s[idx+1:]
	var shades [][2]string
	switch color {
	case "red":
		shades = [][2]string{
			{"50", "#fef2f2"}, {"100", "#fee2e2"}, {"200", "#fecaca"},
			{"300", "#fca5a5"}, {"400", "#f87171"}, {"500", "#ef4444"},
			{"600", "#dc2626"}, {"700", "#b91c1c"}, {"800", "#991b1b"},
			{"900", "#7f1d1d"}, {"950", "#450a0a"},
		}
	case "blue":
		shades = [][2]string{
			{"50", "#eff6ff"}, {"100", "#dbeafe"}, {"200", "#bfdbfe"},
			{"300", "#93c5fd"}, {"400", "#60a5fa"}, {"500", "#3b82f6"},
			{"600", "#2563eb"}, {"700", "#1d4ed8"}, {"800", "#1e40af"},
			{"900", "#1e3a8a"}, {"950", "#172554"},
		}
	case "green":
		shades = [][2]string{
			{"50", "#f0fdf4"}, {"100", "#dcfce7"}, {"200", "#bbf7d0"},
			{"300", "#86efac"}, {"400", "#4ade80"}, {"500", "#22c55e"},
			{"600", "#16a34a"}, {"700", "#15803d"}, {"800", "#166534"},
			{"900", "#14532d"}, {"950", "#052e16"},
		}
	case "gray":
		shades = [][2]string{
			{"50", "#f9fafb"}, {"100", "#f3f4f6"}, {"200", "#e5e7eb"},
			{"300", "#d1d5db"}, {"400", "#9ca3af"}, {"500", "#6b7280"},
			{"600", "#4b5563"}, {"700", "#374151"}, {"800", "#1f2937"},
			{"900", "#111827"}, {"950", "#030712"},
		}
	case "neutral":
		shades = [][2]string{
			{"50", "#fafafa"}, {"100", "#f5f5f5"}, {"200", "#e5e5e5"},
			{"300", "#d4d4d4"}, {"400", "#a3a3a3"}, {"500", "#737373"},
			{"600", "#525252"}, {"700", "#404040"}, {"800", "#262626"},
			{"900", "#171717"}, {"950", "#0a0a0a"},
		}
	default:
		return "", false
	}
	for _, sh := range shades {
		if sh[0] == shade {
			return sh[1], true
		}
	}
	return "", false
}
