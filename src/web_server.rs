use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use chrono::{Local, NaiveDate, Timelike};
use log::debug;
use logic::Eval;
use maud::html;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::html_render::render_layout;

pub(crate) mod state {
    use core::f32;

    use crate::data_loader::fetch_data;
    use chrono::Timelike;
    use dashmap::DashMap;
    use serde::Serialize;

    use super::logic::{EvaluateContext, ExpressionRequirements};

    #[derive(Serialize, Clone)]
    pub struct DayPrices {
        pub prices: [f32; 24],
        // pub date: chrono::NaiveDate,
    }

    pub trait PriceStats {
        fn cheapest_hour(&self) -> (usize, f32);
        fn expensive_hour(&self) -> (usize, f32);
    }

    impl PriceStats for [f32; 24] {
        fn cheapest_hour(&self) -> (usize, f32) {
            self.iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(index, price)| (index, price.clone()))
                .unwrap()
        }

        fn expensive_hour(&self) -> (usize, f32) {
            self.iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(index, price)| (index, price.clone()))
                .unwrap()
        }
    }

    impl DayPrices {
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

        pub async fn expression_context(
            &self,
            requirements: super::logic::ExpressionRequirements,
        ) -> Option<super::logic::EvaluateContext> {
            let now = chrono::Local::now();
            let date = now.date_naive();
            let hour = now.time().hour();
            let prices = self.get_prices(&date).await?.prices;

            match requirements {
                ExpressionRequirements {
                    hours_ago,
                    hours_future,
                } if hours_ago == 0 && hours_future == 0 => Some(EvaluateContext::new(
                    prices.to_vec(),
                    hour.try_into().unwrap(),
                )),

                ExpressionRequirements {
                    hours_ago,
                    hours_future,
                } => todo!(),
            }
        }
    }
}

fn create_app(state: state::AppState) -> Router {
    Router::new()
        .route("/", get(fetch_data_handler))
        .route("/optimalizer", get(optimalizer_handler))
        .route("/exp", get(expression_handler))
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

pub(crate) mod logic {
    use std::ops;

    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone)]
    pub enum Expression {
        #[serde(rename = "and")]
        And(Box<Expression>, Box<Expression>),
        #[serde(rename = "or")]
        Or(Box<Expression>, Box<Expression>),
        #[serde(rename = "not")]
        Not(Box<Expression>),
        #[serde(rename = "if")]
        Condition(Condition),
    }

    #[derive(Serialize, Deserialize, Clone, Copy)]
    pub enum Condition {
        #[serde(rename = "price")]
        PriceLowerThan(f32),
        #[serde(rename = "percentile")]
        Percentile(f32),
        PercentileInRange(f32, Range),
    }

    #[derive(Serialize, Deserialize, Clone, Copy)]
    pub enum Range {
        #[serde(rename = "today")]
        Today,
        #[serde(rename = "future")]
        Future,
        #[serde(rename = "range")]
        PlusMinusHours(u8),
        #[serde(rename = "static")]
        StaticHours(u8, u8),
    }

    impl Range {
        fn apply(&self, ctx: &EvaluateContext) -> LimitedRange {
            match self {
                Range::Today => {
                    let start = (ctx.target_price_index / 24) * 24;
                    let prices = ctx.prices[start..(start + 24)].to_vec();

                    LimitedRange {
                        index: ctx.target_price_index - start,
                        prices,
                    }
                }
                Range::Future => LimitedRange {
                    index: 0,
                    prices: ctx.prices[ctx.target_price_index..].to_vec(),
                },
                Range::PlusMinusHours(hours) => {
                    let start = ctx.target_price_index.saturating_sub(*hours as usize);
                    let end = ctx
                        .target_price_index
                        .saturating_add(1)
                        .saturating_add(*hours as usize);
                    let prices = ctx.prices[start..end].to_vec();

                    LimitedRange {
                        index: *hours as usize,
                        prices,
                    }
                }
                Range::StaticHours(_start_hour, _end_hour) => {
                    // assert!(startHour < endHour, "start hour is lower than end hour");

                    // LimitedRange {
                    //     index
                    // }

                    todo!()
                }
            }
        }
    }

    #[cfg(test)]
    mod range_tests {
        use super::*;

        fn setup() -> EvaluateContext {
            EvaluateContext {
                prices: (0..48).map(|i| i as f32).collect(),
                target_price_index: 25, // 2. hour in second day
            }
        }

        #[test]
        fn test_apply_today() {
            let ctx = setup();
            let result = Range::Today.apply(&ctx);

            assert_eq!(result.index, 1); // 2. hour
            assert_eq!(
                result.prices,
                vec![
                    // 0. hour - 12. hour
                    24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0,
                    // 12. hour - 24. hour
                    36.0, 37.0, 38.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0
                ]
            );
        }

        #[test]
        fn test_apply_future() {
            let ctx = setup();
            let result = Range::Future.apply(&ctx);

            assert_eq!(result.index, 0);
            assert_eq!(
                result.prices,
                vec![
                    25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0,
                    38.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0
                ]
            );
        }

        #[test]
        fn test_apply_plus_minus_hours() {
            let ctx = setup();

            let result = Range::PlusMinusHours(1).apply(&ctx);
            assert_eq!(result.index, 1);
            assert_eq!(result.prices, vec![24.0, 25.0, 26.0]);

            let result = Range::PlusMinusHours(3).apply(&ctx);
            assert_eq!(result.index, 3);
            assert_eq!(
                result.prices,
                vec![22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0]
            );
        }
    }

    struct LimitedRange {
        index: usize,
        prices: Vec<f32>,
    }

    impl LimitedRange {
        /// Percentile with 0.0 as lowest price and 1.0 as highest price
        fn percentile(&mut self) -> f32 {
            assert!(!self.prices.is_empty(), "Empty input");
            assert!(self.index < self.prices.len(), "Out of bound index");

            if self.prices.len() == 1 {
                return 1.0;
            }

            // Sort prices in ascending order
            self.prices.sort_by(|a, b| a.partial_cmp(b).unwrap());

            // Get the target price based on the index
            let target_price = self.prices[self.index];

            // Find the position of the target price in the sorted array
            let position = self.prices.iter().position(|&x| x == target_price).unwrap();

            // Calculate the percentile as the ratio of the position to the array length
            (position as f32) / (self.prices.len() as f32 - 1.0)
        }
    }

    #[cfg(test)]
    mod limited_range_tests {
        use super::*;

        #[test]
        fn test_percentile_low() {
            let mut range = LimitedRange {
                index: 0,
                prices: vec![1.0, 2.0, 3.0],
            };

            assert_eq!(range.percentile(), 0.0);
        }

        #[test]
        fn test_percentile_high() {
            let mut range = LimitedRange {
                index: 2,
                prices: vec![1.0, 2.0, 3.0],
            };

            assert_eq!(range.percentile(), 1.0);
        }

        #[test]
        fn test_percentile_middle() {
            let mut range = LimitedRange {
                index: 1,
                prices: vec![1.0, 2.0, 3.0],
            };

            assert_eq!(range.percentile(), 0.5);
        }

        #[test]
        fn test_percentile_single() {
            let mut range = LimitedRange {
                index: 0,
                prices: vec![1.0],
            };

            assert_eq!(range.percentile(), 1.0);
        }

        #[test]
        #[should_panic(expected = "Empty input")]
        fn test_percentile_empty() {
            LimitedRange {
                index: 0,
                prices: vec![],
            }
            .percentile();
        }

        #[test]
        #[should_panic(expected = "Out of bound index")]
        fn test_percentile_out_of_bound() {
            LimitedRange {
                index: 2,
                prices: vec![1.0],
            }
            .percentile();
        }
    }

    pub trait Eval {
        fn evaluate(&self, ctx: &EvaluateContext) -> bool;
    }

    impl Eval for Expression {
        fn evaluate(&self, ctx: &EvaluateContext) -> bool {
            match self {
                Expression::And(a, b) => a.evaluate(ctx) && b.evaluate(ctx),
                Expression::Or(a, b) => a.evaluate(ctx) || b.evaluate(ctx),
                Expression::Not(a) => !a.evaluate(ctx),
                Expression::Condition(cond) => cond.evaluate(ctx),
            }
        }
    }

    impl Eval for Condition {
        fn evaluate(&self, ctx: &EvaluateContext) -> bool {
            match self {
                Condition::PriceLowerThan(price) => ctx.prices[ctx.target_price_index] <= *price,
                Condition::PercentileInRange(target_percentile, range) => {
                    let calculated_percentile = range.apply(&ctx).percentile();
                    calculated_percentile <= *target_percentile
                }
                Condition::Percentile(target_percentile) => {
                    Condition::PercentileInRange(*target_percentile, Range::Today).evaluate(ctx)
                }
            }
        }
    }

    #[derive(Serialize, Debug)]
    pub(crate) struct EvaluateContext {
        // start_date: chrono::NaiveDate,
        prices: Vec<f32>,
        target_price_index: usize,
    }

    impl EvaluateContext {
        pub(crate) fn new(prices: Vec<f32>, target_price_index: usize) -> Self {
            Self {
                prices,
                target_price_index,
            }
        }
    }

    pub(crate) struct ExpressionRequirements {
        pub hours_ago: u8,
        pub hours_future: u8,
    }

    impl ExpressionRequirements {
        pub(crate) fn new(hours_ago: u8, hours_future: u8) -> Self {
            Self {
                hours_ago,
                hours_future,
            }
        }
    }

    impl ops::Add<ExpressionRequirements> for ExpressionRequirements {
        type Output = ExpressionRequirements;

        fn add(self, rhs: ExpressionRequirements) -> Self::Output {
            ExpressionRequirements {
                hours_ago: self.hours_ago.max(rhs.hours_ago),
                hours_future: self.hours_future.max(rhs.hours_future),
            }
        }
    }

    impl From<&Expression> for ExpressionRequirements {
        fn from(value: &Expression) -> Self {
            match value {
                Expression::And(a, b) | Expression::Or(a, b) => {
                    let a: ExpressionRequirements = (&**a).into();
                    let b: ExpressionRequirements = (&**b).into();

                    a + b
                }
                Expression::Not(exp) => {
                    let exp: ExpressionRequirements = (&**exp).into();

                    exp
                }
                Expression::Condition(Condition::PercentileInRange(_, range)) => match range {
                    Range::Today => todo!(),
                    Range::Future => todo!(),
                    Range::PlusMinusHours(x) => ExpressionRequirements::new(*x, *x),
                    Range::StaticHours(_, _) => todo!(),
                },
                Expression::Condition(_) => ExpressionRequirements::new(0, 0),
            }
        }
    }
}

#[derive(Deserialize)]
struct ConditionQuery {
    exp: String,
}

#[derive(Serialize)]
struct ConditionResult {
    result: bool,
    input: logic::Expression,
    context: logic::EvaluateContext,
}

async fn expression_handler(
    State(state): State<Arc<state::AppState>>,
    query: Query<ConditionQuery>,
) -> Result<Json<ConditionResult>, (StatusCode, String)> {
    let expression = match serde_json::from_str::<logic::Expression>(query.exp.as_str()) {
        Ok(data) => data,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Expression is not valid".to_string(),
            ))
        }
    };

    let requirements: logic::ExpressionRequirements = (&expression).into();

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
