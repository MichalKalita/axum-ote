pub(crate) mod conditions;
mod css_gen;
mod html_render;
pub(crate) mod state;

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{NaiveDate, Timelike, Utc};
use chrono_tz::Europe::Prague;
use conditions::{CheapCondition, Condition, Eval};
use maud::html;
use reqwest::StatusCode;
use serde::Deserialize;
use tower_http::compression::CompressionLayer;

use std::sync::Arc;

use html_render::{link, render_layout, ChartSettings, RenderHtml};
use state::{Currency, PriceStats};

fn create_app(state: state::AppState) -> Router {
    Router::new()
        .route("/", get(route_get_root))
        .route("/optimizer", get(route_get_optimizer))
        .route("/opt", get(route_get_opt))
        .layer(CompressionLayer::new())
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
    cur: Option<String>,
    dist: Option<bool>,
}

async fn route_get_root(
    State(state): State<Arc<state::AppState>>,
    query: Query<QueryParams>,
) -> impl IntoResponse {
let now = Utc::now().with_timezone(&Prague);
    let today = now.date_naive();
    let input_date = query.date.unwrap_or(today);
    let currency: Currency = query.cur.as_deref().unwrap_or("eur").parse().unwrap_or(Currency::Eur);
    let include_dist = query.dist.unwrap_or(false);

    let hour = now.time().hour() as usize;
    let minute = (now.time().minute() / 15) as usize;
    let actual_index = if input_date == today {
        hour * 4 + minute
    } else {
        usize::MAX
    };

    let chart = ChartSettings::default();

    let (status, content) = match state.get_prices(&input_date).await {
        Some(prices) => {
            let total_prices = prices.total_prices(&state.distribution);
            let display_prices: Vec<f32> = if include_dist {
                total_prices.clone()
            } else {
                prices.prices.clone()
            };
            let (cheapest_idx, _) = PriceStats::cheapest_hour(&&display_prices[..]);
            let (expensive_idx, _) = PriceStats::expensive_hour(&&display_prices[..]);

            (
                StatusCode::OK,
                html!(
                    h1 .text-4xl.font-bold.mb-8 { "OTE prices " (input_date)}

                    (link("/optimizer", "Optimizer"))

                    div .flex .flex-row .justify-center .gap-2 {
                        (link(format!("/?date={}&cur={}&dist={}", input_date - chrono::Duration::days(1), currency, include_dist).as_str(), "◀"))
                        span .font-bold { (input_date) }
                        (link(format!("/?date={}&cur={}&dist={}", input_date + chrono::Duration::days(1), currency, include_dist).as_str(), "▶"))
                        " | "
                        @if input_date == today {
                            span .font-bold .text-blue-600 .dark:text-blue-400 { "today" }
                        } @else {
                            (link(format!("/?cur={}&dist={}", currency, include_dist).as_str(), format!("today ({})", today).as_str()))
                        }
                        " | "
                        @if matches!(currency, Currency::Eur) {
                            (link(format!("/?date={}&cur=czk&dist={}", input_date, include_dist).as_str(), "CZK"))
                        } @else {
                            (link(format!("/?date={}&cur=eur&dist={}", input_date, include_dist).as_str(), "EUR"))
                        }
                        " | "
                        form method="GET" class="inline-flex items-center gap-1" {
                            input type="hidden" name="date" value=(input_date) {}
                            input type="hidden" name="cur" value=(currency) {}
                            input type="checkbox" id="dist" name="dist" value="true" checked[include_dist] onchange="this.form.submit()" {}
                            label for="dist" { "Include distribution" }
                        }
                    }
                    h2 .text-2xl.font-semibold.mb-4 { "Graph" }
                    div .mb-4.flex.justify-center { (chart.render(&display_prices, Some(&state.distribution.by_hours()), |(index, price)| {
                        if *index == actual_index {
                            "fill-blue-600"
                        } else if *index == cheapest_idx || *price < 0.0 {
                            "fill-green-600"
                        } else if *index == expensive_idx {
                            "fill-red-600"
                        } else {
                            "fill-gray-500"
                        }
                    }, currency)) }

                    h2 .text-2xl.font-semibold.mb-4 { "Table" }
                    div .mb-4.flex.justify-center { (prices.render_table(&state.distribution, actual_index, currency, include_dist)) }
                ),
            )
        },
        None => (StatusCode::NOT_FOUND, html!(p { "Error fetching data." })),
    };

    (status, render_layout(content))
}

#[derive(Deserialize)]
struct OptimalizerQuery {
    exp: Option<String>,
    hours: Option<u8>,
    from: Option<u8>,
    to: Option<u8>,
}

async fn route_get_optimizer(
    State(state): State<Arc<state::AppState>>,
    Query(query): Query<OptimalizerQuery>,
) -> impl IntoResponse {
    let cheap_condition = match query {
        OptimalizerQuery {
            hours: Some(hours),
            from: Some(from),
            to: Some(to),
            ..
        } => Some(CheapCondition { hours, from, to }),
        _ => None,
    };

    let condition = query.exp.as_ref().map(|exp| Condition::try_from(exp));

    let condition = match condition {
        Some(Ok(data)) => data,
        Some(Err(err)) => return Err(format!("Error parsing expression: {}", err)),
        None => match cheap_condition {
            Some(ref cheap_condition) => Condition::Cheap(cheap_condition.clone()),
            None => Condition::And(vec![]),
        },
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
        h1 .text-4xl.font-bold.mb-8 { "Optimizer, find cheapist hours" }

        (link("/", "Homepage"))

        div .text-left {
            h2 .text-2xl.font-semibold.mb-4 { "Condition" }

            (cheap_condition.render_html())

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
