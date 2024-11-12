use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{Local, NaiveDate};
use maud::html;
use reqwest::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

pub(crate) mod state {
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
    }

    pub struct AppState {
        days: DashMap<chrono::NaiveDate, DayPrices>,
    }

    impl AppState {
        pub fn new() -> Self {
            Self {
                days: DashMap::new(),
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
    }
}

#[derive(Deserialize)]
struct QueryParams {
    date: Option<NaiveDate>,
}

async fn fetch_data_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<QueryParams>,
) -> impl IntoResponse {
    let today = Local::now().date_naive();
    let input_date = query.date.unwrap_or(today);

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
                div .mb-4.flex.justify-center { (prices.render_graph()) }

                h2 .text-2xl.font-semibold.mb-4 { "Table" }
                div .mb-4.flex.justify-center { (prices.render_table()) }
            ),
        ),
        None => (StatusCode::NOT_FOUND, html!(p { "Error fetching data." })),
    };

    (
        status,
        html! {
            html {
                head {
                    title { "OTE CR Price Checker" }
                    script src="https://cdn.tailwindcss.com" {}
                }
                body .p-4.text-center."dark:bg-gray-900"."dark:text-gray-300" {
                    (content)
                }
            }
        },
    )
}

pub(crate) async fn start_web_server() {
    let state = Arc::new(state::AppState::new());

    let app = Router::new()
        .route("/", get(fetch_data_handler))
        .route("/perf", get(perf_handler))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Web server started on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
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
