package webserver

import "testing"

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
