mod data_loader;
mod web_server;

use chrono::Utc;
use chrono_tz::Europe::Prague;
use clap::Parser;
use data_loader::fetch_data;
use std::error::Error;
use web_server::state::Currency;

#[derive(Parser)]
#[clap(
    name = "OTE CR Price Checker",
    version = "1.0",
    author = "Michal Kalita",
    about = "Fetches and displays the current day-ahead electricity prices from OTE CR."
)]
struct Cli {
    #[clap(long)]
    web: bool,
    #[clap(long)]
    czk: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .init();

    if args.web {
        web_server::start_web_server().await;
    } else {
        let currency = if args.czk { Currency::Czk } else { Currency::Eur };
        print(currency).await;
    }

    Ok(())
}

async fn print(currency: Currency) {
    let today = Utc::now().with_timezone(&Prague).date_naive();
    match fetch_data(today).await {
        Ok(prices) => {
            println!("Prices:");
            let min_price = prices.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_price = prices.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

            for hour in 0..24 {
                let base = hour * 4;
                let display_prices: Vec<f32> = prices[base..base + 4].iter().map(|p| currency.convert(*p)).collect();
                let unit = currency.short_label();
                print!("{:>2}:00", hour);
                for (q, &dp) in display_prices.iter().enumerate() {
                    let idx = base + q;
                    let marker = if prices[idx] == min_price {
                        " *"
                    } else if prices[idx] == max_price {
                        " **"
                    } else {
                        "  "
                    };
                    print!("   {:>8.4}{}", dp, marker);
                }
                print!("   {}", unit);
                println!();
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}