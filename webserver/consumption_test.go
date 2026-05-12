package webserver

import (
	"bytes"
	"io"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/MichalKalita/ote/storage"
)

// farFuture is a "now" anchor used by tests whose data dates are in 2026.
// Passing this to AnalyzeConsumption guarantees no row is filtered as future.
var farFuture = time.Date(2099, 1, 1, 0, 0, 0, 0, time.UTC)

// approxEqual reports whether |a-b| is within tol. Used for float32 cost math
// which only needs to be correct to a couple of decimals.
func approxEqual(a, b, tol float32) bool {
	d := a - b
	if d < 0 {
		d = -d
	}
	return d <= tol
}

func TestParseCSV_HappyPath_StartIsEndMinus15Min(t *testing.T) {
	csv := `"Datum";"Profil +A [kW]";"Status";
"01.04.2026 00:15:00";4;"OK";
"01.04.2026 00:30:00";8;"OK";
`
	got, err := ParseConsumptionCSV(strings.NewReader(csv))
	if err != nil {
		t.Fatalf("ParseConsumptionCSV: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("len: got %d, want 2", len(got))
	}

	loc, _ := time.LoadLocation("Europe/Prague")
	wantStart0 := time.Date(2026, 4, 1, 0, 0, 0, 0, loc).UTC()
	wantStart1 := time.Date(2026, 4, 1, 0, 15, 0, 0, loc).UTC()
	if !got[0].QuarterStart.Equal(wantStart0) {
		t.Errorf("row 0 start: got %v, want %v", got[0].QuarterStart, wantStart0)
	}
	if !got[1].QuarterStart.Equal(wantStart1) {
		t.Errorf("row 1 start: got %v, want %v", got[1].QuarterStart, wantStart1)
	}
	// 4 kW × 0.25 h = 1 kWh per row.
	if !approxEqual(got[0].KWh, 1.0, 1e-6) {
		t.Errorf("row 0 kWh: got %v, want 1.0", got[0].KWh)
	}
	if !approxEqual(got[1].KWh, 2.0, 1e-6) {
		t.Errorf("row 1 kWh: got %v, want 2.0", got[1].KWh)
	}
}

func TestParseCSV_24Hour_AnchorsToLastQuarterOfSameDay(t *testing.T) {
	// "01.04.2026 24:00:00" is the END of the last quarter of 2026-04-01.
	// Its QuarterStart must therefore be 2026-04-01 23:45 Prague-local, i.e.,
	// the 95th quarter of the day (idx 95), not 00:00 of the next day.
	csv := `"Datum";"Profil +A [kW]";"Status";
"01.04.2026 24:00:00";4;"OK";
`
	got, err := ParseConsumptionCSV(strings.NewReader(csv))
	if err != nil {
		t.Fatalf("ParseConsumptionCSV: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("len: got %d, want 1", len(got))
	}
	loc, _ := time.LoadLocation("Europe/Prague")
	want := time.Date(2026, 4, 1, 23, 45, 0, 0, loc).UTC()
	if !got[0].QuarterStart.Equal(want) {
		t.Errorf("24:00 start: got %v, want %v", got[0].QuarterStart, want)
	}
}

func TestParseCSV_RejectsInvalidTimestamp(t *testing.T) {
	csv := `"Datum";"Profil +A [kW]";"Status";
"not-a-date";1;"OK";
`
	if _, err := ParseConsumptionCSV(strings.NewReader(csv)); err == nil {
		t.Fatal("expected error for invalid timestamp, got nil")
	}
}

func TestParseCSV_AcceptsCommaDecimal(t *testing.T) {
	// Some locales export decimals with comma. Accept both.
	csv := `"Datum";"Profil +A [kW]";"Status";
"01.04.2026 00:15:00";"4,8";"OK";
`
	got, err := ParseConsumptionCSV(strings.NewReader(csv))
	if err != nil {
		t.Fatalf("ParseConsumptionCSV: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("len: got %d, want 1", len(got))
	}
	// 4.8 kW × 0.25 h = 1.2 kWh.
	if !approxEqual(got[0].KWh, 1.2, 1e-5) {
		t.Errorf("kWh: got %v, want 1.2", got[0].KWh)
	}
}

// AnalyzeConsumption: with flat consumption (1 kWh every quarter), the
// rearrangement inequality collapses — any reordering of equal consumption
// values produces the same total cost. So WeightedPrice = FlatPrice =
// BestPrice = WorstPrice, and Score is 1.0 (nothing to optimize). This
// encodes the "flat baseline" definition: a household that can't shift its
// load has the same actual / best / worst, not a 50/50 mix.
func TestAnalyze_FlatConsumption_WeightedEqualsFlatAvg(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")
	day := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)

	// Prices: 0, 1, 2, ..., 95 EUR/MWh.
	quarters := make([]storage.Quarter, 96)
	for i := range quarters {
		quarters[i] = storage.Quarter{
			Ts:    day.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			Price: float32(i),
		}
	}
	if err := state.db.SaveQuarters(quarters); err != nil {
		t.Fatalf("seed: %v", err)
	}

	cons := make([]ConsumptionQuarter, 96)
	for i := range cons {
		cons[i] = ConsumptionQuarter{
			QuarterStart: day.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			KWh:          1.0,
		}
	}

	analysis, err := state.AnalyzeConsumption(cons, farFuture)
	if err != nil {
		t.Fatalf("AnalyzeConsumption: %v", err)
	}
	if len(analysis.PerDay) != 1 {
		t.Fatalf("PerDay: got %d days, want 1", len(analysis.PerDay))
	}
	d := analysis.PerDay[0]
	// Mean of 0..95 = 47.5
	wantAvg := float32(47.5)
	if !approxEqual(d.WeightedPrice, wantAvg, 0.01) {
		t.Errorf("WeightedPrice: got %v, want %v", d.WeightedPrice, wantAvg)
	}
	if !approxEqual(d.FlatPrice, wantAvg, 0.01) {
		t.Errorf("FlatPrice: got %v, want %v", d.FlatPrice, wantAvg)
	}
	// With identical consumption per quarter, reordering does nothing — all
	// three prices collapse to the flat average.
	if !approxEqual(d.BestPrice, wantAvg, 0.01) {
		t.Errorf("BestPrice for flat consumption should equal flat avg; got %v want %v", d.BestPrice, wantAvg)
	}
	if !approxEqual(d.WorstPrice, wantAvg, 0.01) {
		t.Errorf("WorstPrice for flat consumption should equal flat avg; got %v want %v", d.WorstPrice, wantAvg)
	}
	if !approxEqual(d.Score, 1.0, 0.001) {
		t.Errorf("Score for flat consumption should be 1.0 (no optimization possible); got %v", d.Score)
	}
}

// AnalyzeConsumption: if all consumption happens in the cheapest quarter, the
// weighted price equals the minimum, the best matches it, and the score is 1.0.
func TestAnalyze_AllInCheapestQuarter_PerfectScore(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")
	day := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)

	quarters := make([]storage.Quarter, 96)
	for i := range quarters {
		quarters[i] = storage.Quarter{
			Ts:    day.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			Price: float32(i), // cheapest at idx 0
		}
	}
	if err := state.db.SaveQuarters(quarters); err != nil {
		t.Fatalf("seed: %v", err)
	}

	cons := []ConsumptionQuarter{{
		QuarterStart: day.UTC(),
		KWh:          10.0,
	}}

	analysis, err := state.AnalyzeConsumption(cons, farFuture)
	if err != nil {
		t.Fatalf("AnalyzeConsumption: %v", err)
	}
	d := analysis.PerDay[0]
	if !approxEqual(d.WeightedPrice, 0, 0.01) {
		t.Errorf("WeightedPrice on cheapest quarter: got %v, want 0", d.WeightedPrice)
	}
	if !approxEqual(d.BestPrice, 0, 0.01) {
		t.Errorf("BestPrice with single quarter consumption should equal min price 0; got %v", d.BestPrice)
	}
	if !approxEqual(d.Score, 1.0, 0.001) {
		t.Errorf("Score should be 1.0 (best possible); got %v", d.Score)
	}
}

// AnalyzeConsumption: same setup but consumption sits on the most expensive
// quarter — score must be 0.
func TestAnalyze_AllInExpensiveQuarter_ZeroScore(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")
	day := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)

	quarters := make([]storage.Quarter, 96)
	for i := range quarters {
		quarters[i] = storage.Quarter{
			Ts:    day.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			Price: float32(i),
		}
	}
	if err := state.db.SaveQuarters(quarters); err != nil {
		t.Fatalf("seed: %v", err)
	}

	cons := []ConsumptionQuarter{{
		QuarterStart: day.Add(95 * 15 * time.Minute).UTC(), // most expensive
		KWh:          10.0,
	}}

	analysis, err := state.AnalyzeConsumption(cons, farFuture)
	if err != nil {
		t.Fatalf("AnalyzeConsumption: %v", err)
	}
	d := analysis.PerDay[0]
	if !approxEqual(d.WeightedPrice, 95, 0.01) {
		t.Errorf("WeightedPrice on max quarter: got %v, want 95", d.WeightedPrice)
	}
	if !approxEqual(d.WorstPrice, 95, 0.01) {
		t.Errorf("WorstPrice: got %v, want 95", d.WorstPrice)
	}
	if !approxEqual(d.Score, 0, 0.001) {
		t.Errorf("Score on most expensive quarter should be 0; got %v", d.Score)
	}
}

// AnalyzeConsumption: rearrangement inequality. If big consumption sits on
// cheap prices, the actual weighted price equals BestPrice and Score = 1.
// If big consumption sits on expensive prices, weighted = WorstPrice and Score = 0.
func TestAnalyze_RearrangementInequality(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")
	day := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)

	// Two-quarter day to make the math trivial.
	quarters := []storage.Quarter{
		{Ts: day.UTC(), Price: 10},
		{Ts: day.Add(15 * time.Minute).UTC(), Price: 100},
	}
	if err := state.db.SaveQuarters(quarters); err != nil {
		t.Fatalf("seed: %v", err)
	}

	// Big consumption on the cheap quarter → optimal.
	consGood := []ConsumptionQuarter{
		{QuarterStart: day.UTC(), KWh: 9},
		{QuarterStart: day.Add(15 * time.Minute).UTC(), KWh: 1},
	}
	a, err := state.AnalyzeConsumption(consGood, farFuture)
	if err != nil {
		t.Fatalf("good: %v", err)
	}
	d := a.PerDay[0]
	// cost_good = 9*10 + 1*100 = 190 EUR/MWh per 10 kWh → 19 EUR/MWh.
	// Wait: cost is in EUR (price[EUR/MWh] * kWh / 1000). Re-compute via weighted.
	// weighted = (9*10 + 1*100)/10 = 190/10 = 19 EUR/MWh.
	if !approxEqual(d.WeightedPrice, 19, 0.001) {
		t.Errorf("good case weighted: got %v, want 19", d.WeightedPrice)
	}
	if !approxEqual(d.BestPrice, 19, 0.001) {
		t.Errorf("good case best (same sorted ordering): got %v, want 19", d.BestPrice)
	}
	if !approxEqual(d.Score, 1.0, 0.001) {
		t.Errorf("good case score: got %v, want 1.0", d.Score)
	}

	// Same consumption pattern, mirrored: big on expensive quarter.
	consBad := []ConsumptionQuarter{
		{QuarterStart: day.UTC(), KWh: 1},
		{QuarterStart: day.Add(15 * time.Minute).UTC(), KWh: 9},
	}
	a2, err := state.AnalyzeConsumption(consBad, farFuture)
	if err != nil {
		t.Fatalf("bad: %v", err)
	}
	d2 := a2.PerDay[0]
	// weighted = (1*10 + 9*100)/10 = 910/10 = 91 EUR/MWh.
	if !approxEqual(d2.WeightedPrice, 91, 0.001) {
		t.Errorf("bad case weighted: got %v, want 91", d2.WeightedPrice)
	}
	if !approxEqual(d2.WorstPrice, 91, 0.001) {
		t.Errorf("bad case worst (sorted desc pairing): got %v, want 91", d2.WorstPrice)
	}
	if !approxEqual(d2.Score, 0, 0.001) {
		t.Errorf("bad case score: got %v, want 0", d2.Score)
	}
}

// /consumption POST end-to-end: uploads a CSV multipart, expects an HTML
// response containing the per-day numbers we can predict from a fixture.
func TestRoute_Consumption_PostRendersAnalysis(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")

	day := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)
	quarters := make([]storage.Quarter, 96)
	for i := range quarters {
		quarters[i] = storage.Quarter{
			Ts:    day.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			Price: float32(i),
		}
	}
	if err := state.db.SaveQuarters(quarters); err != nil {
		t.Fatalf("seed: %v", err)
	}
	// Fixture used by AnalyzeConsumption's prefetch (DB hit, but the AppState may
	// still call GetPrices which checks HasDay first → no HTTP traffic expected).
	cleanup, _ := startOTEFixture(t, func(string) ([]float32, bool) { return fixedPrices(96), true })
	defer cleanup()

	// Build a CSV with 4 quarters in one hour: 1, 2, 3, 4 kW (= 0.25, 0.5, 0.75, 1.0 kWh).
	csv := `"Datum";"Profil +A [kW]";"Status";
"10.05.2026 00:15:00";1;"OK";
"10.05.2026 00:30:00";2;"OK";
"10.05.2026 00:45:00";3;"OK";
"10.05.2026 01:00:00";4;"OK";
`
	body, contentType := buildMultipart(t, "csv", "test.csv", csv)

	handler := buildTestHandler(state)
	req := httptest.NewRequest(http.MethodPost, "/consumption", body)
	req.Header.Set("Content-Type", contentType)
	rr := httptest.NewRecorder()
	handler.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", rr.Code)
	}
	resp := readBody(t, rr.Result())
	for _, needle := range []string{
		"Consumption analysis",
		"2026-05-10",
		"Summary",
		"Per day",
	} {
		if !strings.Contains(resp, needle) {
			t.Errorf("response missing %q", needle)
		}
	}

	// Expected weighted price: prices used are price[0..3] = 0, 1, 2, 3.
	// kWh = 0.25, 0.5, 0.75, 1.0; total = 2.5 kWh.
	// cost (EUR) = (0*0.25 + 1*0.5 + 2*0.75 + 3*1.0)/1000 = (0+0.5+1.5+3.0)/1000 = 5.0/1000 = 0.005
	// weighted = 0.005 * 1000 / 2.5 = 2.0 EUR/MWh.
	// The summary card renders one decimal in EUR → "2.0".
	if !strings.Contains(resp, "2.0") {
		t.Errorf("response missing expected weighted price 2.0 EUR/MWh:\n%s", abbreviate(resp))
	}
}

func TestRoute_Consumption_GetRendersUploadForm(t *testing.T) {
	state := openTestState(t)
	cleanup, _ := startOTEFixture(t, func(string) ([]float32, bool) { return fixedPrices(96), true })
	defer cleanup()

	handler := buildTestHandler(state)
	req := httptest.NewRequest(http.MethodGet, "/consumption", nil)
	rr := httptest.NewRecorder()
	handler.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200", rr.Code)
	}
	body := readBody(t, rr.Result())
	for _, needle := range []string{
		`enctype="multipart/form-data"`,
		`name="csv"`,
		`type="file"`,
		"not stored on the server",
	} {
		if !strings.Contains(body, needle) {
			t.Errorf("upload form missing %q", needle)
		}
	}
}

// buildMultipart returns a request body + content type containing a single
// uploaded file field.
func buildMultipart(t *testing.T, fieldName, fileName, content string) (io.Reader, string) {
	t.Helper()
	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)
	part, err := w.CreateFormFile(fieldName, fileName)
	if err != nil {
		t.Fatalf("CreateFormFile: %v", err)
	}
	if _, err := part.Write([]byte(content)); err != nil {
		t.Fatalf("write part: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("close writer: %v", err)
	}
	return &buf, w.FormDataContentType()
}

// abbreviate returns a short prefix of s for error messages.
func abbreviate(s string) string {
	const max = 400
	if len(s) <= max {
		return s
	}
	return s[:max] + "..."
}

// Sanity: 24-hour timestamps from two distinct days produce distinct date keys.
func TestParseCSV_BoundaryEndOfDayBelongsToCurrentDay(t *testing.T) {
	csv := `"Datum";"Profil +A [kW]";"Status";
"01.04.2026 24:00:00";4;"OK";
"02.04.2026 00:15:00";4;"OK";
`
	got, err := ParseConsumptionCSV(strings.NewReader(csv))
	if err != nil {
		t.Fatalf("ParseConsumptionCSV: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("len: got %d, want 2", len(got))
	}
	loc, _ := time.LoadLocation("Europe/Prague")
	// First row's QuarterStart is 2026-04-01 23:45 local → grouped under 2026-04-01.
	d0 := got[0].QuarterStart.In(loc).Format("2006-01-02")
	if d0 != "2026-04-01" {
		t.Errorf("24:00:00 row grouped under %s, want 2026-04-01", d0)
	}
	d1 := got[1].QuarterStart.In(loc).Format("2006-01-02")
	if d1 != "2026-04-02" {
		t.Errorf("next-day row grouped under %s, want 2026-04-02", d1)
	}
	// Distinct days produce different formatted keys.
	if d0 == d1 {
		t.Errorf("rows should land on distinct days; both = %s", d0)
	}
}

// AnalyzeConsumption filters out any Prague-local date strictly after the
// max-knowable OTE date. Before 14:00 only "today" is knowable; from 14:00
// "today + 1" is also knowable (because OTE publishes the next day's prices
// at NextDayPricesHour). The day after that is always future and must be
// dropped without an HTTP request. The fixture below fails the test if any
// such request is made.
func TestAnalyze_FutureDates_AreFilteredWithoutFetch(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")

	// "Now" anchor: 2026-05-12 12:00 Prague (before 14:00 → only today is knowable).
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, loc)
	today := time.Date(2026, 5, 12, 0, 0, 0, 0, loc)
	tomorrow := today.AddDate(0, 0, 1)
	dayAfter := today.AddDate(0, 0, 2)

	// Pre-seed today's prices so the cache hit is satisfied without HTTP.
	quarters := make([]storage.Quarter, 96)
	for i := range quarters {
		quarters[i] = storage.Quarter{
			Ts:    today.Add(time.Duration(i) * 15 * time.Minute).UTC(),
			Price: 50.0,
		}
	}
	if err := state.db.SaveQuarters(quarters); err != nil {
		t.Fatalf("seed: %v", err)
	}

	cleanup, hits := startOTEFixture(t, func(reportDate string) ([]float32, bool) {
		// Any fetch for tomorrow or later is a bug. Today is allowed (pre-seeded
		// so it shouldn't fetch either, but be tolerant of cache-warm semantics).
		if reportDate == "2026-05-13" || reportDate == "2026-05-14" {
			t.Errorf("unexpected OTE fetch for future date %s", reportDate)
		}
		return fixedPrices(96), true
	})
	defer cleanup()

	cons := []ConsumptionQuarter{
		{QuarterStart: today.UTC(), KWh: 1},
		{QuarterStart: tomorrow.UTC(), KWh: 1},
		{QuarterStart: dayAfter.UTC(), KWh: 1},
	}
	analysis, err := state.AnalyzeConsumption(cons, now)
	if err != nil {
		t.Fatalf("AnalyzeConsumption: %v", err)
	}

	// Today is analyzed; tomorrow and day-after are filtered as future.
	if len(analysis.PerDay) != 1 {
		t.Errorf("PerDay: got %d, want 1 (only today)", len(analysis.PerDay))
	}
	wantFuture := map[string]bool{"2026-05-13": true, "2026-05-14": true}
	gotFuture := map[string]bool{}
	for _, k := range analysis.FutureDates {
		gotFuture[k] = true
	}
	for k := range wantFuture {
		if !gotFuture[k] {
			t.Errorf("expected %s in FutureDates, got %v", k, analysis.FutureDates)
		}
	}
	// No fetches for future dates is the strict contract; *hits == 0 is the
	// strongest assertion (today was pre-seeded → no fetch needed).
	if *hits != 0 {
		t.Errorf("expected 0 OTE fetches, got %d", *hits)
	}
}

// AnalyzeConsumption from 14:00 onwards allows "today + 1" but never beyond.
func TestAnalyze_AfterPublishHour_TomorrowAllowed_DayAfterStillFuture(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")

	// "Now": 2026-05-12 14:00 Prague — tomorrow's prices are now knowable.
	now := time.Date(2026, 5, 12, 14, 0, 0, 0, loc)
	today := time.Date(2026, 5, 12, 0, 0, 0, 0, loc)
	tomorrow := today.AddDate(0, 0, 1)
	dayAfter := today.AddDate(0, 0, 2)

	// Pre-seed both today and tomorrow so we don't depend on HTTP.
	for _, d := range []time.Time{today, tomorrow} {
		qs := make([]storage.Quarter, 96)
		for i := range qs {
			qs[i] = storage.Quarter{
				Ts:    d.Add(time.Duration(i) * 15 * time.Minute).UTC(),
				Price: 50.0,
			}
		}
		if err := state.db.SaveQuarters(qs); err != nil {
			t.Fatalf("seed: %v", err)
		}
	}

	cleanup, _ := startOTEFixture(t, func(reportDate string) ([]float32, bool) {
		if reportDate == "2026-05-14" {
			t.Errorf("must not fetch day-after-tomorrow %s", reportDate)
		}
		return fixedPrices(96), true
	})
	defer cleanup()

	cons := []ConsumptionQuarter{
		{QuarterStart: today.UTC(), KWh: 1},
		{QuarterStart: tomorrow.UTC(), KWh: 1},
		{QuarterStart: dayAfter.UTC(), KWh: 1},
	}
	analysis, err := state.AnalyzeConsumption(cons, now)
	if err != nil {
		t.Fatalf("AnalyzeConsumption: %v", err)
	}
	if len(analysis.PerDay) != 2 {
		t.Errorf("PerDay: got %d, want 2 (today + tomorrow)", len(analysis.PerDay))
	}
	if len(analysis.FutureDates) != 1 || analysis.FutureDates[0] != "2026-05-14" {
		t.Errorf("FutureDates: got %v, want [2026-05-14]", analysis.FutureDates)
	}
}

// AnalyzeConsumption returns an error (not silent success) when every row
// is in the future — the user would otherwise see an empty results page.
func TestAnalyze_AllFuture_ReturnsError(t *testing.T) {
	state := openTestState(t)
	loc, _ := time.LoadLocation("Europe/Prague")
	now := time.Date(2026, 5, 12, 10, 0, 0, 0, loc)
	dayAfter := time.Date(2026, 5, 14, 0, 0, 0, 0, loc)

	cons := []ConsumptionQuarter{
		{QuarterStart: dayAfter.UTC(), KWh: 1},
	}
	_, err := state.AnalyzeConsumption(cons, now)
	if err == nil {
		t.Fatal("expected error when all rows are future-dated, got nil")
	}
	if !strings.Contains(err.Error(), "future") {
		t.Errorf("error should mention 'future'; got %q", err.Error())
	}
}

