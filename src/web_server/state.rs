use core::f32;

use crate::data_loader::fetch_data;
use chrono::Timelike;
use dashmap::DashMap;
use serde::Serialize;

use super::conditions::EvaluateContext;

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

    pub async fn expression_context(&self) -> Option<super::conditions::EvaluateContext> {
        let now = chrono::Local::now();
        let date = now.date_naive();
        let hour = now.time().hour();
        let prices = self.get_prices(&date).await?.prices;

        Some(EvaluateContext::new(
            now.naive_local(),
            prices.to_vec(),
            hour.try_into().unwrap(),
        ))
    }
}
