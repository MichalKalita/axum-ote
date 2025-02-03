use chrono::{NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

use super::builder::Position;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Condition {
    #[serde(rename = "and")]
    And(Vec<Condition>),
    #[serde(rename = "or")]
    Or(Vec<Condition>),
    #[serde(rename = "not")]
    Not(Box<Condition>),

    #[serde(rename = "price")]
    Price(f32),
    #[serde(rename = "hours")]
    Hours(u32, u32),
    #[serde(rename = "percentile")]
    Percentile { value: f32, range: Range },

    #[cfg(test)]
    Debug(bool),
}

pub trait Eval {
    fn evaluate(&self, ctx: &EvaluateContext) -> bool;
}

impl Eval for Condition {
    fn evaluate(&self, ctx: &EvaluateContext) -> bool {
        match self {
            Condition::And(items) if items.len() > 0 => items.iter().all(|i| i.evaluate(ctx)),
            Condition::Or(items) if items.len() > 0 => items.iter().any(|i| i.evaluate(ctx)),
            Condition::And(_) | Condition::Or(_) => false,

            Condition::Not(item) => !item.evaluate(ctx),
            Condition::Price(price) => ctx.prices.prices[ctx.prices.now_index] <= *price,
            Condition::Hours(min, max) => {
                let hour = ctx.now.hour();

                *min <= hour && hour <= *max
            }
            Condition::Percentile {
                value: target_percentile,
                range,
            } => {
                let actual_percentile = ctx.limit(*range).percentile();
                println!("actual percentile {}", actual_percentile);

                actual_percentile <= *target_percentile
            }

            #[cfg(test)]
            Condition::Debug(state) => *state,
        }
    }
}

impl TryFrom<&String> for Condition {
    type Error = json5::Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        let items = json5::from_str::<Vec<Condition>>(value)?;
        Ok(Condition::And(items))
    }
}

impl TryFrom<Condition> for String {
    type Error = json5::Error;

    fn try_from(value: Condition) -> Result<Self, Self::Error> {
        let items = match value {
            Condition::And(items) => items,
            _ => {
                return Err(json5::Error::Message {
                    msg: "Other than And is not supported".to_string(),
                    location: None,
                })
            }
        };
        json5::to_string(&items)
    }
}

#[cfg(test)]
mod condition_tests {
    use chrono::NaiveDateTime;

    use crate::web_server::conditions::Eval;

    use super::{Condition, EvaluateContext, Range};

    fn setup() -> EvaluateContext {
        EvaluateContext::new(
            NaiveDateTime::parse_from_str("2020-01-01 02:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            (0..24).map(|i| i as f32).collect(),
            1, // 2. hour
        )
    }

    #[test]
    fn test_price() {
        let ctx = setup();

        let condition = Condition::Price(100.0);
        assert!(condition.evaluate(&ctx));

        let condition = Condition::Price(0.0);
        assert!(!condition.evaluate(&ctx));
    }

    #[test]
    fn test_hours() {
        let ctx = setup();

        assert!(Condition::Hours(0, 2).evaluate(&ctx));
        assert!(!Condition::Hours(3, 4).evaluate(&ctx));
        assert!(Condition::Hours(1, 3).evaluate(&ctx));
    }

    #[test]
    fn test_percentile() {
        let ctx = setup();

        let result = Condition::Percentile {
            value: 0.05,
            range: Range::Today, // calculated percentil is 0.04347826
        }
        .evaluate(&ctx);
        assert_eq!(result, true);

        let result = Condition::Percentile {
            value: 0.04,
            range: Range::Today, // calculated percentil is 0.04347826
        }
        .evaluate(&ctx);
        assert_eq!(result, false);

        let result = Condition::Percentile {
            value: 0.0,
            range: Range::Future,
        }
        .evaluate(&ctx);
        assert_eq!(
            result, true,
            "this is cheapiest one price in now and future"
        );

        let result = Condition::Percentile {
            value: 0.51,
            range: Range::PlusMinusHours(1),
        }
        .evaluate(&ctx);
        assert_eq!(result, true);

        let result = Condition::Percentile {
            value: 0.49,
            range: Range::PlusMinusHours(1),
        }
        .evaluate(&ctx);
        assert_eq!(result, false);
    }

    #[test]
    fn test_not() {
        let ctx = setup();

        assert_eq!(
            Condition::Not(Box::new(Condition::Debug(true))).evaluate(&ctx),
            false
        );
        assert_eq!(
            Condition::Not(Box::new(Condition::Debug(false))).evaluate(&ctx),
            true
        );
    }

    #[test]
    fn test_and() {
        let ctx = setup();

        // Empty is false
        assert_eq!(Condition::And(vec![]).evaluate(&ctx), false);

        // Single value have same result
        assert_eq!(
            Condition::And(vec![Condition::Debug(true)]).evaluate(&ctx),
            true
        );
        assert_eq!(
            Condition::And(vec![Condition::Debug(false)]).evaluate(&ctx),
            false
        );

        // Combination table
        assert_eq!(
            Condition::And(vec![Condition::Debug(true), Condition::Debug(true)]).evaluate(&ctx),
            true
        );
        assert_eq!(
            Condition::And(vec![Condition::Debug(true), Condition::Debug(false)]).evaluate(&ctx),
            false
        );
        assert_eq!(
            Condition::And(vec![Condition::Debug(false), Condition::Debug(true)]).evaluate(&ctx),
            false
        );
        assert_eq!(
            Condition::And(vec![Condition::Debug(false), Condition::Debug(false)]).evaluate(&ctx),
            false
        );
    }

    #[test]
    fn test_or() {
        let ctx = setup();

        // Empty is false
        assert_eq!(Condition::Or(vec![]).evaluate(&ctx), false);

        // Single value have same result
        assert_eq!(
            Condition::Or(vec![Condition::Debug(true)]).evaluate(&ctx),
            true
        );
        assert_eq!(
            Condition::Or(vec![Condition::Debug(false)]).evaluate(&ctx),
            false
        );

        // Combination table
        assert_eq!(
            Condition::Or(vec![Condition::Debug(true), Condition::Debug(true)]).evaluate(&ctx),
            true
        );
        assert_eq!(
            Condition::Or(vec![Condition::Debug(true), Condition::Debug(false)]).evaluate(&ctx),
            true
        );
        assert_eq!(
            Condition::Or(vec![Condition::Debug(false), Condition::Debug(true)]).evaluate(&ctx),
            true
        );
        assert_eq!(
            Condition::Or(vec![Condition::Debug(false), Condition::Debug(false)]).evaluate(&ctx),
            false
        );
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
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
    fn test_percentile_random() {
        let range = PricesContext {
            now_index: 1,
            prices: vec![1.0, 2.0, 3.0, 4.0, 5.0],
        };

        assert_eq!(range.percentile(), 0.25);
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

#[derive(Deserialize, Debug)]
pub struct ChangeRequest {
    #[serde(deserialize_with = "deserialize_id")]
    pub id: Vec<u8>, // todo: change to builder Position
    #[serde(flatten)]
    pub payload: ChangeRequestPayload,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase", untagged)]
pub enum ChangeRequestPayload {
    Extend { extend: ChangeRequestExtenstion },
    Price { price: f32 },
    Hours { from: u32, to: u32 },
    // Percentile,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ChangeRequestExtenstion {
    And,
    Or,
    Not,
    Price,
    Hours,
    Percentile,
}

fn deserialize_id<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(Vec::new())
    } else {
        s.split('.')
            .map(|part| part.parse::<u8>().map_err(serde::de::Error::custom))
            .collect()
    }
}

impl Condition {
    pub fn apply_changes(
        &mut self,
        changes: &ChangeRequest,
    ) -> Result<(Condition, Position), String> {
        // changes have id [0,1,2] => 1. element in And, 2. element inside, and 3. element inside previous
        // change payload will be applied to the last element
        // Err if fails

        let mut current_condition: &mut Condition = self;
        for &index in &changes.id {
            match current_condition {
                Condition::And(ref mut items) | Condition::Or(ref mut items) => {
                    if let Some(next_condition) = items.get_mut(index as usize) {
                        current_condition = next_condition;
                    } else {
                        return Err("Index out of bounds".to_string());
                    }
                }
                _ => return Err("Unsupported condition type for traversal".to_string()),
            }
        }

        let position = Position::from(&changes.id);

        match (current_condition, &changes.payload) {
            (Condition::And(vec), ChangeRequestPayload::Extend { extend })
            | (Condition::Or(vec), ChangeRequestPayload::Extend { extend }) => {
                let diff = match extend {
                    ChangeRequestExtenstion::And => Condition::And(vec![]),
                    ChangeRequestExtenstion::Or => Condition::Or(vec![]),
                    ChangeRequestExtenstion::Not => {
                        Condition::Not(Box::new(Condition::And(vec![])))
                    }
                    ChangeRequestExtenstion::Price => Condition::Price(0.0),
                    ChangeRequestExtenstion::Hours => Condition::Hours(0, 0),
                    ChangeRequestExtenstion::Percentile => Condition::Percentile {
                        value: 0.0,
                        range: Range::Today,
                    },
                };
                vec.push(diff.clone());
                let position = position.extend(vec.len() as u8 - 1);

                Ok((diff, position))
            }
            (Condition::Not(_condition), ChangeRequestPayload::Extend { extend: _ }) => todo!(),
            (Condition::Price(ref mut price_ref), ChangeRequestPayload::Price { price }) => {
                *price_ref = *price;
                Ok((Condition::Price(*price_ref), position))
            }
            (
                Condition::Hours(ref mut from_ref, ref mut to_ref),
                ChangeRequestPayload::Hours { from, to },
            ) => {
                *from_ref = *from;
                *to_ref = *to;
                Ok((Condition::Hours(*from_ref, *to_ref), position))
            }
            _ => Err("Unsupported combination of condition and payload".to_string()),
        }
    }
}

#[cfg(test)]
mod change_request_tests {
    use super::*;

    #[test]
    fn test_simple_extending() {
        let mut condition = Condition::And(vec![Condition::Price(100.0), Condition::Hours(0, 2)]);
        let request = ChangeRequest {
            id: vec![],
            payload: ChangeRequestPayload::Extend {
                extend: ChangeRequestExtenstion::Or,
            },
        };

        let result = condition.apply_changes(&request).unwrap();
        assert_eq!(result, (Condition::Or(vec![]), Position::from(&vec![2])));

        assert_eq!(
            condition,
            Condition::And(vec![
                Condition::Price(100.0),
                Condition::Hours(0, 2),
                Condition::Or(vec![]),
            ])
        );
    }

    #[test]
    fn test_advanced_extending() {
        let mut condition = Condition::And(vec![
            Condition::Price(100.0),
            Condition::Hours(0, 2),
            Condition::Or(vec![]),
        ]);
        let request = ChangeRequest {
            id: vec![2],
            payload: ChangeRequestPayload::Extend {
                extend: ChangeRequestExtenstion::Price,
            },
        };

        let result = condition.apply_changes(&request).unwrap();
        assert_eq!(result, (Condition::Price(0.0), Position::from(&vec![2, 0])));

        assert_eq!(
            condition,
            Condition::And(vec![
                Condition::Price(100.0),
                Condition::Hours(0, 2),
                Condition::Or(vec![Condition::Price(0.0),]),
            ])
        );
    }

    #[test]
    fn test_bad_id() {
        let mut condition = Condition::And(vec![]);
        let request = ChangeRequest {
            id: vec![3],
            payload: ChangeRequestPayload::Extend {
                extend: ChangeRequestExtenstion::Price,
            },
        };

        let result = condition.apply_changes(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_change_price() {
        let mut condition = Condition::And(vec![
            Condition::Price(100.0),
            Condition::Hours(0, 2),
            Condition::Or(vec![]),
        ]);
        let request = ChangeRequest {
            id: vec![0],
            payload: ChangeRequestPayload::Price { price: 50.0 },
        };

        condition.apply_changes(&request).unwrap();

        assert_eq!(
            condition,
            Condition::And(vec![
                Condition::Price(50.0),
                Condition::Hours(0, 2),
                Condition::Or(vec![]),
            ])
        );
    }
}
