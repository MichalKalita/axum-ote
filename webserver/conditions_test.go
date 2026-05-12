package webserver

import (
	"testing"
	"time"
)

func setupCtx() *EvaluateContext {
	now, _ := time.Parse("2006-01-02 15:04:05", "2020-01-01 02:00:00")
	prices := make([]float32, 24)
	for i := range prices {
		prices[i] = float32(i)
	}
	return NewEvaluateContext(now, prices, 2)
}

func TestPrice(t *testing.T) {
	ctx := setupCtx()

	cond := Condition{Kind: CondPrice, Price: 100.0}
	if !cond.Evaluate(ctx) {
		t.Fatal("expected price condition to be true")
	}
	got := cond.EvaluateAll(ctx)
	for i, v := range got {
		if !v {
			t.Fatalf("expected all true at idx %d", i)
		}
	}

	cond = Condition{Kind: CondPrice, Price: 0.0}
	if cond.Evaluate(ctx) {
		t.Fatal("expected price condition to be false")
	}
	got = cond.EvaluateAll(ctx)
	expected := [24]bool{true}
	for i, v := range got {
		if v != expected[i] {
			t.Fatalf("idx %d: got %v want %v", i, v, expected[i])
		}
	}
}

func TestHours(t *testing.T) {
	ctx := setupCtx()

	if !(Condition{Kind: CondHours, HoursMin: 0, HoursMax: 2}.Evaluate(ctx)) {
		t.Fatal("Hours(0,2) should be true")
	}
	if (Condition{Kind: CondHours, HoursMin: 3, HoursMax: 4}.Evaluate(ctx)) {
		t.Fatal("Hours(3,4) should be false")
	}
	if !(Condition{Kind: CondHours, HoursMin: 1, HoursMax: 3}.Evaluate(ctx)) {
		t.Fatal("Hours(1,3) should be true")
	}

	got := Condition{Kind: CondHours, HoursMin: 1, HoursMax: 3}.EvaluateAll(ctx)
	expected := [24]bool{false, true, true, true}
	for i, v := range got {
		if v != expected[i] {
			t.Fatalf("idx %d: got %v want %v", i, v, expected[i])
		}
	}
}

func TestNot(t *testing.T) {
	ctx := setupCtx()
	inner := Condition{Kind: CondDebug, Debug: true}
	if (Condition{Kind: CondNot, Inner: &inner}).Evaluate(ctx) {
		t.Fatal("Not(true) should be false")
	}
	inner = Condition{Kind: CondDebug, Debug: false}
	if !(Condition{Kind: CondNot, Inner: &inner}).Evaluate(ctx) {
		t.Fatal("Not(false) should be true")
	}
}

func TestAnd(t *testing.T) {
	ctx := setupCtx()
	tt := Condition{Kind: CondDebug, Debug: true}
	ff := Condition{Kind: CondDebug, Debug: false}

	if (Condition{Kind: CondAnd}).Evaluate(ctx) {
		t.Fatal("empty And should be false")
	}
	if !(Condition{Kind: CondAnd, Children: []Condition{tt}}).Evaluate(ctx) {
		t.Fatal("And([true]) should be true")
	}
	if (Condition{Kind: CondAnd, Children: []Condition{ff}}).Evaluate(ctx) {
		t.Fatal("And([false]) should be false")
	}
	if !(Condition{Kind: CondAnd, Children: []Condition{tt, tt}}).Evaluate(ctx) {
		t.Fatal("And([true,true]) should be true")
	}
	if (Condition{Kind: CondAnd, Children: []Condition{tt, ff}}).Evaluate(ctx) {
		t.Fatal("And([true,false]) should be false")
	}
	if (Condition{Kind: CondAnd, Children: []Condition{ff, tt}}).Evaluate(ctx) {
		t.Fatal("And([false,true]) should be false")
	}
	if (Condition{Kind: CondAnd, Children: []Condition{ff, ff}}).Evaluate(ctx) {
		t.Fatal("And([false,false]) should be false")
	}
}

func TestOr(t *testing.T) {
	ctx := setupCtx()
	tt := Condition{Kind: CondDebug, Debug: true}
	ff := Condition{Kind: CondDebug, Debug: false}

	if (Condition{Kind: CondOr}).Evaluate(ctx) {
		t.Fatal("empty Or should be false")
	}
	if !(Condition{Kind: CondOr, Children: []Condition{tt}}).Evaluate(ctx) {
		t.Fatal("Or([true]) should be true")
	}
	if (Condition{Kind: CondOr, Children: []Condition{ff}}).Evaluate(ctx) {
		t.Fatal("Or([false]) should be false")
	}
	if !(Condition{Kind: CondOr, Children: []Condition{tt, tt}}).Evaluate(ctx) {
		t.Fatal("Or([true,true]) should be true")
	}
	if !(Condition{Kind: CondOr, Children: []Condition{tt, ff}}).Evaluate(ctx) {
		t.Fatal("Or([true,false]) should be true")
	}
	if !(Condition{Kind: CondOr, Children: []Condition{ff, tt}}).Evaluate(ctx) {
		t.Fatal("Or([false,true]) should be true")
	}
	if (Condition{Kind: CondOr, Children: []Condition{ff, ff}}).Evaluate(ctx) {
		t.Fatal("Or([false,false]) should be false")
	}
}

func TestCheapToday(t *testing.T) {
	ctx := setupCtx()
	if !(CheapCondition{Hours: 1, From: 2, To: 3}).Evaluate(ctx) {
		t.Fatal("single price always true")
	}
	if (CheapCondition{Hours: 24, From: 3, To: 24}).Evaluate(ctx) {
		t.Fatal("out of range should be false")
	}
	if !(CheapCondition{Hours: 3, From: 0, To: 3}).Evaluate(ctx) {
		t.Fatal("hours=3 in 0-3 should be true")
	}
	if (CheapCondition{Hours: 2, From: 0, To: 3}).Evaluate(ctx) {
		t.Fatal("hours=2 in 0-3 should be false")
	}
}

func TestCheapYesterdayToday(t *testing.T) {
	now, _ := time.Parse("2006-01-02 15:04:05", "2025-02-16 09:43:44")
	prices := []float32{
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
		9, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 1,
	}
	ctx := NewEvaluateContext(now, prices, 24)
	if !(CheapCondition{Hours: 1, From: 23, To: 1}).Evaluate(ctx) {
		t.Fatal("expected cheap to be true with yesterday[23]=10")
	}
	ctx.Prices.Prices[23] = 8.0
	if (CheapCondition{Hours: 1, From: 23, To: 1}).Evaluate(ctx) {
		t.Fatal("expected cheap to be false with yesterday[23]=8")
	}
}

func TestCheapTodayTomorrow(t *testing.T) {
	now, _ := time.Parse("2006-01-02 15:04:05", "2025-02-16 09:43:44")
	prices := []float32{
		1, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 9,
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
	}
	ctx := NewEvaluateContext(now, prices, 23)
	if !(CheapCondition{Hours: 1, From: 23, To: 1}).Evaluate(ctx) {
		t.Fatal("expected cheap to be true")
	}
	ctx.Prices.Prices[24] = 8.0
	if (CheapCondition{Hours: 1, From: 23, To: 1}).Evaluate(ctx) {
		t.Fatal("expected cheap to be false")
	}
}

func TestFindTimeRange(t *testing.T) {
	type tc struct {
		idx           int
		from, to      uint8
		expected      [2]int
		expectedFound bool
	}
	cases := []tc{
		{0, 0, 0, [2]int{}, false},
		{0, 0, 8, [2]int{0, 8}, true},
		{0, 0, 24, [2]int{0, 24}, true},
		{26, 0, 24, [2]int{24, 48}, true},
		{0, 1, 8, [2]int{}, false},
		{23, 23, 24, [2]int{23, 24}, true},
		{24, 23, 24, [2]int{}, false},
		{47, 23, 24, [2]int{47, 48}, true},
		{23, 23, 1, [2]int{23, 25}, true},
		{24, 23, 1, [2]int{23, 25}, true},
		{47, 23, 1, [2]int{47, 49}, true},
		{0, 0, 24, [2]int{0, 24}, true},
		{24, 0, 24, [2]int{24, 48}, true},
	}
	for _, c := range cases {
		got, ok := findTimeRange(c.idx, c.from, c.to)
		if ok != c.expectedFound {
			t.Fatalf("idx=%d from=%d to=%d: ok=%v expected %v", c.idx, c.from, c.to, ok, c.expectedFound)
		}
		if ok && got != c.expected {
			t.Fatalf("idx=%d from=%d to=%d: got %v want %v", c.idx, c.from, c.to, got, c.expected)
		}
	}
}

func TestActualPrice(t *testing.T) {
	now, _ := time.Parse("2006-01-02 15:04:05", "2020-01-01 02:00:00")
	prices := make([]float32, 48)
	for i := range prices {
		prices[i] = float32(i)
	}
	ctx := NewEvaluateContext(now, prices, 26)
	if ctx.ActualPrice() != 26.0 {
		t.Fatalf("expected 26.0, got %v", ctx.ActualPrice())
	}
}

func TestSlice(t *testing.T) {
	now, _ := time.Parse("2006-01-02 15:04:05", "2020-01-01 02:00:00")
	prices := make([]float32, 48)
	for i := range prices {
		prices[i] = float32(i)
	}
	ctx := NewEvaluateContext(now, prices, 26)

	got, ok := ctx.Slice(0, 24)
	if !ok || len(got) != 24 {
		t.Fatalf("expected 24-len slice, got len=%d ok=%v", len(got), ok)
	}
	for i, v := range got {
		if v != float32(24+i) {
			t.Fatalf("idx %d: got %v want %v", i, v, 24+i)
		}
	}

	if _, ok := ctx.Slice(0, 2); ok {
		t.Fatal("expected slice 0..2 with now=26 to be out of range")
	}
	if _, ok := ctx.Slice(3, 24); ok {
		t.Fatal("expected slice 3..24 with now=26 to be out of range")
	}

	got, ok = ctx.Slice(2, 3)
	if !ok || len(got) != 1 || got[0] != 26.0 {
		t.Fatalf("expected [26.0], got %v ok=%v", got, ok)
	}

	ctx.Prices.NowIndex = 23
	got, ok = ctx.Slice(22, 2)
	if !ok {
		t.Fatal("expected slice over midnight to be valid")
	}
	want := []float32{22, 23, 24, 25}
	for i, v := range want {
		if got[i] != v {
			t.Fatalf("idx %d: got %v want %v", i, got[i], v)
		}
	}

	ctx.Prices.NowIndex = 24
	got, _ = ctx.Slice(22, 2)
	for i, v := range want {
		if got[i] != v {
			t.Fatalf("idx %d: got %v want %v", i, got[i], v)
		}
	}

	ctx.Prices.NowIndex = 0
	if _, ok := ctx.Slice(22, 2); ok {
		t.Fatal("expected slice 22..2 at idx 0 to be out of range")
	}
	ctx.Prices.NowIndex = 47
	if _, ok := ctx.Slice(22, 2); ok {
		t.Fatal("expected slice 22..2 at idx 47 to be out of range")
	}
}
