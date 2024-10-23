mod data_loader;
mod html_render;
mod web_server;

use chrono::Local;
use clap::Parser;
use data_loader::fetch_data;
use std::error::Error;

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    env_logger::init();

    if args.web {
        web_server::start_web_server().await;
    } else {
        print().await;
    }

    Ok(())
}

async fn print() {
    let today = Local::now().date_naive();
    match fetch_data(today).await {
        Ok(prices) => {
            println!("Prices:");
            let min_price = prices.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_price = prices.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

            for (hour, &price) in prices.iter().enumerate() {
                print!("{0:>2}:00 - {0:>2}:59\t{1:>7.2} EUR/MWh", hour, price);
                if price == min_price {
                    print!(" (min)");
                }
                if price == max_price {
                    print!(" (max)");
                }
                println!();
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
