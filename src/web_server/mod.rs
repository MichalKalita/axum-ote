mod conditions;
mod html_render;
mod state;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use chrono::{Local, NaiveDate, Timelike};
use conditions::Eval;
use maud::html;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use html_render::render_layout;

fn create_app(state: state::AppState) -> Router {
    Router::new()
        .route("/", get(fetch_data_handler))
        .route("/optimalizer", get(optimalizer_handler))
        .route("/exp", get(condition_handler))
        .route("/perf", get(perf_handler))
        .with_state(Arc::new(state))
}

pub(crate) async fn start_web_server() {
    let state = state::AppState::new();

    let app = create_app(state);

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

#[derive(Deserialize)]
struct ConditionQuery {
    exp: String,
}

#[derive(Serialize)]
struct ConditionResult {
    result: bool,
    input: conditions::Expression,
    context: conditions::EvaluateContext,
}

async fn condition_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<ConditionQuery>,
) -> Result<Json<ConditionResult>, (StatusCode, String)> {
    let expression = match serde_json::from_str::<conditions::Expression>(query.exp.as_str()) {
        Ok(data) => data,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Expression is not valid".to_string(),
            ))
        }
    };

    let requirements: conditions::ExpressionRequirements = (&expression).into();

    let exp_context = match state.expression_context(requirements).await {
        Some(context) => context,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error creating expression context".to_string(),
            ))
        }
    };

    let result = expression.evaluate(&exp_context);

    Ok(Json(ConditionResult {
        result,
        input: expression,
        context: exp_context,
    }))
}
