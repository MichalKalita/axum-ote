use core::f32;
use std::str::FromStr;

use crate::data_loader::fetch_data;
use chrono::{Timelike, Utc};
use chrono_tz::Europe::Prague;
use dashmap::DashMap;
use serde::Serialize;
use tokio::join;

use super::conditions::EvaluateContext;

#[derive(Serialize, Clone)]
pub struct DayPrices {
    pub prices: Vec<f32>,
}

pub trait PriceStats {
    fn cheapest_hour(&self) -> (usize, &f32);
    fn expensive_hour(&self) -> (usize, &f32);
}

impl<'a> PriceStats for &'a [f32] {
    fn cheapest_hour(&self) -> (usize, &f32) {
        self.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap()
    }

    fn expensive_hour(&self) -> (usize, &f32) {
        self.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap()
    }
}

impl DayPrices {
    pub fn total_prices(&self, dist: &Distribution) -> Vec<f32> {
        let mut prices = self.prices.clone();
        for (i, price) in prices.iter_mut().enumerate() {
            if dist.high_hours.contains(&(i as u8 / 4)) {
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
    pub fn by_hours(&self) -> [&str; 24] {
        let mut distribution = ["N"; 24];
        for hour in self.high_hours.iter() {
            distribution[*hour as usize] = "V";
        }
        distribution
    }
}

pub struct AppState {
    pub days: DashMap<chrono::NaiveDate, DayPrices>,
    pub distribution: Distribution,
}

const NEXT_DAY_PRICES_HOUR: u32 = 13;

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
                    self.days.insert(*date, DayPrices { prices: prices.clone() });

                    return Some(DayPrices { prices });
                }
                Err(_) => {
                    return None;
                }
            }
        }

        self.days.get(date).map(|i| i.value().clone())
    }

    pub async fn expression_context(&self) -> Option<EvaluateContext> {
        let now = Utc::now().with_timezone(&Prague);
        let hour = now.time().hour();

        let today = now.date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let tomorrow = today + chrono::Duration::days(1);

        let join_yesterday = self.get_prices(&yesterday);
        let join_today = self.get_prices(&today);
        let join_tomorrow = self.get_prices(&tomorrow);

        let prices: (Option<DayPrices>, Option<DayPrices>, Option<DayPrices>) =
            if hour >= NEXT_DAY_PRICES_HOUR {
                join!(join_yesterday, join_today, join_tomorrow)
            } else {
                let (yesterday, today) = join!(join_yesterday, join_today);

                (yesterday, today, None)
            };

        match prices {
            (yesterday, Some(today), tomorrow) => {
                let mut prices: Vec<f32> = Vec::new();
                let mut offset = 0;

                if let Some(yesterday) = yesterday {
                    prices.extend_from_slice(&yesterday.prices);
                    offset = 24;
                }

                prices.extend_from_slice(&today.prices);

                if let Some(tomorrow) = tomorrow {
                    prices.extend_from_slice(&tomorrow.prices);
                }

                Some(EvaluateContext::new(
                    now.naive_local(),
                    prices,
                    (hour + offset) as usize,
                ))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Currency {
    Eur,
    Czk,
}

impl Currency {
    pub const RATE: f32 = 24.30;

    pub fn convert(self, price: f32) -> f32 {
        match self {
            Currency::Eur => price,
            Currency::Czk => price * Self::RATE / 1000.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Currency::Eur => "Price EUR/MWh",
            Currency::Czk => "Price CZK/kWh",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Currency::Eur => "EUR/MWh",
            Currency::Czk => "CZK/kWh",
        }
    }
}

impl FromStr for Currency {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "eur" => Ok(Currency::Eur),
            "czk" => Ok(Currency::Czk),
            _ => Err(format!("unknown currency: {s}")),
        }
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Currency::Eur => write!(f, "eur"),
            Currency::Czk => write!(f, "czk"),
        }
    }
}