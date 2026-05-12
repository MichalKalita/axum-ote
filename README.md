# OTE — Czech Electricity Spot Prices

Small Go app that fetches day-ahead electricity spot prices for the Czech Republic from the OTE market and serves them through a web UI and a CLI.

## Features

- Web UI with a day view of quarter-hour prices
- Next/previous day navigation
- Month calendar with daily averages
- Optimizer: find the N cheapest hours in a selected window
- EUR and CZK currencies
- Local SQLite cache (DST-aware) — each day is fetched from OTE once
- CLI mode for printing today's prices to stdout

## Run

Web server (default port `3000`, set `PORT` to override):

```sh
go run .
```

CLI:

```sh
go run . -cli           # EUR
go run . -cli -czk      # CZK
```

## Configuration

| Variable  | Default        | Purpose                     |
|-----------|----------------|-----------------------------|
| `PORT`    | `3000`         | HTTP listen port            |
| `DB_PATH` | `./data/ote.db`| SQLite database file path   |

## Data source

Prices come from the OTE-CR day-ahead market. Data is available from **2025-10-01** onwards (earlier dates are rejected by the fetcher).

## Deployment

A `railway.json` is included for one-click deploy on Railway. The `data/` directory should be backed by a persistent volume so the SQLite cache survives restarts.
