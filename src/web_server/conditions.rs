use chrono::{NaiveDateTime, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Condition {
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Not(Box<Condition>),

    Price(f32),
    Hours(u32, u32),
    Percentile {
        value: f32,
        range: Range,
    },
    Cheap {
        hours: u8,
        from: u8,
        to: u8,
    },

    #[cfg(test)]
    Debug(bool),
}

pub trait Eval {
    fn evaluate(&self, ctx: &EvaluateContext) -> bool;
    fn evaluate_all(&self, ctx: &EvaluateContext) -> Vec<bool>;
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
            } => match ctx.limit(*range) {
                Some(prices) => prices.percentile() <= *target_percentile,
                None => return false,
            },
            Condition::Cheap { hours, from, to } => {
                let prices = ctx.slice(*from as usize, *to as usize);

                if let Some(mut prices) = prices {
                    prices.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let actual_price = ctx.actual_price();
                    let actual_price_position = prices
                        .iter()
                        .position(|price| actual_price < *price)
                        .unwrap_or(prices.len());
                    actual_price_position <= *hours as usize
                } else {
                    false
                }
            }

            #[cfg(test)]
            Condition::Debug(state) => *state,
        }
    }

    fn evaluate_all(&self, ctx: &EvaluateContext) -> Vec<bool> {
        let start_time = ctx
            .now
            .checked_sub_signed(TimeDelta::hours(ctx.prices.now_index as i64))
            .expect("Time overflow");

        ctx.prices
            .prices
            .iter()
            .enumerate()
            .map(|(index, _price)| {
                let updated_ctx = EvaluateContext {
                    now: start_time
                        .checked_add_signed(TimeDelta::hours(index as i64))
                        .expect("Time overflow"),
                    prices: PricesContext {
                        prices: ctx.prices.prices.clone(),
                        now_index: index,
                    },
                };

                self.evaluate(&updated_ctx)
            })
            .collect::<Vec<bool>>()
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
            2, // 2:00 - 2:59
        )
    }

    #[test]
    fn test_price() {
        let ctx = setup();

        let condition = Condition::Price(100.0);
        assert!(condition.evaluate(&ctx));
        assert_eq!(condition.evaluate_all(&ctx), [true; 24]);

        let condition = Condition::Price(0.0);
        assert!(!condition.evaluate(&ctx));
        assert_eq!(
            condition.evaluate_all(&ctx),
            [
                true, false, false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false
            ]
        );
    }

    #[test]
    fn test_hours() {
        let ctx = setup();

        assert!(Condition::Hours(0, 2).evaluate(&ctx));
        assert!(!Condition::Hours(3, 4).evaluate(&ctx));
        assert!(Condition::Hours(1, 3).evaluate(&ctx));

        assert_eq!(
            Condition::Hours(1, 3).evaluate_all(&ctx),
            [
                false, true, true, true, false, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false, false, false
            ]
        );
    }

    #[test]
    fn test_percentile() {
        let ctx = setup();

        let result = Condition::Percentile {
            value: 0.09,
            range: Range::Today, // calculated percentil is 0.08695652
        }
        .evaluate(&ctx);
        assert_eq!(result, true);

        let result = Condition::Percentile {
            value: 0.08,
            range: Range::Today, // calculated percentil is 0.08695652
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

    mod cheap_tests {
        use chrono::NaiveDateTime;

        use crate::web_server::conditions::{
            condition_tests::setup, Condition, Eval, EvaluateContext,
        };

        #[test]
        fn test_cheap_today() {
            // Actual hours is 2:00 - 2:59
            let ctx = setup();

            // Single price, always true
            assert_eq!(
                Condition::Cheap {
                    hours: 1,
                    from: 2,
                    to: 3,
                }
                .evaluate(&ctx),
                true
            );

            assert_eq!(
                Condition::Cheap {
                    hours: 1,
                    from: 2,
                    to: 3,
                }
                .evaluate(&ctx),
                true
            );

            // Out of range
            assert_eq!(
                Condition::Cheap {
                    hours: 24,
                    from: 3,
                    to: 24,
                }
                .evaluate(&ctx),
                false
            );

            // Real usage
            assert_eq!(
                Condition::Cheap {
                    hours: 3,
                    from: 0,
                    to: 3,
                }
                .evaluate(&ctx),
                true
            );
            assert_eq!(
                Condition::Cheap {
                    hours: 2,
                    from: 0,
                    to: 3,
                }
                .evaluate(&ctx),
                false
            );
        }

        #[test]
        fn test_cheap_yesterday_today() {
            let mut ctx = EvaluateContext::new(
                NaiveDateTime::parse_from_str("2025-02-16 09:43:44", "%Y-%m-%d %H:%M:%S").unwrap(),
                vec![
                    // yesterday 0-12
                    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                    // yesterday 12-24
                    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                    // today 0-12
                    9.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                    // today 12-24
                    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 1.0,
                ],
                24,
            );

            assert_eq!(
                Condition::Cheap {
                    hours: 1,
                    from: 23,
                    to: 1,
                }
                .evaluate(&ctx),
                true
            );

            ctx.prices.prices[23] = 8.0;

            assert_eq!(
                Condition::Cheap {
                    hours: 1,
                    from: 23,
                    to: 1,
                }
                .evaluate(&ctx),
                false
            );
        }

        #[test]
        fn test_cheap_today_tomorrow() {
            let mut ctx = EvaluateContext::new(
                NaiveDateTime::parse_from_str("2025-02-16 09:43:44", "%Y-%m-%d %H:%M:%S").unwrap(),
                vec![
                    // today 0-12
                    1.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                    // today 12-24
                    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 9.0,
                    // tomorrow 0-12
                    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                    // tomorrow 12-24
                    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                ],
                23,
            );

            assert_eq!(
                Condition::Cheap {
                    hours: 1,
                    from: 23,
                    to: 1,
                }
                .evaluate(&ctx),
                true
            );

            ctx.prices.prices[24] = 8.0;

            assert_eq!(
                Condition::Cheap {
                    hours: 1,
                    from: 23,
                    to: 1,
                }
                .evaluate(&ctx),
                false
            );
        }
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
    #[serde(rename = "fromto")]
    FromTo(u32, u32),
}

#[derive(Serialize, Debug)]
pub struct EvaluateContext {
    pub now: NaiveDateTime,
    pub prices: PricesContext,
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

    fn actual_price(&self) -> f32 {
        self.prices.prices[self.prices.now_index]
    }

    fn slice(&self, from: usize, to: usize) -> Option<Vec<f32>> {
        let range = find_time_range(self.prices.now_index, from as u8, to as u8)?;

        if range.1 > self.prices.prices.len() {
            return None;
        }

        Some(self.prices.prices[range.0..range.1].to_vec())
    }

    fn limit(&self, range: Range) -> Option<PricesContext> {
        match range {
            Range::Today => {
                let start = (self.prices.now_index / 24) * 24;
                let prices = self.prices.prices[start..(start + 24)].to_vec();

                Some(PricesContext {
                    now_index: self.prices.now_index - start,
                    prices,
                })
            }
            Range::Future => Some(PricesContext {
                now_index: 0,
                prices: self.prices.prices[self.prices.now_index..].to_vec(),
            }),
            Range::PlusMinusHours(hours) => {
                let start = self.prices.now_index.saturating_sub(hours as usize);
                let end = self
                    .prices
                    .now_index
                    .saturating_add(1)
                    .saturating_add(hours as usize);
                let prices = self.prices.prices[start..end].to_vec();

                Some(PricesContext {
                    now_index: hours as usize,
                    prices,
                })
            }
            Range::FromTo(from, to) => {
                assert!(from < to, "From must be lower than to");

                let start_of_day = (self.prices.now_index / 24) * 24;
                let start_of_range = start_of_day.saturating_add(from as usize);

                if self.prices.now_index < start_of_range {
                    return None;
                }

                let now_index = self.prices.now_index - start_of_range;
                let end_of_range = start_of_day.saturating_add(to as usize);

                if now_index >= end_of_range - start_of_range {
                    return None;
                }

                let prices = self.prices.prices[start_of_range..end_of_range].to_vec();

                Some(PricesContext { now_index, prices })
            }
        }
    }
}

/// Given:
/// - `current_hour_idx`: the current hour in "absolute" indexing (e.g., 0..72 for 3 days),
/// - `from_hour` (inclusive, 0..23),
/// - `to_hour`   (exclusive, 0..23),
///
/// this function determines whether `current_hour_idx` lies in the interval
/// [start_idx..end_idx) derived from the given `from_hour..to_hour`.
///
/// If `current_hour_idx` is within that interval, returns `Some((start_idx, end_idx))`.
/// Otherwise, returns `None`.
///
/// The interval may cross midnight (e.g., 22..4) in which case the "to" day
/// is `day_offset_from + 1`.
fn find_time_range(
    current_hour_idx: usize,
    from_hour: u8, // inclusive
    to_hour: u8,   // exclusive
) -> Option<(usize, usize)> {
    // Determine the current day and hour.
    let current_day = current_hour_idx / 24;
    let current_hour = current_hour_idx % 24;

    // Decide if `from_hour` belongs to "today" or "yesterday".
    // If from_hour > current_hour, we shift to the previous day.
    let mut from_day_offset = current_day as isize;
    if from_hour as usize > current_hour {
        from_day_offset -= 1;
    }

    // Check if the range crosses midnight (e.g. 22..4).
    // If from_hour > to_hour, it must cross midnight.
    let crosses_midnight = from_hour > to_hour;
    let mut to_day_offset = from_day_offset;
    if crosses_midnight {
        to_day_offset += 1;
    }

    // Calculate the absolute start and end indices, with `end_idx` being exclusive.
    let start_isize = from_day_offset * 24 + from_hour as isize;
    let end_isize = to_day_offset * 24 + to_hour as isize; // exclusive

    // If they are negative, we can't convert to usize, so return None.
    if start_isize < 0 || end_isize < 0 {
        return None;
    }
    let start_idx = start_isize as usize;
    let end_idx = end_isize as usize;

    // Check if current_hour_idx lies in [start_idx, end_idx).
    if start_idx <= current_hour_idx && current_hour_idx < end_idx {
        Some((start_idx, end_idx))
    } else {
        None
    }
}

#[cfg(test)]
mod find_time_range_tests {
    use crate::web_server::conditions::find_time_range;

    #[test]
    fn test_find_time_range() {
        assert_eq!(find_time_range(0, 0, 0), None);
        assert_eq!(find_time_range(0, 0, 8), Some((0, 8)));
        assert_eq!(find_time_range(0, 0, 24), Some((0, 24)));
        assert_eq!(find_time_range(26, 0, 24), Some((24, 48)));
        assert_eq!(find_time_range(0, 1, 8), None);

        assert_eq!(find_time_range(23, 23, 24), Some((23, 24)));
        assert_eq!(find_time_range(24, 23, 24), None);
        assert_eq!(find_time_range(47, 23, 24), Some((47, 48)));

        assert_eq!(find_time_range(23, 23, 1), Some((23, 25)));
        assert_eq!(find_time_range(24, 23, 1), Some((23, 25)));
        assert_eq!(find_time_range(47, 23, 1), Some((47, 49)));

        assert_eq!(find_time_range(0, 0, 24), Some((0, 24)));
        assert_eq!(find_time_range(24, 0, 24), Some((24, 48)));
    }
}

#[cfg(test)]
mod evaluate_context_tests {
    use super::*;

    fn setup() -> EvaluateContext {
        EvaluateContext::new(
            NaiveDateTime::parse_from_str("2020-01-01 02:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            (0..48).map(|i| i as f32).collect(),
            26, // 24 = 0-0:59, 25 = 1-1:59, 26 = 2-2:59
        )
    }

    #[test]
    fn test_actual_price() {
        let ctx = setup();
        assert_eq!(ctx.actual_price(), 26.0);
    }

    #[test]
    fn test_slice() {
        let ctx = setup();

        assert_eq!(
            ctx.slice(0, 24),
            Some(vec![
                // Actual day, 0 - 12h
                24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0,
                // 13-24h
                36.0, 37.0, 38.0, 39.0, 40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0
            ])
        );

        // Out of range
        assert_eq!(ctx.slice(0, 2), None);
        assert_eq!(ctx.slice(3, 24), None);

        // Actual hour
        assert_eq!(ctx.slice(2, 3), Some(vec![26.0]));

        // Over midnight, tests other day but same results
        let mut ctx = setup();
        ctx.prices.now_index = 23;
        assert_eq!(ctx.slice(22, 2), Some(vec![22.0, 23.0, 24.0, 25.0]));

        let mut ctx = setup();
        ctx.prices.now_index = 24;
        assert_eq!(ctx.slice(22, 2), Some(vec![22.0, 23.0, 24.0, 25.0]));

        let mut ctx = setup();
        ctx.prices.now_index = 0;
        assert_eq!(ctx.slice(22, 2), None);

        let mut ctx = setup();
        ctx.prices.now_index = 47;
        assert_eq!(ctx.slice(22, 2), None);
    }

    #[test]
    fn test_limit_today() {
        let ctx = setup();
        let result = ctx.limit(Range::Today).unwrap();

        assert_eq!(result.now_index, 2); // 3. hour
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
    fn test_limit_future() {
        let ctx = setup();
        let result = ctx.limit(Range::Future).unwrap();

        assert_eq!(result.now_index, 0);
        assert_eq!(
            result.prices,
            vec![
                26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0, 37.0, 38.0, 39.0,
                40.0, 41.0, 42.0, 43.0, 44.0, 45.0, 46.0, 47.0
            ]
        );
    }

    #[test]
    fn test_limit_plus_minus_hours() {
        let ctx = setup();

        let result = ctx.limit(Range::PlusMinusHours(1)).unwrap();
        assert_eq!(result.now_index, 1);
        assert_eq!(result.prices, vec![25.0, 26.0, 27.0]);

        let result = ctx.limit(Range::PlusMinusHours(3)).unwrap();
        assert_eq!(result.now_index, 3);
        assert_eq!(
            result.prices,
            vec![23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0]
        );
    }

    #[test]
    fn test_limit_from_to() {
        let ctx = setup();

        // Out of range
        let result = ctx.limit(Range::FromTo(0, 2));
        assert!(result.is_none());
        let result = ctx.limit(Range::FromTo(3, 4));
        assert!(result.is_none());

        let result = ctx.limit(Range::FromTo(2, 3)).unwrap();
        assert_eq!(result.now_index, 0);
        assert_eq!(result.prices, vec![26.0]);

        let result = ctx.limit(Range::FromTo(2, 4)).unwrap();
        assert_eq!(result.now_index, 0);
        assert_eq!(result.prices, vec![26.0, 27.0]);

        let result = ctx.limit(Range::FromTo(0, 5)).unwrap();
        assert_eq!(result.now_index, 2);
        assert_eq!(result.prices, vec![24.0, 25.0, 26.0, 27.0, 28.0]);
    }
}

#[derive(Serialize, Debug)]
pub struct PricesContext {
    pub prices: Vec<f32>,
    pub now_index: usize,
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
