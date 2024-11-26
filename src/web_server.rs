use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{Local, NaiveDate, Timelike};
use maud::html;
use reqwest::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

use crate::html_render::render_layout;

pub(crate) mod state {
    use core::f32;

    use crate::data_loader::fetch_data;
    use dashmap::DashMap;
    use serde::Serialize;

    #[derive(Serialize, Clone)]
    pub struct DayPrices {
        pub prices: [f32; 24],
        // pub date: chrono::NaiveDate,
    }

    impl DayPrices {
        pub(crate) fn cheapest_hour(&self) -> (usize, &f32) {
            self.prices
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap()
        }

        pub(crate) fn expensive_hour(&self) -> (usize, &f32) {
            self.prices
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap()
        }

        pub fn total_prices(&self, dist: &Distribution) -> [f32; 24] {
            let mut prices = self.prices.clone();
            for (i, price) in prices.iter_mut().enumerate() {
                if dist.high_hours.contains(&(i as u8)) {
                    *price += dist.high_price;
                } else {
                    *price += dist.low_price;
                }
            }

            prices
        }
    }

    pub struct Distribution {
        pub high_hours: Vec<u8>,
        pub high_price: f32,
        pub low_price: f32,
    }

    impl Distribution {
        pub fn by_hours(&self) -> [bool; 24] {
            let mut distribution = [false; 24];
            for hour in self.high_hours.iter() {
                distribution[*hour as usize] = true;
            }
            distribution
        }
    }

    pub struct AppState {
        pub days: DashMap<chrono::NaiveDate, DayPrices>,
        pub distribution: Distribution,
    }

    impl AppState {
        pub fn new() -> Self {
            Self {
                days: DashMap::new(),
                distribution: Distribution {
                    high_hours: vec![10, 12, 14, 17],
                    high_price: 648.0 / 25.29,
                    low_price: 438.0 / 25.29,
                },
            }
        }
        pub async fn get_prices(&self, date: &chrono::NaiveDate) -> Option<DayPrices> {
            if !self.days.contains_key(date) {
                match fetch_data(*date).await {
                    Ok(prices) => {
                        self.days.insert(*date, DayPrices { prices });

                        return Some(DayPrices { prices });
                    }
                    Err(_) => {
                        return None;
                    }
                }
            }

            self.days.get(date).map(|i| i.value().clone())
        }

        /// Find the cheapest hours in row for the next days, return first of them
        pub async fn find_cheapiest_hours(&self, hours: u8) -> Option<u8> {
            let date = chrono::Local::now().date_naive();
            let prices = self.get_prices(&date).await;
            if prices.is_none() {
                return None;
            }

            let prices = prices.unwrap().prices;
            let mut hour = 0u8;
            let mut cheapist_price = f32::MAX;

            for i in 0..(24 - hours) {
                let total_price: f32 = prices.iter().skip(i as usize).take(hours as usize).sum();

                if total_price < cheapist_price {
                    cheapist_price = total_price;
                    hour = i;
                }
            }

            Some(hour)
        }
    }
}

pub(crate) async fn start_web_server() {
    let state = Arc::new(state::AppState::new());

    let app = Router::new()
        .route("/", get(fetch_data_handler))
        .route("/optimalizer", get(optimalizer_handler))
        .route("/perf", get(perf_handler))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Web server started on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct QueryParams {
    date: Option<NaiveDate>,
}

async fn fetch_data_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<QueryParams>,
) -> impl IntoResponse {
    let now = Local::now();
    let today = now.date_naive();
    let input_date = query.date.unwrap_or(today);

    let hour = now.time().hour() as usize;
    let active_hour = if input_date == today {
        hour
    } else {
        usize::MAX
    };

    let (status, content) = match state.get_prices(&input_date).await {
        Some(prices) => (
            StatusCode::OK,
            html!(
                h1 .text-4xl.font-bold.mb-8 { "OTE prices " (input_date) }
                    a href={"/?date=" (input_date - chrono::Duration::days(1))} { "Previous day" }
                    " | "
                    a href="/" { "today (" (today) ")" }
                    " | "
                    a href={"/?date=" (input_date + chrono::Duration::days(1))} { "Next day" }
                h2 .text-2xl.font-semibold.mb-4 { "Graph" }
                div .mb-4.flex.justify-center { (prices.render_graph(&state.distribution, active_hour)) }

                h2 .text-2xl.font-semibold.mb-4 { "Table" }
                div .mb-4.flex.justify-center { (prices.render_table(&state.distribution)) }
            ),
        ),
        None => (StatusCode::NOT_FOUND, html!(p { "Error fetching data." })),
    };

    (status, render_layout(content))
}

#[derive(Deserialize)]
struct OptimalizerQuery {
    hours: Option<u8>,
}

async fn optimalizer_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<OptimalizerQuery>,
) -> impl IntoResponse {
    // Form with numbers of hours in row to cheapist price

    let hours = query.hours.unwrap_or(1);
    let start_cheapiest = state.find_cheapiest_hours(hours).await;

    let content = html!(
        h1 .text-4xl.font-bold.mb-8 { "Optimalizer, find cheapist hours" }
        form {
            label for="hours" { "Number of hours" }
            input type="number" name="hours" min="1" max="24" value=(hours);
            button type="submit" { "Submit" }
        }

        h2 .text-2xl.font-semibold.mb-4 { "Cheapest hours starts in " (start_cheapiest.unwrap_or(0)) " hour" }
    );

    render_layout(content)
}

async fn perf_handler() -> String {
    // Make some CPU-bound work
    let mut sum: i64 = 0;
    for i in 0..1_000_000_000 {
        sum += i;
        if sum == 2 {
            sum = 3;
        }
        if sum == 10 {
            sum = 11;
        }
    }
    format!("Sum: {}", sum)
}
