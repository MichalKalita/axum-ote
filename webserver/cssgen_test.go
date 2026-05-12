package webserver

import (
	"strings"
	"testing"
)

func one(class string) string {
	s := map[string]struct{}{class: {}}
	css := GenerateCSS(s)
	return strings.TrimPrefix(css, cssReset)
}

func TestFlex(t *testing.T) {
	if got := one("flex"); got != ".flex{display:flex}" {
		t.Fatalf("got %q", got)
	}
}

func TestPadding4Is1rem(t *testing.T) {
	if got := one("p-4"); got != ".p-4{padding:1rem}" {
		t.Fatalf("got %q", got)
	}
}

func TestPaddingRight10Is2_5rem(t *testing.T) {
	if got := one("pr-10"); got != ".pr-10{padding-right:2.5rem}" {
		t.Fatalf("got %q", got)
	}
}

func TestMarginBottom8Is2rem(t *testing.T) {
	if got := one("mb-8"); got != ".mb-8{margin-bottom:2rem}" {
		t.Fatalf("got %q", got)
	}
}

func TestPx4IsPaired(t *testing.T) {
	if got := one("px-4"); got != ".px-4{padding-left:1rem;padding-right:1rem}" {
		t.Fatalf("got %q", got)
	}
}

func TestWidth16(t *testing.T) {
	if got := one("w-16"); got != ".w-16{width:4rem}" {
		t.Fatalf("got %q", got)
	}
}

func TestText4xl(t *testing.T) {
	css := one("text-4xl")
	if !strings.Contains(css, "font-size:2.25rem") {
		t.Fatalf("missing font-size: %q", css)
	}
	if !strings.Contains(css, "line-height:2.5rem") {
		t.Fatalf("missing line-height: %q", css)
	}
}

func TestBgRed100(t *testing.T) {
	if got := one("bg-red-100"); got != ".bg-red-100{background-color:#fee2e2}" {
		t.Fatalf("got %q", got)
	}
}

func TestTextWhite(t *testing.T) {
	if got := one("text-white"); got != ".text-white{color:#fff}" {
		t.Fatalf("got %q", got)
	}
}

func TestTextColorWithShade(t *testing.T) {
	if got := one("text-blue-600"); got != ".text-blue-600{color:#2563eb}" {
		t.Fatalf("got %q", got)
	}
}

func TestFillWithShade(t *testing.T) {
	if got := one("fill-green-600"); got != ".fill-green-600{fill:#16a34a}" {
		t.Fatalf("got %q", got)
	}
}

// The consumption-analysis score gradient uses yellow and orange shades —
// without them the layout renders unstyled and the css_gen log fills with
// "unknown class" lines.
func TestYellowOrangeShadesAreRecognized(t *testing.T) {
	cases := map[string]string{
		"text-yellow-700": ".text-yellow-700{color:#a16207}",
		"text-yellow-400": ".text-yellow-400{color:#facc15}",
		"text-orange-700": ".text-orange-700{color:#c2410c}",
		"text-orange-400": ".text-orange-400{color:#fb923c}",
	}
	for class, want := range cases {
		if got := one(class); got != want {
			t.Errorf("%s: got %q want %q", class, got, want)
		}
	}
}

func TestDarkVariantWrapsInMediaQuery(t *testing.T) {
	got := one("dark:bg-gray-900")
	want := `@media (prefers-color-scheme:dark){.dark\:bg-gray-900{background-color:#111827}}`
	if got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

func TestHoverVariant(t *testing.T) {
	got := one("hover:text-red-400")
	want := `.hover\:text-red-400:hover{color:#f87171}`
	if got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

func TestSpaceXUsesChildCombinator(t *testing.T) {
	if got := one("space-x-2"); got != ".space-x-2 > * + *{margin-left:0.5rem}" {
		t.Fatalf("got %q", got)
	}
}

func TestOutlineNumericIsWidth(t *testing.T) {
	if got := one("outline-2"); got != ".outline-2{outline-width:2px;outline-style:solid}" {
		t.Fatalf("got %q", got)
	}
}

func TestOutlineColor(t *testing.T) {
	if got := one("outline-blue-500"); got != ".outline-blue-500{outline-color:#3b82f6}" {
		t.Fatalf("got %q", got)
	}
}

func TestUnknownClassIsSkipped(t *testing.T) {
	if got := one("does-not-exist-42"); got != "" {
		t.Fatalf("got %q", got)
	}
}

func TestHtmlScanPicksUpClasses(t *testing.T) {
	classes := map[string]struct{}{}
	ExtractClassesFromHTML(`<div class="a b c"><span class="d">x</span></div>`, classes)
	if len(classes) != 4 {
		t.Fatalf("expected 4, got %d", len(classes))
	}
	if _, ok := classes["a"]; !ok {
		t.Fatal("missing a")
	}
	if _, ok := classes["d"]; !ok {
		t.Fatal("missing d")
	}
}

func TestHtmlScanIgnoresOtherAttributes(t *testing.T) {
	classes := map[string]struct{}{}
	ExtractClassesFromHTML(`<a href="x.html" title="hi" class="link"></a>`, classes)
	if len(classes) != 1 {
		t.Fatalf("expected 1, got %d", len(classes))
	}
	if _, ok := classes["link"]; !ok {
		t.Fatal("missing link")
	}
}

func TestHtmlScanHandlesUnicode(t *testing.T) {
	classes := map[string]struct{}{}
	ExtractClassesFromHTML(`<a title="Něco česky" class="ok"></a>`, classes)
	if _, ok := classes["ok"]; !ok {
		t.Fatal("missing ok")
	}
}

func TestDarkRulesShareOneMediaBlock(t *testing.T) {
	classes := map[string]struct{}{
		"dark:bg-gray-900":   {},
		"dark:text-gray-300": {},
	}
	css := GenerateCSS(classes)
	if strings.Count(css, "@media") != 1 {
		t.Fatalf("expected one @media block, got css=%q", css)
	}
	if !strings.Contains(css, `.dark\:bg-gray-900`) {
		t.Fatal("missing dark:bg-gray-900")
	}
	if !strings.Contains(css, `.dark\:text-gray-300`) {
		t.Fatal("missing dark:text-gray-300")
	}
}

func TestBaseHoverDarkOrdering(t *testing.T) {
	classes := map[string]struct{}{
		"flex":               {},
		"hover:text-red-400": {},
		"dark:bg-gray-900":   {},
	}
	css := GenerateCSS(classes)
	basePos := strings.LastIndex(css, ".flex{")
	hoverPos := strings.Index(css, ":hover")
	darkPos := strings.Index(css, "@media")
	if basePos < 0 || hoverPos < 0 || darkPos < 0 {
		t.Fatalf("missing pos: %q", css)
	}
	if !(basePos < hoverPos) {
		t.Fatalf("base should come before hover: %q", css)
	}
	if !(hoverPos < darkPos) {
		t.Fatalf("hover should come before dark: %q", css)
	}
}

func TestResetIsEmittedEvenForEmptySet(t *testing.T) {
	css := GenerateCSS(map[string]struct{}{})
	if !strings.Contains(css, "box-sizing:border-box") {
		t.Fatal("missing reset")
	}
	if !strings.Contains(css, "body{margin:0") {
		t.Fatal("missing body reset")
	}
}

func TestResetIncludesDefaultFont(t *testing.T) {
	css := GenerateCSS(map[string]struct{}{})
	if !strings.Contains(css, "font-family:ui-sans-serif") {
		t.Fatal("missing default font")
	}
}
