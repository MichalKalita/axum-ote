use crate::data_loader::fetch_data;
use arc_swap::ArcSwap;
use axum::{extract::State, routing::get, Json, Router};
use chrono::Local;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize, Clone)]
struct Prices {
    pub prices: [f32; 24],
    pub date: chrono::NaiveDate,
}

struct AppState {
    pub prices: ArcSwap<Option<Prices>>,
}

async fn fetch_data_handler(State(state): State<Arc<AppState>>) -> Json<Result<Prices, String>> {
    if state.prices.load().is_none() {
        let today = Local::now().date_naive();

        match fetch_data(today).await {
            Ok(prices) => {
                state.prices.store(Arc::new(Some(Prices {
                    prices,
                    date: today,
                })));
            }
            Err(e) => {
                return Json(Err(e.to_string()));
            }
        }
    }

    match state.prices.load().as_ref() {
        Some(prices) => Json(Ok(prices.clone())),
        None => Json(Err("Data not found".into())),
    }
}

pub(crate) async fn start_web_server() {
    let state = Arc::new(AppState {
        prices: ArcSwap::from(Arc::new(None)),
    });

    let app = Router::new()
        .route("/", get(fetch_data_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("Web server started on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
