use chrono::prelude::*;
use log::{error, info};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

#[derive(Deserialize)]
struct Point {
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

#[derive(Error, Debug)]
pub(crate) enum FetchError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Price data not found")]
    PriceDataNotFound,
    #[error("Unexpected response status: {0}")]
    UnexpectedStatus(reqwest::StatusCode),
    #[error("Price data does not contain exactly 24 points")]
    InvalidDataSize,
}

pub async fn fetch_data(date: NaiveDate) -> Result<Vec<f32>, FetchError> {
    let url = format!("https://www.ote-cr.cz/en/short-term-markets/electricity/day-ahead-market/@@chart-data?report_date={}", date);
    info!("Fetching data for date {}", date);

    let start = std::time::Instant::now();

    // Create a client
    let client = Client::new();

    // Make the GET request
    let response = client.get(&url).send().await.map_err(|error| {
        error!(
            "Request failed {} in {:?} error {}",
            date,
            start.elapsed(),
            error
        );

        error
    })?;

    info!(
        "Request for {} in {:?} status {}",
        date,
        start.elapsed(),
        response.status()
    );

    // Check if the request was successful
    if response.status().is_success() {
        // Parse the response JSON
        let response_json: Response = response.json().await?;

        // Find the data line with the title "Price (EUR/MWh)"
        if let Some(price_data) = response_json
            .data
            .data_line
            .iter()
            .find(|line| line.title == "15min price (EUR/MWh)")
        {
            // Check if the number of points is exactly 24
            if price_data.point.len() != 96 {
                error!("Price data does not contain exactly 96 points.");
                return Err(FetchError::InvalidDataSize);
            }

            Ok(price_data.point.iter().map(|point| point.y).collect())
        } else {
            error!("Price data not found in the response.");
            Err(FetchError::PriceDataNotFound)
        }
    } else {
        error!("Failed to fetch data. Status: {}", response.status());
        Err(FetchError::UnexpectedStatus(response.status()))
    }
}
