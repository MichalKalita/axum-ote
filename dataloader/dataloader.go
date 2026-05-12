package dataloader

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"time"

	"github.com/MichalKalita/ote/storage"
)

type point struct {
	Y float32 `json:"y"`
}

type dataLine struct {
	Title string  `json:"title"`
	Point []point `json:"point"`
}

type data struct {
	DataLine []dataLine `json:"dataLine"`
}

type response struct {
	Data data `json:"data"`
}

var (
	ErrPriceDataNotFound = errors.New("Price data not found")
	// ErrDateBeforeQuarterHourly is returned for any date before 2025-10-01
	// (Prague-local), the first day OTE published 15-minute prices. No HTTP
	// request is made for such dates.
	ErrDateBeforeQuarterHourly = errors.New("15-minute prices are only available from 2025-10-01")
)

// BaseURL is the OTE chart-data endpoint. Tests override it to point at a local
// httptest.Server.
var BaseURL = "https://www.ote-cr.cz/en/short-term-markets/electricity/day-ahead-market/@@chart-data"

type UnexpectedStatusError struct {
	Status int
}

func (e *UnexpectedStatusError) Error() string {
	return fmt.Sprintf("Unexpected response status: %d", e.Status)
}

// FetchData fetches day-ahead 15-minute electricity prices for the given
// Prague-local date. The returned slice has one entry per quarter-hour;
// timestamps are in UTC. On DST days the slice has 92 or 100 entries.
func FetchData(date time.Time) ([]storage.Quarter, error) {
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		loc = time.UTC
	}
	dayStart := time.Date(date.Year(), date.Month(), date.Day(), 0, 0, 0, 0, loc)
	if dayStart.Before(time.Date(2025, 10, 1, 0, 0, 0, 0, loc)) {
		return nil, ErrDateBeforeQuarterHourly
	}
	dateStr := dayStart.Format("2006-01-02")
	url := fmt.Sprintf("%s?report_date=%s", BaseURL, dateStr)
	log.Printf("Fetching data for date %s", dateStr)

	start := time.Now()

	client := &http.Client{}
	req, err := http.NewRequestWithContext(context.Background(), http.MethodGet, url, nil)
	if err != nil {
		return nil, fmt.Errorf("Network error: %w", err)
	}

	resp, err := client.Do(req)
	if err != nil {
		log.Printf("Request failed %s in %v error %v", dateStr, time.Since(start), err)
		return nil, fmt.Errorf("Network error: %w", err)
	}
	defer resp.Body.Close()

	log.Printf("Request for %s in %v status %d", dateStr, time.Since(start), resp.StatusCode)

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		log.Printf("Failed to fetch data. Status: %d", resp.StatusCode)
		return nil, &UnexpectedStatusError{Status: resp.StatusCode}
	}

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("Network error: %w", err)
	}

	var respJSON response
	if err := json.Unmarshal(body, &respJSON); err != nil {
		return nil, fmt.Errorf("JSON parsing error: %w", err)
	}

	for _, line := range respJSON.Data.DataLine {
		if line.Title == "15min price (EUR/MWh)" {
			out := make([]storage.Quarter, len(line.Point))
			for i, p := range line.Point {
				ts := dayStart.Add(time.Duration(i) * 15 * time.Minute).UTC()
				out[i] = storage.Quarter{Ts: ts, Price: p.Y}
			}
			return out, nil
		}
	}

	log.Printf("Price data not found in the response.")
	return nil, ErrPriceDataNotFound
}
