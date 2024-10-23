use axum::{extract::State, routing::get, Router};
use chrono::Local;
use maud::{html, Markup};
use std::sync::Arc;

pub(crate) mod state {
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

async fn fetch_data_handler(State(state): State<Arc<state::AppState>>) -> Markup {
    let today = Local::now().date_naive();
    let content = match state.get_prices(&today).await {
        Some(prices) => html!(
            h1 .text-4xl.font-bold.mb-8 { "OTE prices" }
            h2 .text-2xl.font-semibold.mb-4 { "Graph" }
            div .mb-4.flex.justify-center { (prices.render_graph()) }

            h2 .text-2xl.font-semibold.mb-4 { "Table" }
            div .mb-4.flex.justify-center { (prices.render_table()) }
        ),
        None => html!(p { "Error fetching data." }),
    };

    html! {
        html {
            head {
                title { "OTE CR Price Checker" }
                script src="https://cdn.tailwindcss.com" {}
            }
            body .p-4.text-center {
                (content)
            }
        }
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
