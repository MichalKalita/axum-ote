package storage

import (
	"path/filepath"
	"testing"
	"time"
)

// openTestDB opens a fresh SQLite file in t.TempDir(). The DB is closed automatically.
func openTestDB(t *testing.T) *DB {
	t.Helper()
	path := filepath.Join(t.TempDir(), "test.db")
	db, err := Open(path)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func mustLoadPrague(t *testing.T) *time.Location {
	t.Helper()
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		t.Fatalf("load location: %v", err)
	}
	return loc
}

// makeQuarters builds n consecutive 15-minute quarters starting at startPrague (Prague-local).
// Returned timestamps are in UTC.
func makeQuarters(startPrague time.Time, n int, priceFn func(i int) float32) []Quarter {
	out := make([]Quarter, n)
	for i := 0; i < n; i++ {
		out[i] = Quarter{
			Ts:    startPrague.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			Price: priceFn(i),
		}
	}
	return out
}

func TestSaveAndGet_RoundTripsAllQuarters(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	// 2026-05-10 in Prague (UTC+2), 96 quarters
	start := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)
	in := makeQuarters(start, 96, func(i int) float32 { return float32(i) * 0.5 })

	if err := db.SaveQuarters(in); err != nil {
		t.Fatalf("SaveQuarters: %v", err)
	}

	out, err := db.GetDay("2026-05-10")
	if err != nil {
		t.Fatalf("GetDay: %v", err)
	}
	if len(out) != 96 {
		t.Fatalf("expected 96 quarters, got %d", len(out))
	}
	for i, q := range out {
		if !q.Ts.Equal(in[i].Ts) {
			t.Errorf("ts mismatch at %d: got %v, want %v", i, q.Ts, in[i].Ts)
		}
		if q.Price != in[i].Price {
			t.Errorf("price mismatch at %d: got %f, want %f", i, q.Price, in[i].Price)
		}
	}
}

func TestHasDay_ReflectsPersistence(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	has, err := db.HasDay("2026-05-10")
	if err != nil {
		t.Fatalf("HasDay before save: %v", err)
	}
	if has {
		t.Fatal("HasDay returned true on empty DB")
	}

	start := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)
	if err := db.SaveQuarters(makeQuarters(start, 96, func(i int) float32 { return 1 })); err != nil {
		t.Fatalf("SaveQuarters: %v", err)
	}

	has, err = db.HasDay("2026-05-10")
	if err != nil {
		t.Fatalf("HasDay after save: %v", err)
	}
	if !has {
		t.Fatal("HasDay returned false after SaveQuarters")
	}
}

func TestMonthAverages_ComputesPerDayMean(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	// Day A: all prices = 10 → avg 10
	// Day B: prices 0..95 → avg 47.5
	// Day C: not saved → absent from result
	dayA := time.Date(2026, 5, 1, 0, 0, 0, 0, loc)
	dayB := time.Date(2026, 5, 2, 0, 0, 0, 0, loc)
	if err := db.SaveQuarters(makeQuarters(dayA, 96, func(i int) float32 { return 10 })); err != nil {
		t.Fatal(err)
	}
	if err := db.SaveQuarters(makeQuarters(dayB, 96, func(i int) float32 { return float32(i) })); err != nil {
		t.Fatal(err)
	}

	avgs, err := db.MonthAverages("2026-05-01", "2026-05-03")
	if err != nil {
		t.Fatalf("MonthAverages: %v", err)
	}
	if got, want := avgs["2026-05-01"], float32(10); got != want {
		t.Errorf("day A avg: got %v, want %v", got, want)
	}
	if got, want := avgs["2026-05-02"], float32(47.5); got != want {
		t.Errorf("day B avg: got %v, want %v", got, want)
	}
	if _, ok := avgs["2026-05-03"]; ok {
		t.Errorf("day C should be absent (not saved), got entry: %v", avgs["2026-05-03"])
	}
	if len(avgs) != 2 {
		t.Errorf("expected 2 entries, got %d: %v", len(avgs), avgs)
	}
}

func TestMonthAverages_RangeBoundsAreInclusive(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	// Save 3 days: 04-30, 05-01, 05-31. Query 05-01..05-31 should hit only the latter two.
	for _, d := range []time.Time{
		time.Date(2026, 4, 30, 0, 0, 0, 0, loc),
		time.Date(2026, 5, 1, 0, 0, 0, 0, loc),
		time.Date(2026, 5, 31, 0, 0, 0, 0, loc),
	} {
		if err := db.SaveQuarters(makeQuarters(d, 96, func(int) float32 { return 1 })); err != nil {
			t.Fatal(err)
		}
	}

	avgs, err := db.MonthAverages("2026-05-01", "2026-05-31")
	if err != nil {
		t.Fatalf("MonthAverages: %v", err)
	}
	if _, ok := avgs["2026-04-30"]; ok {
		t.Error("2026-04-30 should be outside range")
	}
	if _, ok := avgs["2026-05-01"]; !ok {
		t.Error("2026-05-01 should be inside range (lower bound inclusive)")
	}
	if _, ok := avgs["2026-05-31"]; !ok {
		t.Error("2026-05-31 should be inside range (upper bound inclusive)")
	}
}

func TestPragueDate_AcrossDSTBoundary(t *testing.T) {
	db := openTestDB(t)

	// 2026-03-29 02:30 Prague does not exist (spring-forward 02:00→03:00).
	// 2026-03-29 00:30 Prague = 2026-03-28 23:30 UTC (winter UTC+1)
	// 2026-03-29 12:00 Prague = 2026-03-29 10:00 UTC (summer UTC+2)
	wintertime := time.Date(2026, 3, 28, 23, 30, 0, 0, time.UTC)
	summertime := time.Date(2026, 3, 29, 10, 0, 0, 0, time.UTC)

	if got := db.PragueDate(wintertime); got != "2026-03-29" {
		t.Errorf("wintertime 00:30 Prague: got date %q, want %q", got, "2026-03-29")
	}
	if got := db.PragueDate(summertime); got != "2026-03-29" {
		t.Errorf("summertime 12:00 Prague: got date %q, want %q", got, "2026-03-29")
	}

	// Just-before-midnight UTC must round to next Prague day in summer.
	// 2026-05-10 22:30 UTC = 2026-05-11 00:30 Prague (CEST = UTC+2).
	late := time.Date(2026, 5, 10, 22, 30, 0, 0, time.UTC)
	if got := db.PragueDate(late); got != "2026-05-11" {
		t.Errorf("summer late UTC: got %q, want 2026-05-11", got)
	}
}

func TestDSTSpringDay_Stores92Quarters(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	// 2026-03-29 in Prague has 23 hours (clock skips 02:00→03:00). 23*4 = 92 quarters.
	start := time.Date(2026, 3, 29, 0, 0, 0, 0, loc)
	quarters := makeQuarters(start, 92, func(i int) float32 { return float32(i) })

	if err := db.SaveQuarters(quarters); err != nil {
		t.Fatalf("SaveQuarters: %v", err)
	}

	got, err := db.GetDay("2026-03-29")
	if err != nil {
		t.Fatalf("GetDay: %v", err)
	}
	if len(got) != 92 {
		t.Fatalf("DST spring day should store 92 quarters, got %d", len(got))
	}

	// Timestamps must be strictly increasing (no duplicates from DST skip).
	for i := 1; i < len(got); i++ {
		if !got[i].Ts.After(got[i-1].Ts) {
			t.Errorf("ts not strictly increasing at %d: %v <= %v", i, got[i].Ts, got[i-1].Ts)
		}
	}
}

func TestDSTAutumnDay_Stores100Quarters_WithBothOccurrencesOf02h(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	// 2025-10-26 in Prague has 25 hours (02:00 happens twice). 25*4 = 100 quarters.
	// Iterating via 15-min Add from midnight is the same logic dataloader uses.
	start := time.Date(2025, 10, 26, 0, 0, 0, 0, loc)
	quarters := makeQuarters(start, 100, func(i int) float32 { return float32(i) })

	if err := db.SaveQuarters(quarters); err != nil {
		t.Fatalf("SaveQuarters: %v", err)
	}

	got, err := db.GetDay("2025-10-26")
	if err != nil {
		t.Fatalf("GetDay: %v", err)
	}
	if len(got) != 100 {
		t.Fatalf("DST autumn day should store 100 quarters, got %d", len(got))
	}

	// Both "02:30 Prague" occurrences must be present as distinct UTC instants.
	// First 02:30 = 00:30 UTC, second 02:30 = 01:30 UTC (offsets +2 → +1).
	first := time.Date(2025, 10, 26, 0, 30, 0, 0, time.UTC)
	second := time.Date(2025, 10, 26, 1, 30, 0, 0, time.UTC)
	var foundFirst, foundSecond bool
	for _, q := range got {
		if q.Ts.Equal(first) {
			foundFirst = true
		}
		if q.Ts.Equal(second) {
			foundSecond = true
		}
	}
	if !foundFirst || !foundSecond {
		t.Errorf("expected both 02:30 occurrences in UTC (00:30, 01:30); foundFirst=%v foundSecond=%v",
			foundFirst, foundSecond)
	}
}

func TestSaveQuarters_OverwriteSameTimestamp(t *testing.T) {
	db := openTestDB(t)
	loc := mustLoadPrague(t)

	start := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)
	if err := db.SaveQuarters(makeQuarters(start, 96, func(int) float32 { return 1 })); err != nil {
		t.Fatal(err)
	}
	// Re-save with different prices — INSERT OR REPLACE keeps the second.
	if err := db.SaveQuarters(makeQuarters(start, 96, func(int) float32 { return 99 })); err != nil {
		t.Fatal(err)
	}

	out, _ := db.GetDay("2026-05-10")
	for i, q := range out {
		if q.Price != 99 {
			t.Errorf("after re-save, quarter %d price = %v, want 99", i, q.Price)
		}
	}
}

func TestSaveQuarters_Empty(t *testing.T) {
	db := openTestDB(t)
	// Empty slice must be a no-op (no transaction overhead, no error).
	if err := db.SaveQuarters(nil); err != nil {
		t.Errorf("SaveQuarters(nil): %v", err)
	}
	if err := db.SaveQuarters([]Quarter{}); err != nil {
		t.Errorf("SaveQuarters([]): %v", err)
	}
}
