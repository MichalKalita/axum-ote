mod conditions;
mod html_render;
mod state;

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{NaiveDate, Timelike, Utc};
use chrono_tz::Europe::Prague;
use conditions::{Condition, Eval};
use maud::html;
use reqwest::StatusCode;
use serde::Deserialize;

use std::sync::Arc;

use html_render::{link, render_layout, ChartSettings, RenderHtml};

fn create_app(state: state::AppState) -> Router {
    Router::new()
        .route("/", get(route_get_root))
        .route("/optimizer", get(route_get_optimizer))
        .route("/opt", get(route_get_opt))
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

async fn route_get_root(
    State(state): State<Arc<state::AppState>>,
    query: Query<QueryParams>,
) -> impl IntoResponse {
    let now = Utc::now().with_timezone(&Prague);
    let today = now.date_naive();
    let input_date = query.date.unwrap_or(today);

    let hour = now.time().hour() as usize;
    let active_hour = if input_date == today {
        hour
    } else {
        usize::MAX
    };

    let chart = ChartSettings::default();

    let (status, content) = match state.get_prices(&input_date).await {
        Some(prices) => (
            StatusCode::OK,
            html!(
                h1 .text-4xl.font-bold.mb-8 { "OTE prices " (input_date) }

                (link("/optimizer", "Optimalizer"))

                div .flex .flex-row .justify-center .gap-2 {
                    (link(format!("/?date={}", input_date - chrono::Duration::days(1)).as_str(), "Previous day"))
                    " | "
                    (link("/", format!("today ({})", today).as_str()))
                    " | "
                    (link(format!("/?date={}", input_date + chrono::Duration::days(1)).as_str(), "Next day"))
                }

                h2 .text-2xl.font-semibold.mb-4 { "Graph" }
                div .mb-4.flex.justify-center { (chart.render(&prices.prices, Some(&state.distribution.by_hours()), |(index, _price)| { if *index == active_hour { "fill-green-600" } else { "fill-blue-600" } })) }

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

async fn route_get_optimizer(
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

    const DOMAIN: &str = "https://ota.kalita.cz";
    let automation_url = format!(
        "{DOMAIN}/opt?exp={}",
        query.exp.clone().unwrap_or("".into())
    );

    let examples = [r#"/optimizer?exp=[{"price":120},{"hours":[0,10]}]"#];

    let content = html!(
        h1 .text-4xl.font-bold.mb-8 { "Optimalizer, find cheapist hours" }

        (link("/", "Homepage"))

        div .text-left {
            h2 .text-2xl.font-semibold.mb-4 { "Condition" }
            (&condition.render_html())

            h2 .text-2xl.font-semibold.mb-4 { "Evaluation" }
            pre {
                (format!("{:?}", condition.evaluate(&exp_context)))
            }
            a href=(automation_url) { "URL for automation tools " (automation_url) }

            h2 .text-2xl.font-semibold.mb-4 { "Evaluate in Chart" }
            div .mb-4.flex.justify-center { (condition.evaluate_all_in_chart(&exp_context)) }

            h2 .text-2xl.font-semibold.mb-4 { "Examples" }
            ul {
                @for example in examples.iter() {
                    li { (link(example, example)) }
                }
            }
        }
    );

    Ok(render_layout(content))
}

async fn route_get_opt(
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

    let result = condition.evaluate(&exp_context);

    Ok(format!("{:?}", result))
}
