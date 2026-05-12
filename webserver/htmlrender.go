package webserver

import (
	"embed"
	"fmt"
	"html"
	"html/template"
	"math"
	"strings"
)

const bodyClasses = "p-4 text-center dark:bg-gray-900 dark:text-gray-300"

//go:embed templates/*.html
var templatesFS embed.FS

var layoutTmpl = template.Must(template.ParseFS(templatesFS, "templates/*.html"))

// RenderLayout wraps a page body in the full HTML document and emits the generated CSS.
func RenderLayout(content string) string {
	classes := map[string]struct{}{}
	ExtractClassesFromHTML(content, classes)
	ExtractClassesFromStr(bodyClasses, classes)
	css := GenerateCSS(classes)

	var sb strings.Builder
	err := layoutTmpl.ExecuteTemplate(&sb, "layout.html", struct {
		CSS         template.CSS
		BodyClasses string
		Content     template.HTML
	}{
		CSS:         template.CSS(css),
		BodyClasses: bodyClasses,
		Content:     template.HTML(content),
	})
	if err != nil {
		panic(err)
	}
	return sb.String()
}

// ChartSettings controls bar chart dimensions.
type ChartSettings struct {
	Height     float32
	BarWidth   int
	BarSpacing int
}

func DefaultChartSettings() ChartSettings {
	return ChartSettings{Height: 300.0, BarWidth: 24, BarSpacing: 1}
}

type chartMetrics struct {
	scale      float32
	zeroOffset float32
	svgWidth   int
	svgHeight  float32
}

func (cs ChartSettings) calculateMetrics(prices []float32) chartMetrics {
	_, cheapest := CheapestHour(prices)
	_, expensive := ExpensiveHour(prices)

	var scale, zeroOffset float32
	if cheapest < 0 {
		scale = cs.Height / (expensive - cheapest)
		zeroOffset = 15.0 + expensive*scale
	} else {
		scale = cs.Height / expensive
		zeroOffset = cs.Height + 15.0
	}

	return chartMetrics{
		scale:      scale,
		zeroOffset: zeroOffset,
		svgWidth:   len(prices) * (cs.BarWidth + cs.BarSpacing),
		svgHeight:  cs.Height + 30.0,
	}
}

func (cs ChartSettings) calculateBarX(hour int) int {
	return hour * (cs.BarWidth + cs.BarSpacing)
}

func (cs ChartSettings) calculateBarY(price float32, m chartMetrics) float32 {
	if price >= 0 {
		return m.zeroOffset - price*m.scale
	}
	return m.zeroOffset
}

func (cs ChartSettings) calculateBarHeight(price float32, m chartMetrics) float32 {
	v := float32(math.Abs(float64(price))) * m.scale
	if v < 1.0 {
		return 1.0
	}
	return v
}

func (cs ChartSettings) calculateTextX(hour int) int {
	return hour*(cs.BarWidth+cs.BarSpacing) + cs.BarWidth/2
}

func (cs ChartSettings) calculatePriceTextY(price float32, m chartMetrics) float32 {
	return m.zeroOffset - price*m.scale - 3.0
}

func (cs ChartSettings) calculateLabelTextY(m chartMetrics) float32 {
	return m.zeroOffset - 10.0
}

// Render returns the SVG markup for a bar chart over prices.
func (cs ChartSettings) Render(prices []float32, labels []string, color func(index int, price float32) string, currency Currency) string {
	metrics := cs.calculateMetrics(prices)

	var sb strings.Builder
	fmt.Fprintf(&sb, `<svg viewBox="0 0 %d %s" style="max-width:%dpx">`,
		metrics.svgWidth, fmtFloat(metrics.svgHeight), metrics.svgWidth)
	sb.WriteString("<g>")
	for hour, price := range prices {
		cls := color(hour, price)
		fmt.Fprintf(&sb, `<rect x="%d" y="%s" width="%d" height="%s" class="%s" data-idx="%d"></rect>`,
			cs.calculateBarX(hour),
			fmtFloat(cs.calculateBarY(price, metrics)),
			cs.BarWidth,
			fmtFloat(cs.calculateBarHeight(price, metrics)),
			cls,
			hour,
		)
		var priceStr string
		if currency == CurrencyCzk {
			priceStr = fmt.Sprintf("%.1f", currency.Convert(price))
		} else {
			priceStr = fmt.Sprintf("%.0f", currency.Convert(price))
		}
		fmt.Fprintf(&sb, `<text x="%d" y="%s" text-anchor="middle" class="font-mono text-xs dark:fill-gray-300">%s</text>`,
			cs.calculateTextX(hour),
			fmtFloat(cs.calculatePriceTextY(price, metrics)),
			html.EscapeString(priceStr),
		)
		if labels != nil {
			fmt.Fprintf(&sb, `<text x="%d" y="%s" text-anchor="middle" class="font-mono text-xs dark:fill-gray-100">%s</text>`,
				cs.calculateTextX(hour),
				fmtFloat(cs.calculateLabelTextY(metrics)),
				html.EscapeString(labels[hour/4]),
			)
		}
	}
	sb.WriteString("</g></svg>")
	return sb.String()
}

func fmtFloat(f float32) string {
	// Print without trailing zeros, mimicking Rust's f32 Display.
	s := fmt.Sprintf("%g", f)
	return s
}

// EvaluateAllInChart renders a chart visualizing condition results across the context.
func (c Condition) EvaluateAllInChart(ctx *EvaluateContext) string {
	results := c.EvaluateAll(ctx)
	labels := make([]string, len(results))
	for i, r := range results {
		if r {
			labels[i] = "T"
		} else {
			labels[i] = "F"
		}
	}
	chart := DefaultChartSettings()
	return chart.Render(ctx.Prices.Prices, labels, func(index int, _ float32) string {
		if results[index] {
			return "fill-green-600"
		}
		return "fill-red-600"
	}, CurrencyEur)
}

func formatPrice(price float32, currency Currency) string {
	s := fmt.Sprintf("%.2f", currency.Convert(price))
	idx := strings.Index(s, ".")
	return fmt.Sprintf(`%s<span class="text-neutral-500 text-sm">.%s</span>`,
		s[:idx], s[idx+1:])
}

// Link returns an anchor tag with the underline+hover style.
func Link(url, text string) string {
	return fmt.Sprintf(`<a href="%s" class="underline hover:text-red-400">%s</a>`,
		html.EscapeString(url), html.EscapeString(text))
}

// RenderTable renders the 24x4 table of quarter-hour prices for a single day.
func (d *DayPrices) RenderTable(dist *Distribution, actualIndex int, currency Currency, includeDist bool) string {
	totalPrices := d.TotalPrices(dist)
	var displayPrices []float32
	if includeDist {
		displayPrices = totalPrices
	} else {
		displayPrices = d.Prices
	}

	minIdx, _ := CheapestHour(displayPrices)
	maxIdx, _ := ExpensiveHour(displayPrices)

	var sb strings.Builder
	sb.WriteString("<table>")
	sb.WriteString(`<tr><th class="px-4">Hour</th><th class="px-4">:00</th><th class="px-4">:15</th><th class="px-4">:30</th><th class="px-4">:45</th></tr>`)
	for hour := 0; hour < 24; hour++ {
		sb.WriteString("<tr>")
		fmt.Fprintf(&sb, `<td class="text-right font-mono px-4">%d:00</td>`, hour)
		for q := 0; q < 4; q++ {
			idx := hour*4 + q
			price := displayPrices[idx]
			classes := []string{"text-right", "font-mono", "px-4"}
			if idx == minIdx {
				classes = append(classes, "bg-green-100", "dark:bg-green-900")
			}
			if idx == maxIdx {
				classes = append(classes, "bg-red-100", "dark:bg-red-900")
			}
			if idx == actualIndex {
				classes = append(classes, "font-bold", "outline-2", "outline-blue-500")
			}
			if price < 0 {
				classes = append(classes, "text-green-700")
			}
			fmt.Fprintf(&sb, `<td class="%s" data-idx="%d">%s</td>`,
				strings.Join(classes, " "), idx, formatPrice(price, currency))
		}
		sb.WriteString("</tr>")
	}
	sb.WriteString("</table>")
	return sb.String()
}

// RenderHTML returns the HTML representation of the condition tree.
func (c Condition) RenderHTML() string {
	switch c.Kind {
	case CondAnd:
		var sb strings.Builder
		sb.WriteString(`<div class="ml-4">AND<ul>`)
		for _, child := range c.Children {
			sb.WriteString("<li>")
			sb.WriteString(child.RenderHTML())
			sb.WriteString("</li>")
		}
		sb.WriteString("</ul></div>")
		return sb.String()
	case CondOr:
		var sb strings.Builder
		sb.WriteString(`<div class="ml-4">OR<ul>`)
		for _, child := range c.Children {
			sb.WriteString("<li>")
			sb.WriteString(child.RenderHTML())
			sb.WriteString("</li>")
		}
		sb.WriteString("</ul></div>")
		return sb.String()
	case CondNot:
		return `<div class="ml-4">NOT` + c.Inner.RenderHTML() + `</div>`
	case CondPrice:
		return fmt.Sprintf(`<div class="ml-4">Price: %s</div>`, fmt.Sprintf("%g", c.Price))
	case CondHours:
		return fmt.Sprintf(`<div class="ml-4">Hours: %d - %d</div>`, c.HoursMin, c.HoursMax)
	case CondCheap:
		return fmt.Sprintf(`<div class="ml-4">Cheap: %d cheapiest hours in hours %d - %d</div>`,
			c.Cheap.Hours, c.Cheap.From, c.Cheap.To)
	}
	return ""
}

// RenderCheapForm renders the GET form for editing the Cheap fields.
func RenderCheapForm(cc *CheapCondition) string {
	actual := cc
	if actual == nil {
		actual = &CheapCondition{Hours: 1, From: 0, To: 24}
	}
	var sb strings.Builder
	sb.WriteString(`<form method="GET" class="flex space-x-2 items-center">`)
	sb.WriteString(`<label for="cheap_hours">Cheap Hours:</label>`)
	fmt.Fprintf(&sb, `<input type="number" id="cheap_hours" name="hours" value="%d" min="1" max="24" step="1" class="w-16 p-1 border rounded">`, actual.Hours)
	sb.WriteString(`<label for="cheap_from">From:</label>`)
	fmt.Fprintf(&sb, `<input type="number" id="cheap_from" name="from" value="%d" min="0" max="23" step="1" class="w-16 p-1 border rounded">`, actual.From)
	sb.WriteString(`<label for="cheap_to">To:</label>`)
	fmt.Fprintf(&sb, `<input type="number" id="cheap_to" name="to" value="%d" min="1" max="24" step="1" class="w-16 p-1 border rounded">`, actual.To)
	sb.WriteString(`<button type="submit" class="px-4 py-1 bg-blue-500 text-white rounded cursor-pointer">Update</button>`)
	sb.WriteString(`</form>`)
	return sb.String()
}
