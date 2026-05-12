package webserver

import (
	"fmt"
	"math"
	"strings"
	"sync"
	"time"

	"github.com/MichalKalita/ote/dataloader"
)

type DayPrices struct {
	Prices []float32
}

// CheapestHour returns the index and the value of the lowest price.
func CheapestHour(prices []float32) (int, float32) {
	idx := 0
	min := float32(math.Inf(1))
	for i, p := range prices {
		if p < min {
			min = p
			idx = i
		}
	}
	return idx, min
}

// ExpensiveHour returns the index and the value of the highest price.
func ExpensiveHour(prices []float32) (int, float32) {
	idx := 0
	max := float32(math.Inf(-1))
	for i, p := range prices {
		if p > max {
			max = p
			idx = i
		}
	}
	return idx, max
}

// TotalPrices returns prices including distribution surcharges.
func (d *DayPrices) TotalPrices(dist *Distribution) []float32 {
	out := make([]float32, len(d.Prices))
	for i, price := range d.Prices {
		hour := byte(i / 4)
		if containsByte(dist.HighHours, hour) {
			out[i] = price + dist.HighPrice
		} else {
			out[i] = price + dist.LowPrice
		}
	}
	return out
}

func containsByte(s []byte, v byte) bool {
	for _, x := range s {
		if x == v {
			return true
		}
	}
	return false
}

type Distribution struct {
	HighHours []byte
	HighPrice float32
	LowPrice  float32
}

// ByHours returns an array of 24 labels ("V" for high, "N" for low).
func (d *Distribution) ByHours() [24]string {
	var out [24]string
	for i := range out {
		out[i] = "N"
	}
	for _, h := range d.HighHours {
		out[h] = "V"
	}
	return out
}

type AppState struct {
	mu           sync.RWMutex
	days         map[string]DayPrices
	Distribution Distribution
}

const NextDayPricesHour = 13

func NewAppState() *AppState {
	return &AppState{
		days: make(map[string]DayPrices),
		Distribution: Distribution{
			HighHours: []byte{10, 12, 14, 17},
			HighPrice: 648.0 / 25.29,
			LowPrice:  438.0 / 25.29,
		},
	}
}

func dateKey(date time.Time) string {
	return date.Format("2006-01-02")
}

// GetPrices returns cached prices for the date, or fetches them.
func (s *AppState) GetPrices(date time.Time) (*DayPrices, bool) {
	key := dateKey(date)

	s.mu.RLock()
	if dp, ok := s.days[key]; ok {
		s.mu.RUnlock()
		copyPrices := make([]float32, len(dp.Prices))
		copy(copyPrices, dp.Prices)
		return &DayPrices{Prices: copyPrices}, true
	}
	s.mu.RUnlock()

	prices, err := dataloader.FetchData(date)
	if err != nil {
		return nil, false
	}

	s.mu.Lock()
	s.days[key] = DayPrices{Prices: prices}
	s.mu.Unlock()

	copyPrices := make([]float32, len(prices))
	copy(copyPrices, prices)
	return &DayPrices{Prices: copyPrices}, true
}

// MonthAverages fetches average prices for each day in the given month and returns a map
// from day-of-month (1..31) to the daily average. Days that fail to load are absent.
// Days strictly after maxDate are skipped to avoid pointless API calls for future days.
func (s *AppState) MonthAverages(year int, month time.Month, loc *time.Location, includeDist bool, maxDate time.Time) map[int]float32 {
	first := time.Date(year, month, 1, 0, 0, 0, 0, loc)
	daysInMonth := first.AddDate(0, 1, -1).Day()

	results := make(map[int]float32)
	var mu sync.Mutex
	var wg sync.WaitGroup

	for day := 1; day <= daysInMonth; day++ {
		d := time.Date(year, month, day, 0, 0, 0, 0, loc)
		if d.After(maxDate) {
			continue
		}
		wg.Add(1)
		go func(d time.Time, day int) {
			defer wg.Done()
			prices, ok := s.GetPrices(d)
			if !ok {
				return
			}
			var displayPrices []float32
			if includeDist {
				displayPrices = prices.TotalPrices(&s.Distribution)
			} else {
				displayPrices = prices.Prices
			}
			var sum float32
			for _, p := range displayPrices {
				sum += p
			}
			avg := sum / float32(len(displayPrices))
			mu.Lock()
			results[day] = avg
			mu.Unlock()
		}(d, day)
	}
	wg.Wait()
	return results
}

// ExpressionContext builds an EvaluateContext from yesterday/today (+tomorrow if late enough).
func (s *AppState) ExpressionContext() *EvaluateContext {
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		loc = time.UTC
	}
	now := time.Now().In(loc)
	hour := now.Hour()
	today := time.Date(now.Year(), now.Month(), now.Day(), 0, 0, 0, 0, loc)
	yesterday := today.AddDate(0, 0, -1)
	tomorrow := today.AddDate(0, 0, 1)

	type fetchResult struct {
		prices *DayPrices
		ok     bool
	}
	var wg sync.WaitGroup
	var ysd, td, tmw fetchResult

	wg.Add(2)
	go func() {
		defer wg.Done()
		ysd.prices, ysd.ok = s.GetPrices(yesterday)
	}()
	go func() {
		defer wg.Done()
		td.prices, td.ok = s.GetPrices(today)
	}()
	if hour >= NextDayPricesHour {
		wg.Add(1)
		go func() {
			defer wg.Done()
			tmw.prices, tmw.ok = s.GetPrices(tomorrow)
		}()
	}
	wg.Wait()

	if !td.ok {
		return nil
	}

	var prices []float32
	offset := 0
	if ysd.ok {
		prices = append(prices, ysd.prices.Prices...)
		offset = 24
	}
	prices = append(prices, td.prices.Prices...)
	if tmw.ok {
		prices = append(prices, tmw.prices.Prices...)
	}

	nowLocal := time.Date(now.Year(), now.Month(), now.Day(), now.Hour(), now.Minute(), now.Second(), now.Nanosecond(), time.UTC)
	return NewEvaluateContext(nowLocal, prices, hour+offset)
}

type Currency int

const (
	CurrencyEur Currency = iota
	CurrencyCzk
)

const CurrencyRate float32 = 24.30

func (c Currency) Convert(price float32) float32 {
	switch c {
	case CurrencyEur:
		return price
	case CurrencyCzk:
		return price * CurrencyRate / 1000.0
	}
	return price
}

func (c Currency) ShortLabel() string {
	switch c {
	case CurrencyEur:
		return "EUR/MWh"
	case CurrencyCzk:
		return "CZK/kWh"
	}
	return ""
}

func (c Currency) String() string {
	switch c {
	case CurrencyEur:
		return "eur"
	case CurrencyCzk:
		return "czk"
	}
	return ""
}

func ParseCurrency(s string) (Currency, error) {
	switch strings.ToLower(s) {
	case "eur":
		return CurrencyEur, nil
	case "czk":
		return CurrencyCzk, nil
	}
	return CurrencyEur, fmt.Errorf("unknown currency: %s", s)
}
