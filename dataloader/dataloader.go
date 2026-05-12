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
	ErrInvalidDataSize   = errors.New("Price data does not contain exactly 24 points")
)

type UnexpectedStatusError struct {
	Status int
}

func (e *UnexpectedStatusError) Error() string {
	return fmt.Sprintf("Unexpected response status: %d", e.Status)
}

// FetchData fetches day-ahead electricity prices for the given date.
func FetchData(date time.Time) ([]float32, error) {
	dateStr := date.Format("2006-01-02")
	url := fmt.Sprintf(
		"https://www.ote-cr.cz/en/short-term-markets/electricity/day-ahead-market/@@chart-data?report_date=%s",
		dateStr,
	)
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
			if len(line.Point) != 96 {
				log.Printf("Price data does not contain exactly 96 points.")
				return nil, ErrInvalidDataSize
			}
			out := make([]float32, len(line.Point))
			for i, p := range line.Point {
				out[i] = p.Y
			}
			return out, nil
		}
	}

	log.Printf("Price data not found in the response.")
	return nil, ErrPriceDataNotFound
}
