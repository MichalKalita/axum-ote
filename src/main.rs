use chrono::prelude::*;
use reqwest::Client;
use serde::Deserialize;
use std::error::Error;

#[derive(Deserialize)]
struct Point {
    // x: String,
    y: f32,
}

#[derive(Deserialize)]
struct DataLine {
    title: String,
    point: Vec<Point>,
}

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "dataLine")]
    data_line: Vec<DataLine>,
}

#[derive(Deserialize)]
struct Response {
    data: Data,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let url = format!("https://www.ote-cr.cz/en/short-term-markets/electricity/day-ahead-market/@@chart-data?report_date={}", today);
    println!("Fetching date {}", today);

    // Create a client
    let client = Client::new();

    // Make the GET request
    let response = client.get(&url).send().await?;

    // Check if the request was successful
    if response.status().is_success() {
        // Parse the response JSON
        let response_json: Response = response.json().await?;

        // Find the data line with the title "Price (EUR/MWh)"
        if let Some(price_data) = response_json
            .data
            .data_line
            .iter()
            .find(|line| line.title == "Price (EUR/MWh)")
        {
            println!("Prices:");
            let min_price = price_data
                .point
                .iter()
                .map(|p| p.y)
                .fold(f32::INFINITY, f32::min);
            let max_price = price_data
                .point
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);

            for item in price_data.point.iter().enumerate() {
                print!("{0:>2}:00 - {0:>2}:59\t{1:>7.2} EUR/MWh", item.0, item.1.y);
                if item.1.y == min_price {
                    print!(" (min)");
                }
                if item.1.y == max_price {
                    print!(" (max)");
                }
                println!();
            }
        } else {
            println!("Price data not found.");
        }
    } else {
        println!("Failed to fetch data. Status: {}", response.status());
    }

    Ok(())
}
