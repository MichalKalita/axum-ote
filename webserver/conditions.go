package webserver

import (
	"encoding/json"
	"fmt"
	"sort"
	"time"

	json5 "github.com/titanous/json5"
)

// ConditionKind discriminates Condition variants.
type ConditionKind int

const (
	CondAnd ConditionKind = iota
	CondOr
	CondNot
	CondPrice
	CondHours
	CondCheap
	CondDebug // test-only
)

// Condition mirrors the Rust enum.
type Condition struct {
	Kind     ConditionKind
	Children []Condition    // And, Or
	Inner    *Condition     // Not
	Price    float32        // Price
	HoursMin uint32         // Hours
	HoursMax uint32         // Hours
	Cheap    CheapCondition // Cheap
	Debug    bool           // Debug (tests)
}

type CheapCondition struct {
	Hours uint8 `json:"hours"`
	From  uint8 `json:"from"`
	To    uint8 `json:"to"`
}

// MarshalJSON encodes a Condition the same way Serde does for the tagged enum.
func (c Condition) MarshalJSON() ([]byte, error) {
	switch c.Kind {
	case CondAnd:
		return json.Marshal(map[string]any{"and": c.Children})
	case CondOr:
		return json.Marshal(map[string]any{"or": c.Children})
	case CondNot:
		return json.Marshal(map[string]any{"not": c.Inner})
	case CondPrice:
		return json.Marshal(map[string]any{"price": c.Price})
	case CondHours:
		return json.Marshal(map[string]any{"hours": [2]uint32{c.HoursMin, c.HoursMax}})
	case CondCheap:
		return json.Marshal(map[string]any{"cheap": c.Cheap})
	case CondDebug:
		return json.Marshal(map[string]any{"debug": c.Debug})
	}
	return nil, fmt.Errorf("unknown condition kind")
}

// UnmarshalJSON decodes JSON in the form {"price": 120} | {"hours":[0,10]} | ...
func (c *Condition) UnmarshalJSON(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if len(raw) != 1 {
		return fmt.Errorf("condition must have a single key, got %d", len(raw))
	}
	for key, val := range raw {
		switch key {
		case "and":
			var arr []Condition
			if err := json.Unmarshal(val, &arr); err != nil {
				return err
			}
			c.Kind = CondAnd
			c.Children = arr
		case "or":
			var arr []Condition
			if err := json.Unmarshal(val, &arr); err != nil {
				return err
			}
			c.Kind = CondOr
			c.Children = arr
		case "not":
			var inner Condition
			if err := json.Unmarshal(val, &inner); err != nil {
				return err
			}
			c.Kind = CondNot
			c.Inner = &inner
		case "price":
			var v float32
			if err := json.Unmarshal(val, &v); err != nil {
				return err
			}
			c.Kind = CondPrice
			c.Price = v
		case "hours":
			var arr [2]uint32
			if err := json.Unmarshal(val, &arr); err != nil {
				return err
			}
			c.Kind = CondHours
			c.HoursMin = arr[0]
			c.HoursMax = arr[1]
		case "cheap":
			var cc CheapCondition
			if err := json.Unmarshal(val, &cc); err != nil {
				return err
			}
			c.Kind = CondCheap
			c.Cheap = cc
		case "debug":
			var v bool
			if err := json.Unmarshal(val, &v); err != nil {
				return err
			}
			c.Kind = CondDebug
			c.Debug = v
		default:
			return fmt.Errorf("unknown condition key: %s", key)
		}
	}
	return nil
}

// ParseCondition parses a JSON5 array of conditions and wraps it in And.
func ParseCondition(s string) (Condition, error) {
	// json5 -> normal json first, then unmarshal
	var raw any
	if err := json5.Unmarshal([]byte(s), &raw); err != nil {
		return Condition{}, err
	}
	normal, err := json.Marshal(raw)
	if err != nil {
		return Condition{}, err
	}
	var items []Condition
	if err := json.Unmarshal(normal, &items); err != nil {
		return Condition{}, err
	}
	return Condition{Kind: CondAnd, Children: items}, nil
}

// Format formats a Condition similarly to Rust's Debug.
func (c Condition) Format() string {
	switch c.Kind {
	case CondAnd:
		s := "And(["
		for i, child := range c.Children {
			if i > 0 {
				s += ", "
			}
			s += child.Format()
		}
		return s + "])"
	case CondOr:
		s := "Or(["
		for i, child := range c.Children {
			if i > 0 {
				s += ", "
			}
			s += child.Format()
		}
		return s + "])"
	case CondNot:
		return "Not(" + c.Inner.Format() + ")"
	case CondPrice:
		return fmt.Sprintf("Price(%g)", c.Price)
	case CondHours:
		return fmt.Sprintf("Hours(%d, %d)", c.HoursMin, c.HoursMax)
	case CondCheap:
		return fmt.Sprintf("Cheap(CheapCondition { hours: %d, from: %d, to: %d })",
			c.Cheap.Hours, c.Cheap.From, c.Cheap.To)
	case CondDebug:
		return fmt.Sprintf("Debug(%v)", c.Debug)
	}
	return ""
}

// Evaluate evaluates the condition against the given context.
func (c Condition) Evaluate(ctx *EvaluateContext) bool {
	switch c.Kind {
	case CondAnd:
		if len(c.Children) == 0 {
			return false
		}
		for _, child := range c.Children {
			if !child.Evaluate(ctx) {
				return false
			}
		}
		return true
	case CondOr:
		if len(c.Children) == 0 {
			return false
		}
		for _, child := range c.Children {
			if child.Evaluate(ctx) {
				return true
			}
		}
		return false
	case CondNot:
		return !c.Inner.Evaluate(ctx)
	case CondPrice:
		return ctx.Prices.Prices[ctx.Prices.NowIndex] <= c.Price
	case CondHours:
		hour := uint32(ctx.Now.Hour())
		return c.HoursMin <= hour && hour <= c.HoursMax
	case CondCheap:
		return c.Cheap.Evaluate(ctx)
	case CondDebug:
		return c.Debug
	}
	return false
}

// EvaluateAll evaluates the condition across all price slots in the context.
func (c Condition) EvaluateAll(ctx *EvaluateContext) []bool {
	startTime := ctx.Now.Add(-time.Duration(ctx.Prices.NowIndex) * time.Hour)
	out := make([]bool, len(ctx.Prices.Prices))
	for i := range ctx.Prices.Prices {
		updatedCtx := &EvaluateContext{
			Now: startTime.Add(time.Duration(i) * time.Hour),
			Prices: PricesContext{
				Prices:   append([]float32(nil), ctx.Prices.Prices...),
				NowIndex: i,
			},
		}
		out[i] = c.Evaluate(updatedCtx)
	}
	return out
}

func (cc CheapCondition) Evaluate(ctx *EvaluateContext) bool {
	prices, ok := ctx.Slice(int(cc.From), int(cc.To))
	if !ok {
		return false
	}
	sort.Slice(prices, func(i, j int) bool { return prices[i] < prices[j] })
	actualPrice := ctx.ActualPrice()
	pos := len(prices)
	for i, p := range prices {
		if actualPrice < p {
			pos = i
			break
		}
	}
	return pos <= int(cc.Hours)
}

// EvaluateContext is the price + time context for evaluation.
type EvaluateContext struct {
	Now    time.Time
	Prices PricesContext
}

type PricesContext struct {
	Prices   []float32
	NowIndex int
}

func NewEvaluateContext(now time.Time, prices []float32, targetPriceIndex int) *EvaluateContext {
	return &EvaluateContext{
		Now: now,
		Prices: PricesContext{
			Prices:   prices,
			NowIndex: targetPriceIndex,
		},
	}
}

func (ctx *EvaluateContext) ActualPrice() float32 {
	return ctx.Prices.Prices[ctx.Prices.NowIndex]
}

// Slice returns the price slice for the time range [from..to), or false if not applicable.
func (ctx *EvaluateContext) Slice(from, to int) ([]float32, bool) {
	rng, ok := findTimeRange(ctx.Prices.NowIndex, uint8(from), uint8(to))
	if !ok {
		return nil, false
	}
	if rng[1] > len(ctx.Prices.Prices) {
		return nil, false
	}
	out := make([]float32, rng[1]-rng[0])
	copy(out, ctx.Prices.Prices[rng[0]:rng[1]])
	return out, true
}

// findTimeRange — see Rust doc; returns [start,end) if current index lies inside.
func findTimeRange(currentHourIdx int, fromHour, toHour uint8) ([2]int, bool) {
	currentDay := currentHourIdx / 24
	currentHour := currentHourIdx % 24

	fromDayOffset := currentDay
	if int(fromHour) > currentHour {
		fromDayOffset--
	}

	toDayOffset := fromDayOffset
	if fromHour > toHour {
		toDayOffset++
	}

	startISize := fromDayOffset*24 + int(fromHour)
	endISize := toDayOffset*24 + int(toHour)
	if startISize < 0 || endISize < 0 {
		return [2]int{}, false
	}
	if startISize <= currentHourIdx && currentHourIdx < endISize {
		return [2]int{startISize, endISize}, true
	}
	return [2]int{}, false
}
