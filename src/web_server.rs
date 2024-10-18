use crate::data_loader::fetch_data;
use axum::{extract::State, routing::get, Json, Router};
use chrono::Local;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Serialize, Clone)]
struct Prices {
    pub prices: [f32; 24],
    pub date: chrono::NaiveDate,
}

struct AppState {
    pub prices: RwLock<Option<Prices>>,
}

async fn fetch_data_handler(State(state): State<Arc<AppState>>) -> Json<Result<Prices, String>> {
    let missing_data = { state.prices.read().await.is_none() };

    if missing_data {
        let today = Local::now().date_naive();
        match fetch_data(today).await {
            Ok(prices) => {
                let mut prices_lock = state.prices.write().await;
                *prices_lock = Some(Prices {
                    prices,
                    date: today,
                });
                return Json(Ok(prices_lock.as_ref().unwrap().clone()));
            }
            Err(e) => {
                return Json(Err(e.to_string()));
            }
        }
    }

    Json(Ok(state.prices.read().await.clone().unwrap()))
}

pub(crate) async fn start_web_server() {
    let state = Arc::new(AppState {
        prices: RwLock::new(None),
    });

    let app = Router::new()
        .route("/", get(fetch_data_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
