mod data_loader;
mod web_server;

use chrono::Local;
use data_loader::fetch_data;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    web_server::start_web_server().await;

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
