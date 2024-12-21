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
                25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0, 38.0,
                39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0
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
