mod builder;
mod conditions;
mod html_render;
mod state;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json},
    routing::get,
    Form, Router,
};
use builder::additional_condition;
use chrono::{Local, NaiveDate, Timelike};
use conditions::{ChangeRequest, Condition, Eval};
use maud::html;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

use html_render::render_layout;

fn create_app(state: state::AppState) -> Router {
    Router::new()
        .route("/", get(fetch_data_handler))
        .route(
            "/builder",
            get(builder_handler).post(builder_update_handler),
        )
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
    exp: Option<String>,
}

async fn builder_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<OptimalizerQuery>,
) -> impl IntoResponse {
    let condition = query.exp.as_ref().map(|exp| Condition::try_from(exp));

    let condition = match condition {
        Some(Ok(data)) => data,
        Some(Err(err)) => return Err(format!("Error parsing expression: {}", err)),
        None => Condition::And(vec![]),
    };

    let exp_context = match state.expression_context().await {
        Some(context) => context,
        None => return Err("Error creating expression context".into()),
    };

    let content = html!(
        h1 .text-4xl.font-bold.mb-8 { "Optimalizer, find cheapist hours" }

        h2 .text-2xl.font-semibold.mb-4 { "Actual expression" }
        pre {
            (format!("{:?}", condition))
        }

        h2 .text-2xl.font-semibold.mb-4 { "Result" }
        pre {
            (format!("{:?}", condition.evaluate(&exp_context)))
        }

        h2 .text-2xl.font-semibold.mb-4 { "Builder" }
        div .builder.text-left {
            (builder::builder(&condition))
        }
    );

    Ok(render_layout(content))
}

async fn builder_update_handler(
    query: Query<OptimalizerQuery>,
    form_data: Form<ChangeRequest>,
) -> impl IntoResponse {
    let condition = query.exp.as_ref().map(|exp| Condition::try_from(exp));
    let mut condition = match condition {
        Some(Ok(data)) => data,
        Some(Err(err)) => return Err(format!("Error parsing expression: {}", err)),
        None => Condition::And(vec![]),
    };

    let (diff, new_position) = condition.apply_changes(&form_data)?;

    let exp: &String = &condition.try_into().unwrap();
    let url = format!("/builder?exp={}", exp);

    let response = additional_condition(&diff, new_position);

    Ok(([("Location", url.clone()), ("HX-Push-Url", url)], response))
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
    input: Condition,
    context: conditions::EvaluateContext,
}

async fn condition_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<ConditionQuery>,
) -> Result<Json<ConditionResult>, (StatusCode, String)> {
    let expression: Condition = match (&query.exp).try_into() {
        Ok(data) => data,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Expression is not valid".to_string(),
            ))
        }
    };

    let exp_context = match state.expression_context().await {
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
