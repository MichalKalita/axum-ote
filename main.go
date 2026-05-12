package main

import (
	"flag"
	"fmt"
	"log"
	"math"
	"os"
	"path/filepath"
	"time"

	"github.com/MichalKalita/ote/dataloader"
	"github.com/MichalKalita/ote/storage"
	"github.com/MichalKalita/ote/webserver"
)

func main() {
	cli := flag.Bool("cli", false, "Print prices to stdout instead of starting the web server")
	czk := flag.Bool("czk", false, "Use CZK currency (CLI mode only)")
	flag.Parse()

	log.SetFlags(log.LstdFlags)

	if !*cli {
		dbPath := os.Getenv("DB_PATH")
		if dbPath == "" {
			dbPath = "./data/ote.db"
		}
		if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
			log.Fatalf("create db dir: %v", err)
		}
		db, err := storage.Open(dbPath)
		if err != nil {
			log.Fatalf("open db: %v", err)
		}
		defer db.Close()
		webserver.StartWebServer(db)
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

	quarters, err := dataloader.FetchData(today)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
		return
	}

	fmt.Println("Prices:")
	minPrice := float32(math.Inf(1))
	maxPrice := float32(math.Inf(-1))
	for _, q := range quarters {
		if q.Price < minPrice {
			minPrice = q.Price
		}
		if q.Price > maxPrice {
			maxPrice = q.Price
		}
	}

	for hour := 0; hour < 24; hour++ {
		base := hour * 4
		if base >= len(quarters) {
			break
		}
		fmt.Printf("%2d:00", hour)
		for q := 0; q < 4; q++ {
			idx := base + q
			if idx >= len(quarters) {
				break
			}
			price := quarters[idx].Price
			dp := currency.Convert(price)
			marker := "  "
			switch price {
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
