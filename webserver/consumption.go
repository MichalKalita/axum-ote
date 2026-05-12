package webserver

import (
	"encoding/csv"
	"fmt"
	"html"
	"io"
	"log"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

// ConsumptionQuarter is a 15-minute consumption reading aligned to an OTE
// quarter start (UTC). KWh is energy in the interval, not power.
type ConsumptionQuarter struct {
	QuarterStart time.Time
	KWh          float32
}

// ParseConsumptionCSV reads a ČEZ "PND export" CSV.
//
// Format (semicolon-delimited, Prague-local timestamps):
//
//	"Datum";"Profil +A [kW]";"Status";...
//	"DD.MM.YYYY HH:MM:SS";<kW>;"...";...
//
// The Datum column is the END of the 15-min interval; "24:00:00" represents
// midnight at the end of that day (not "00:00:00" of the next). Profil +A is
// average power in kW, so energy in kWh = kW × 0.25.
func ParseConsumptionCSV(r io.Reader) ([]ConsumptionQuarter, error) {
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		loc = time.UTC
	}
	cr := csv.NewReader(r)
	cr.Comma = ';'
	cr.FieldsPerRecord = -1
	cr.LazyQuotes = true

	var out []ConsumptionQuarter
	headerSeen := false
	for line := 1; ; line++ {
		rec, err := cr.Read()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("CSV read at line %d: %w", line, err)
		}
		if !headerSeen {
			headerSeen = true
			continue
		}
		if len(rec) < 2 {
			continue
		}
		dateStr := strings.TrimSpace(rec[0])
		kwStr := strings.TrimSpace(rec[1])
		if dateStr == "" || kwStr == "" {
			continue
		}
		end, err := parseCSVTimestamp(dateStr, loc)
		if err != nil {
			return nil, fmt.Errorf("line %d: invalid timestamp %q: %w", line, dateStr, err)
		}
		kwStr = strings.Replace(kwStr, ",", ".", 1)
		kw, err := strconv.ParseFloat(kwStr, 32)
		if err != nil {
			return nil, fmt.Errorf("line %d: invalid kW value %q: %w", line, kwStr, err)
		}
		start := end.Add(-15 * time.Minute).UTC()
		out = append(out, ConsumptionQuarter{
			QuarterStart: start,
			KWh:          float32(kw) * 0.25,
		})
	}
	return out, nil
}

func parseCSVTimestamp(s string, loc *time.Location) (time.Time, error) {
	if datePart, ok := strings.CutSuffix(s, " 24:00:00"); ok {
		d, err := time.ParseInLocation("02.01.2006", datePart, loc)
		if err != nil {
			return time.Time{}, err
		}
		return d.AddDate(0, 0, 1), nil
	}
	return time.ParseInLocation("02.01.2006 15:04:05", s, loc)
}

// DayStats summarizes one calendar day of consumption against OTE prices.
// All prices are in EUR/MWh.
type DayStats struct {
	Date          time.Time
	TotalKWh      float32
	WeightedPrice float32 // what the user effectively paid per MWh (consumption-weighted)
	FlatPrice     float32 // simple mean of OTE prices for the day
	BestPrice     float32 // minimum reachable by reordering the same consumption pattern
	WorstPrice    float32 // maximum reachable by reordering the same consumption pattern
	Score         float32 // 0..1; higher = closer to BestPrice
}

// ConsumptionAnalysis is the result of pairing a CSV upload with OTE prices.
type ConsumptionAnalysis struct {
	PerDay       []DayStats
	Overall      DayStats
	SkippedDates []string // dates whose OTE prices could not be loaded
	FutureDates  []string // dates ignored because OTE has not published them yet
}

// maxOTEDate returns the latest Prague-local date for which OTE prices may
// be available given `now`. OTE publishes the next day's prices at
// NextDayPricesHour (14:00) Prague-local; anything later than that is
// unknowable and we don't attempt to fetch it.
func maxOTEDate(now time.Time, loc *time.Location) time.Time {
	local := now.In(loc)
	today := time.Date(local.Year(), local.Month(), local.Day(), 0, 0, 0, 0, loc)
	if local.Hour() >= NextDayPricesHour {
		return today.AddDate(0, 0, 1)
	}
	return today
}

// AnalyzeConsumption groups the consumption quarters by Prague-local date,
// loads the corresponding day's OTE prices from the cache (fetching once on
// miss), and computes per-day plus overall statistics. Dates beyond what OTE
// could have published given `now` are recorded in FutureDates and skipped
// entirely — no fetch is attempted.
func (s *AppState) AnalyzeConsumption(quarters []ConsumptionQuarter, now time.Time) (*ConsumptionAnalysis, error) {
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		loc = time.UTC
	}
	maxDate := maxOTEDate(now, loc)

	byDate := map[string][]ConsumptionQuarter{}
	for _, q := range quarters {
		key := q.QuarterStart.In(loc).Format("2006-01-02")
		byDate[key] = append(byDate[key], q)
	}
	dateKeys := make([]string, 0, len(byDate))
	for k := range byDate {
		dateKeys = append(dateKeys, k)
	}
	sort.Strings(dateKeys)

	result := &ConsumptionAnalysis{}
	// Partition into fetchable and future dates up-front so the warm-up does
	// not issue requests for future days.
	fetchable := make([]string, 0, len(dateKeys))
	for _, k := range dateKeys {
		d, parseErr := time.ParseInLocation("2006-01-02", k, loc)
		if parseErr != nil {
			continue
		}
		if d.After(maxDate) {
			result.FutureDates = append(result.FutureDates, k)
			continue
		}
		fetchable = append(fetchable, k)
	}

	// Warm the cache concurrently — sequential per-day computation below then
	// hits SQLite only.
	var wg sync.WaitGroup
	for _, k := range fetchable {
		d, _ := time.ParseInLocation("2006-01-02", k, loc)
		wg.Add(1)
		go func(d time.Time) {
			defer wg.Done()
			s.GetPrices(d)
		}(d)
	}
	wg.Wait()

	var totalKWh, totalCost, bestSum, worstSum, flatSumKWh float32

	for _, k := range fetchable {
		date, _ := time.ParseInLocation("2006-01-02", k, loc)
		prices, ok := s.GetPrices(date)
		if !ok {
			result.SkippedDates = append(result.SkippedDates, k)
			continue
		}
		midnight := time.Date(date.Year(), date.Month(), date.Day(), 0, 0, 0, 0, loc)

		stats := DayStats{Date: date}
		cons := make([]float32, 0, len(prices.Prices))
		paired := make([]float32, 0, len(prices.Prices))

		for _, c := range byDate[k] {
			offsetMin := int(c.QuarterStart.Sub(midnight).Minutes())
			idx := offsetMin / 15
			if idx < 0 || idx >= len(prices.Prices) {
				continue
			}
			cons = append(cons, c.KWh)
			paired = append(paired, prices.Prices[idx])
			stats.TotalKWh += c.KWh
		}
		if stats.TotalKWh == 0 {
			result.PerDay = append(result.PerDay, stats)
			continue
		}

		var cost float32
		for i, p := range paired {
			cost += p * cons[i] / 1000.0
		}
		stats.WeightedPrice = cost * 1000.0 / stats.TotalKWh

		var sumP float32
		for _, p := range prices.Prices {
			sumP += p
		}
		stats.FlatPrice = sumP / float32(len(prices.Prices))

		bestCost, worstCost := bestWorstByReorder(cons, prices.Prices)
		stats.BestPrice = bestCost * 1000.0 / stats.TotalKWh
		stats.WorstPrice = worstCost * 1000.0 / stats.TotalKWh
		stats.Score = scoreFromCosts(cost, bestCost, worstCost)

		totalKWh += stats.TotalKWh
		totalCost += cost
		bestSum += bestCost
		worstSum += worstCost
		flatSumKWh += stats.FlatPrice * stats.TotalKWh

		result.PerDay = append(result.PerDay, stats)
	}

	if totalKWh > 0 {
		result.Overall.TotalKWh = totalKWh
		result.Overall.WeightedPrice = totalCost * 1000.0 / totalKWh
		result.Overall.BestPrice = bestSum * 1000.0 / totalKWh
		result.Overall.WorstPrice = worstSum * 1000.0 / totalKWh
		result.Overall.FlatPrice = flatSumKWh / totalKWh
		result.Overall.Score = scoreFromCosts(totalCost, bestSum, worstSum)
	}
	if len(result.PerDay) == 0 && len(result.SkippedDates) == 0 && len(result.FutureDates) == 0 {
		return nil, fmt.Errorf("no consumption rows could be matched against OTE prices")
	}
	if len(result.PerDay) == 0 && len(result.FutureDates) > 0 {
		return nil, fmt.Errorf("all consumption rows are dated in the future (OTE has not published prices for: %s)", strings.Join(result.FutureDates, ", "))
	}
	if len(result.SkippedDates) > 0 {
		log.Printf("AnalyzeConsumption skipped %d day(s) with no OTE data: %v",
			len(result.SkippedDates), result.SkippedDates)
	}
	if len(result.FutureDates) > 0 {
		log.Printf("AnalyzeConsumption ignored %d future day(s): %v",
			len(result.FutureDates), result.FutureDates)
	}
	return result, nil
}

// bestWorstByReorder returns the (min, max) cost achievable by reordering the
// same multiset of consumption values onto the day's quarters. By the
// rearrangement inequality, min = sort(cons desc) · sort(prices asc), and
// symmetrically for max.
func bestWorstByReorder(cons, prices []float32) (best, worst float32) {
	cs := append([]float32{}, cons...)
	sort.Slice(cs, func(i, j int) bool { return cs[i] > cs[j] })
	asc := append([]float32{}, prices...)
	sort.Slice(asc, func(i, j int) bool { return asc[i] < asc[j] })
	desc := append([]float32{}, prices...)
	sort.Slice(desc, func(i, j int) bool { return desc[i] > desc[j] })
	n := min(len(cs), len(asc))
	for i := 0; i < n; i++ {
		best += cs[i] * asc[i] / 1000.0
		worst += cs[i] * desc[i] / 1000.0
	}
	return best, worst
}

func scoreFromCosts(actual, best, worst float32) float32 {
	if worst <= best {
		return 1.0
	}
	v := (worst - actual) / (worst - best)
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

// renderConsumptionForm renders the file-upload form.
func renderConsumptionForm(curStr string) string {
	return fmt.Sprintf(`<form method="POST" action="/consumption?cur=%s" enctype="multipart/form-data" class="flex flex-col gap-2 items-center my-4">
<input type="file" name="csv" accept=".csv,text/csv" required class="p-2 border rounded">
<button type="submit" class="px-4 py-2 bg-blue-500 text-white rounded cursor-pointer">Analyze</button>
</form>`, html.EscapeString(curStr))
}

func priceFmt(p float32, currency Currency) string {
	v := currency.Convert(p)
	if currency == CurrencyCzk {
		return fmt.Sprintf("%.2f", v)
	}
	return fmt.Sprintf("%.1f", v)
}

func scoreColorClass(pct float32) string {
	switch {
	case pct >= 75:
		return "text-green-700 dark:text-green-400"
	case pct >= 50:
		return "text-yellow-700 dark:text-yellow-400"
	case pct >= 25:
		return "text-orange-700 dark:text-orange-400"
	default:
		return "text-red-600 dark:text-red-400"
	}
}

// renderConsumptionResults renders the overall summary, optional warnings,
// and the per-day breakdown table.
func renderConsumptionResults(a *ConsumptionAnalysis, currency Currency) string {
	var sb strings.Builder

	if len(a.SkippedDates) > 0 {
		sb.WriteString(`<p class="text-orange-700 dark:text-orange-400 my-2">Skipped (no OTE data): `)
		sb.WriteString(html.EscapeString(strings.Join(a.SkippedDates, ", ")))
		sb.WriteString(`</p>`)
	}
	if len(a.FutureDates) > 0 {
		sb.WriteString(`<p class="text-orange-700 dark:text-orange-400 my-2">Ignored (future, OTE has not published prices yet): `)
		sb.WriteString(html.EscapeString(strings.Join(a.FutureDates, ", ")))
		sb.WriteString(`</p>`)
	}

	sb.WriteString(`<h2 class="text-2xl font-semibold mb-2 mt-6">Summary</h2>`)
	sb.WriteString(renderSummaryCard(a.Overall, currency))

	sb.WriteString(`<h2 class="text-2xl font-semibold mb-2 mt-8">Per day</h2>`)
	sb.WriteString(`<div class="flex justify-center"><table>`)
	fmt.Fprintf(&sb,
		`<tr><th class="px-2">Date</th><th class="px-2">kWh</th><th class="px-2">My price</th><th class="px-2">Day avg</th><th class="px-2">Best</th><th class="px-2">Worst</th><th class="px-2">Score</th></tr>`)
	for _, d := range a.PerDay {
		sb.WriteString(renderDayRow(d, currency))
	}
	sb.WriteString(`</table></div>`)
	sb.WriteString(`<p class="text-xs mt-4 text-neutral-500">Best/Worst show what the same daily consumption pattern would have cost if shifted into the cheapest / most expensive quarters of the same day. Score is the position between them (100% = best, 0% = worst).</p>`)

	return sb.String()
}

func renderSummaryCard(s DayStats, currency Currency) string {
	pct := s.Score * 100.0
	var sb strings.Builder
	sb.WriteString(`<div class="text-lg mb-2">`)
	fmt.Fprintf(&sb, `<div class="font-bold text-xl">Total %.1f kWh</div>`, s.TotalKWh)
	fmt.Fprintf(&sb, `<div>My price: <span class="font-bold">%s</span> %s | Day avg: %s | Best: <span class="text-green-700 dark:text-green-400">%s</span> | Worst: <span class="text-red-700 dark:text-red-400">%s</span></div>`,
		priceFmt(s.WeightedPrice, currency), html.EscapeString(currency.ShortLabel()),
		priceFmt(s.FlatPrice, currency), priceFmt(s.BestPrice, currency), priceFmt(s.WorstPrice, currency))
	fmt.Fprintf(&sb, `<div>Score: <span class="font-bold %s">%.0f%%</span> <span class="text-xs">(higher = closer to best)</span></div>`,
		scoreColorClass(pct), pct)
	sb.WriteString(`</div>`)
	return sb.String()
}

func renderDayRow(s DayStats, currency Currency) string {
	pct := s.Score * 100.0
	var sb strings.Builder
	sb.WriteString(`<tr>`)
	fmt.Fprintf(&sb, `<td class="px-2 font-mono">%s</td>`, s.Date.Format("2006-01-02"))
	fmt.Fprintf(&sb, `<td class="px-2 font-mono text-right">%.1f</td>`, s.TotalKWh)
	if s.TotalKWh > 0 {
		fmt.Fprintf(&sb, `<td class="px-2 font-mono text-right font-bold">%s</td>`, priceFmt(s.WeightedPrice, currency))
		fmt.Fprintf(&sb, `<td class="px-2 font-mono text-right">%s</td>`, priceFmt(s.FlatPrice, currency))
		fmt.Fprintf(&sb, `<td class="px-2 font-mono text-right text-green-700 dark:text-green-400">%s</td>`, priceFmt(s.BestPrice, currency))
		fmt.Fprintf(&sb, `<td class="px-2 font-mono text-right text-red-700 dark:text-red-400">%s</td>`, priceFmt(s.WorstPrice, currency))
		fmt.Fprintf(&sb, `<td class="px-2 font-mono text-right %s">%.0f%%</td>`, scoreColorClass(pct), pct)
	} else {
		sb.WriteString(`<td class="px-2 font-mono text-right" colspan="5">—</td>`)
	}
	sb.WriteString(`</tr>`)
	return sb.String()
}
