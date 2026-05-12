package dataloader

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// otePayload builds a JSON body in the exact shape the real OTE endpoint
// returns. The 15-minute price series is the only dataLine consumed; we include
// a decoy line to confirm the parser picks the right one by title.
func otePayload(prices []float32, includeTarget bool) []byte {
	type pt struct {
		Y float32 `json:"y"`
	}
	type line struct {
		Title string `json:"title"`
		Point []pt   `json:"point"`
	}
	points := make([]pt, len(prices))
	for i, p := range prices {
		points[i] = pt{Y: p}
	}

	lines := []line{
		{Title: "Hourly average (EUR/MWh)", Point: []pt{{Y: 1}, {Y: 2}}}, // decoy
	}
	if includeTarget {
		lines = append(lines, line{Title: "15min price (EUR/MWh)", Point: points})
	}

	body, _ := json.Marshal(map[string]any{
		"data": map[string]any{"dataLine": lines},
	})
	return body
}

// startOTEServer returns a httptest.Server whose handler is `handler` and rewires
// dataloader.BaseURL to point at it. Original BaseURL is restored on cleanup.
func startOTEServer(t *testing.T, handler http.HandlerFunc) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(handler)
	prev := BaseURL
	BaseURL = srv.URL
	t.Cleanup(func() {
		BaseURL = prev
		srv.Close()
	})
	return srv
}

func mustPragueLoc(t *testing.T) *time.Location {
	t.Helper()
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		t.Fatalf("load location: %v", err)
	}
	return loc
}

func TestFetchData_NormalDay96Quarters(t *testing.T) {
	loc := mustPragueLoc(t)
	prices := make([]float32, 96)
	for i := range prices {
		prices[i] = float32(i) * 0.25
	}

	var gotPath, gotQuery string
	startOTEServer(t, func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotQuery = r.URL.RawQuery
		w.Write(otePayload(prices, true))
	})

	date := time.Date(2026, 5, 10, 0, 0, 0, 0, loc)
	got, err := FetchData(date)
	if err != nil {
		t.Fatalf("FetchData: %v", err)
	}

	// HTTP-level: the loader hit the documented endpoint with the date query param.
	if !strings.Contains(gotPath, "") || !strings.Contains(gotQuery, "report_date=2026-05-10") {
		t.Errorf("unexpected request: path=%q query=%q", gotPath, gotQuery)
	}

	if len(got) != 96 {
		t.Fatalf("want 96 quarters, got %d", len(got))
	}
	// Each price round-trips into the corresponding Quarter.
	for i, q := range got {
		if q.Price != prices[i] {
			t.Errorf("price[%d]: got %v, want %v", i, q.Price, prices[i])
		}
	}
	// First quarter is Prague-midnight in UTC; in May Prague is UTC+2, so 2026-05-09 22:00 UTC.
	wantFirst := time.Date(2026, 5, 9, 22, 0, 0, 0, time.UTC)
	if !got[0].Ts.Equal(wantFirst) {
		t.Errorf("first ts: got %v, want %v", got[0].Ts, wantFirst)
	}
	// Quarters are 15 min apart, ordered, all UTC.
	for i := 1; i < len(got); i++ {
		if got[i].Ts.Sub(got[i-1].Ts) != 15*time.Minute {
			t.Errorf("gap at %d: %v", i, got[i].Ts.Sub(got[i-1].Ts))
		}
	}
}

func TestFetchData_DSTSpringDay_Returns92(t *testing.T) {
	loc := mustPragueLoc(t)
	prices := make([]float32, 92)
	for i := range prices {
		prices[i] = float32(i)
	}
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		w.Write(otePayload(prices, true))
	})

	date := time.Date(2026, 3, 29, 0, 0, 0, 0, loc)
	got, err := FetchData(date)
	if err != nil {
		t.Fatalf("FetchData: %v", err)
	}
	if len(got) != 92 {
		t.Fatalf("DST spring day expected 92 quarters, got %d", len(got))
	}
}

func TestFetchData_DSTAutumnDay_Returns100(t *testing.T) {
	loc := mustPragueLoc(t)
	prices := make([]float32, 100)
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		w.Write(otePayload(prices, true))
	})

	date := time.Date(2025, 10, 26, 0, 0, 0, 0, loc)
	got, err := FetchData(date)
	if err != nil {
		t.Fatalf("FetchData: %v", err)
	}
	if len(got) != 100 {
		t.Fatalf("DST autumn day expected 100 quarters, got %d", len(got))
	}
}

func TestFetchData_404ReturnsUnexpectedStatus(t *testing.T) {
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		http.NotFound(w, &http.Request{})
	})
	_, err := FetchData(time.Date(2026, 5, 10, 0, 0, 0, 0, time.UTC))
	if err == nil {
		t.Fatal("expected error on 404")
	}
	uerr, ok := err.(*UnexpectedStatusError)
	if !ok {
		t.Fatalf("want *UnexpectedStatusError, got %T: %v", err, err)
	}
	if uerr.Status != http.StatusNotFound {
		t.Errorf("status: got %d, want 404", uerr.Status)
	}
}

func TestFetchData_MissingPriceLineReturnsErrPriceDataNotFound(t *testing.T) {
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		w.Write(otePayload(nil, false)) // payload without the "15min price" line
	})
	_, err := FetchData(time.Date(2026, 5, 10, 0, 0, 0, 0, time.UTC))
	if err != ErrPriceDataNotFound {
		t.Errorf("got %v, want ErrPriceDataNotFound", err)
	}
}

func TestFetchData_InvalidJSONReturnsError(t *testing.T) {
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		fmt.Fprint(w, "this is not JSON {{{")
	})
	_, err := FetchData(time.Date(2026, 5, 10, 0, 0, 0, 0, time.UTC))
	if err == nil {
		t.Fatal("expected JSON parse error")
	}
	if !strings.Contains(err.Error(), "JSON parsing error") {
		t.Errorf("want JSON parsing error, got %v", err)
	}
}

func TestFetchData_BeforeCutoffReturnsErrorAndSkipsRequest(t *testing.T) {
	loc := mustPragueLoc(t)
	var hit bool
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		hit = true
		w.Write(otePayload(nil, true))
	})

	// 2025-09-30 Prague is the last day before OTE's 15-minute series.
	_, err := FetchData(time.Date(2025, 9, 30, 0, 0, 0, 0, loc))
	if err != ErrDateBeforeQuarterHourly {
		t.Fatalf("got %v, want ErrDateBeforeQuarterHourly", err)
	}
	if hit {
		t.Error("FetchData must not issue an HTTP request for dates before the cutoff")
	}
}

func TestFetchData_CutoffDayAllowed(t *testing.T) {
	loc := mustPragueLoc(t)
	prices := make([]float32, 96)
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		w.Write(otePayload(prices, true))
	})
	// 2025-10-01 Prague — the first allowed day.
	_, err := FetchData(time.Date(2025, 10, 1, 0, 0, 0, 0, loc))
	if err != nil {
		t.Fatalf("cutoff day must be allowed, got %v", err)
	}
}

func TestFetchData_TimestampsAcrossPragueMidnightInWinter(t *testing.T) {
	loc := mustPragueLoc(t)
	prices := make([]float32, 96)
	startOTEServer(t, func(w http.ResponseWriter, _ *http.Request) {
		w.Write(otePayload(prices, true))
	})
	// January (CET, UTC+1). Prague midnight = 23:00 UTC previous day.
	date := time.Date(2026, 1, 15, 0, 0, 0, 0, loc)
	got, err := FetchData(date)
	if err != nil {
		t.Fatalf("FetchData: %v", err)
	}
	want := time.Date(2026, 1, 14, 23, 0, 0, 0, time.UTC)
	if !got[0].Ts.Equal(want) {
		t.Errorf("winter first ts: got %v, want %v", got[0].Ts, want)
	}
}
