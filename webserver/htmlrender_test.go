package webserver

import "testing"

func TestFormatPriceRoundsCarryToNextInteger(t *testing.T) {
	cases := []struct {
		name  string
		price float32
		want  string
	}{
		{"plain", 2.10, `2<span class="text-neutral-500 text-sm">.10</span>`},
		{"carry from .999", 2.999, `3<span class="text-neutral-500 text-sm">.00</span>`},
		{"carry from .9999", 2.9999, `3<span class="text-neutral-500 text-sm">.00</span>`},
		{"below carry", 2.99, `2<span class="text-neutral-500 text-sm">.99</span>`},
		{"zero", 0, `0<span class="text-neutral-500 text-sm">.00</span>`},
		{"negative no carry", -2.5, `-2<span class="text-neutral-500 text-sm">.50</span>`},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := formatPrice(c.price, CurrencyEur)
			if got != c.want {
				t.Fatalf("formatPrice(%v): got %q want %q", c.price, got, c.want)
			}
		})
	}
}

func TestChartSettingsWithPricesNegativeZeroPositive(t *testing.T) {
	cs := DefaultChartSettings()
	prices := []float32{-10, 0, 10}
	m := cs.calculateMetrics(prices)

	if m.scale != 15.0 {
		t.Fatalf("scale: got %v want 15.0", m.scale)
	}
	if m.zeroOffset != 165.0 {
		t.Fatalf("zeroOffset: got %v want 165.0", m.zeroOffset)
	}
	if cs.calculateBarHeight(-10, m) != 150.0 {
		t.Fatalf("bar height -10: got %v want 150.0", cs.calculateBarHeight(-10, m))
	}
	if cs.calculateBarHeight(0, m) != 1.0 {
		t.Fatalf("bar height 0: got %v want 1.0", cs.calculateBarHeight(0, m))
	}
	if cs.calculateBarHeight(10, m) != 150.0 {
		t.Fatalf("bar height 10: got %v want 150.0", cs.calculateBarHeight(10, m))
	}
	if cs.calculateBarY(-10, m) != 165.0 {
		t.Fatalf("bar y -10: got %v want 165.0", cs.calculateBarY(-10, m))
	}
	if cs.calculateBarY(0, m) != 165.0 {
		t.Fatalf("bar y 0: got %v want 165.0", cs.calculateBarY(0, m))
	}
	if cs.calculateBarY(10, m) != 15.0 {
		t.Fatalf("bar y 10: got %v want 15.0", cs.calculateBarY(10, m))
	}
}
