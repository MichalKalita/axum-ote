use axum::{extract::State, routing::get, Json, Router};
use chrono::Local;
use std::sync::Arc;

mod state {
    use crate::data_loader::fetch_data;
    use arc_swap::ArcSwap;
    use serde::Serialize;
    use std::sync::Arc;

    #[derive(Serialize, Clone)]
    pub struct Prices {
        pub prices: [f32; 24],
        pub date: chrono::NaiveDate,
    }
    pub struct AppState {
        prices: ArcSwap<Option<Prices>>,
    }

    impl AppState {
        pub fn new() -> Self {
            Self {
                prices: ArcSwap::from(Arc::new(None)),
            }
        }
        pub async fn get_prices(&self, date: &chrono::NaiveDate) -> Option<Prices> {
            if self.prices.load().is_none() {
                match fetch_data(*date).await {
                    Ok(prices) => {
                        self.prices.store(Arc::new(Some(Prices {
                            prices,
                            date: *date,
                        })));
                    }
                    Err(_) => {
                        return None;
                    }
                }
            }

            self.prices.load().as_ref().clone()
        }
    }
}

async fn fetch_data_handler(
    State(state): State<Arc<state::AppState>>,
) -> Json<Result<state::Prices, String>> {
    let today = Local::now().date_naive();
    match state.get_prices(&today).await {
        Some(prices) => Json(Ok(prices)),
        None => Json(Err("Data not found".into())),
    }
}

pub(crate) async fn start_web_server() {
    let state = Arc::new(state::AppState::new());

    let app = Router::new()
        .route("/", get(fetch_data_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("Web server started on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
