package main

import (
	"flag"
	"fmt"
	"log"
	"math"
	"time"

	"github.com/MichalKalita/ote/dataloader"
	"github.com/MichalKalita/ote/webserver"
)

func main() {
	cli := flag.Bool("cli", false, "Print prices to stdout instead of starting the web server")
	czk := flag.Bool("czk", false, "Use CZK currency (CLI mode only)")
	flag.Parse()

	log.SetFlags(log.LstdFlags)

	if !*cli {
		webserver.StartWebServer()
		return
	}

	currency := webserver.CurrencyEur
	if *czk {
		currency = webserver.CurrencyCzk
	}
	printPrices(currency)
}

func printPrices(currency webserver.Currency) {
	loc, err := time.LoadLocation("Europe/Prague")
	if err != nil {
		loc = time.UTC
	}
	today := time.Now().In(loc)
	today = time.Date(today.Year(), today.Month(), today.Day(), 0, 0, 0, 0, loc)

	prices, err := dataloader.FetchData(today)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
		return
	}

	fmt.Println("Prices:")
	minPrice := float32(math.Inf(1))
	maxPrice := float32(math.Inf(-1))
	for _, p := range prices {
		if p < minPrice {
			minPrice = p
		}
		if p > maxPrice {
			maxPrice = p
		}
	}

	for hour := 0; hour < 24; hour++ {
		base := hour * 4
		fmt.Printf("%2d:00", hour)
		for q := 0; q < 4; q++ {
			idx := base + q
			dp := currency.Convert(prices[idx])
			marker := "  "
			switch prices[idx] {
			case minPrice:
				marker = " *"
			case maxPrice:
				marker = " **"
			}
			fmt.Printf("   %8.4f%s", dp, marker)
		}
		fmt.Printf("   %s\n", currency.ShortLabel())
	}
}
