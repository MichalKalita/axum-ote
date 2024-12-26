use chrono::{NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Condition {
    #[serde(rename = "and")]
    And(Vec<Condition>),
    #[serde(rename = "or")]
    Or(Vec<Condition>),
    #[serde(rename = "not")]
    Not(Box<Condition>),

    #[serde(rename = "price")]
    PriceLowerThan(f32),
    #[serde(rename = "hours")]
    Hours(u32, u32),
    #[serde(rename = "percentile")]
    PercentileInRange { value: f32, range: Range },
}

pub trait Eval {
    fn evaluate(&self, ctx: &EvaluateContext) -> bool;
}

impl Eval for Condition {
    fn evaluate(&self, ctx: &EvaluateContext) -> bool {
        match self {
            Condition::And(items) => items.iter().all(|i| i.evaluate(ctx)),
            Condition::Or(items) => items.iter().any(|i| i.evaluate(ctx)),
            Condition::Not(item) => !item.evaluate(ctx),
            Condition::PriceLowerThan(price) => ctx.prices.prices[ctx.prices.now_index] <= *price,
            Condition::Hours(min, max) => {
                let hour = ctx.now.hour() + 1;

                *min <= hour && hour < *max
            }
            Condition::PercentileInRange {
                value: target_percentile,
                range,
            } => {
                let actual_percentile = ctx.limit(*range).percentile();

                actual_percentile >= *target_percentile
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum Range {
    #[serde(rename = "today")]
    Today,
    #[serde(rename = "future")]
    Future,
    #[serde(rename = "range")]
    PlusMinusHours(u32),
}

#[derive(Serialize, Debug)]
pub(crate) struct EvaluateContext {
    now: NaiveDateTime,
    prices: PricesContext,
}

impl EvaluateContext {
    pub(crate) fn new(now: NaiveDateTime, prices: Vec<f32>, target_price_index: usize) -> Self {
        Self {
            now,
            prices: PricesContext {
                prices,
                now_index: target_price_index,
            },
        }
    }

    fn limit(&self, range: Range) -> PricesContext {
        match range {
            Range::Today => {
                let start = (self.prices.now_index / 24) * 24;
                let prices = self.prices.prices[start..(start + 24)].to_vec();

                PricesContext {
                    now_index: self.prices.now_index - start,
                    prices,
                }
            }
            Range::Future => PricesContext {
                now_index: 0,
                prices: self.prices.prices[self.prices.now_index..].to_vec(),
            },
            Range::PlusMinusHours(hours) => {
                let start = self.prices.now_index.saturating_sub(hours as usize);
                let end = self
                    .prices
                    .now_index
                    .saturating_add(1)
                    .saturating_add(hours as usize);
                let prices = self.prices.prices[start..end].to_vec();

                PricesContext {
                    now_index: hours as usize,
                    prices,
                }
            }
        }
    }
}

#[cfg(test)]
mod evaluate_context_tests {
    use super::*;

    fn setup() -> EvaluateContext {
        EvaluateContext::new(
            NaiveDateTime::parse_from_str("2020-01-01 02:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            (0..48).map(|i| i as f32).collect(),
            25, // 2. hour in second day
        )
    }

    #[test]
    fn test_apply_today() {
        let ctx = setup();
        let result = ctx.limit(Range::Today);

        assert_eq!(result.now_index, 1); // 2. hour
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
        let result = ctx.limit(Range::Future);

        assert_eq!(result.now_index, 0);
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

        let result = ctx.limit(Range::PlusMinusHours(1));
        assert_eq!(result.now_index, 1);
        assert_eq!(result.prices, vec![24.0, 25.0, 26.0]);

        let result = ctx.limit(Range::PlusMinusHours(3));
        assert_eq!(result.now_index, 3);
        assert_eq!(
            result.prices,
            vec![22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0]
        );
    }
}

#[derive(Serialize, Debug)]
struct PricesContext {
    prices: Vec<f32>,
    now_index: usize,
}

impl PricesContext {
    fn percentile(&self) -> f32 {
        assert!(!self.prices.is_empty(), "Empty input");
        assert!(self.now_index < self.prices.len(), "Out of bound index");

        if self.prices.len() == 1 {
            return 1.0;
        }

        // Get the target price based on the index
        let target_price = self.prices[self.now_index];

        // Sort prices in ascending order
        let mut sorted = self.prices.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Find the position of the target price in the sorted array
        let position = sorted.iter().position(|&x| x == target_price).unwrap();

        // Calculate the percentile as the ratio of the position to the array length
        (position as f32) / (self.prices.len() as f32 - 1.0)
    }
}

#[cfg(test)]
mod prices_context_test {
    use super::PricesContext;

    #[test]
    fn test_percentile_low() {
        let range = PricesContext {
            now_index: 0,
            prices: vec![1.0, 2.0, 3.0],
        };

        assert_eq!(range.percentile(), 0.0);
    }

    #[test]
    fn test_percentile_high() {
        let range = PricesContext {
            now_index: 2,
            prices: vec![1.0, 2.0, 3.0],
        };

        assert_eq!(range.percentile(), 1.0);
    }

    #[test]
    fn test_percentile_middle() {
        let range = PricesContext {
            now_index: 1,
            prices: vec![1.0, 2.0, 3.0],
        };

        assert_eq!(range.percentile(), 0.5);
    }

    #[test]
    fn test_percentile_single() {
        let range = PricesContext {
            now_index: 0,
            prices: vec![1.0],
        };

        assert_eq!(range.percentile(), 1.0);
    }

    #[test]
    #[should_panic(expected = "Empty input")]
    fn test_percentile_empty() {
        PricesContext {
            now_index: 0,
            prices: vec![],
        }
        .percentile();
    }

    #[test]
    #[should_panic(expected = "Out of bound index")]
    fn test_percentile_out_of_bound() {
        PricesContext {
            now_index: 2,
            prices: vec![1.0],
        }
        .percentile();
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

impl From<&Condition> for ExpressionRequirements {
    fn from(_value: &Condition) -> Self {
        ExpressionRequirements::new(0, 0)

        // match value {
        //     Expression::And(a, b) | Expression::Or(a, b) => {
        //         let a: ExpressionRequirements = (&**a).into();
        //         let b: ExpressionRequirements = (&**b).into();

        //         a + b
        //     }
        //     Expression::Not(exp) => {
        //         let exp: ExpressionRequirements = (&**exp).into();

        //         exp
        //     }
        //     Expression::Condition(Condition::PercentileInRange(_, range)) => match range {
        //         Range::Today => todo!(),
        //         Range::Future => todo!(),
        //         Range::PlusMinusHours(x) => ExpressionRequirements::new(*x, *x),
        //         Range::StaticHours(_, _) => todo!(),
        //     },
        //     Expression::Condition(_) => ExpressionRequirements::new(0, 0),
        // }
    }
}
