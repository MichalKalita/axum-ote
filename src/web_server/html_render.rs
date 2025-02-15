use maud::{html, Markup};

use crate::web_server::{
    conditions::Range,
    state::{Distribution, PriceStats},
};

use super::conditions::{Condition, Eval, EvaluateContext};

pub fn render_layout(content: Markup) -> Markup {
    html! {
        html {
            head {
                title { "OTE CR Price Checker" }
                script src="https://unpkg.com/@tailwindcss/browser@4" {}
                script src="https://unpkg.com/htmx.org@2.0.4" integrity="sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+" crossorigin="anonymous" {}
                script defer src="https://cdn.jsdelivr.net/npm/alpinejs@3.x.x/dist/cdn.min.js" {}
            }
            body .p-4.text-center."dark:bg-gray-900"."dark:text-gray-300" {
                (content)
            }
        }
    }
}

pub struct ChartSettings {
    pub height: f32,
    pub bar_width: usize,
    pub bar_spacing: usize,
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            height: 300.0,
            bar_width: 24,
            bar_spacing: 1,
        }
    }
}

impl ChartSettings {
    pub fn render(
        &self,
        prices: &[f32],
        labels: Option<&[&str]>,
        colors: impl for<'a> Fn(&'a (usize, f32)) -> &'a str,
    ) -> Markup {
        let cheapiest_hour = PriceStats::cheapest_hour(&prices);
        let expensive_hour = PriceStats::expensive_hour(&prices);

        let scale = if *cheapiest_hour.1 < 0.0 {
            self.height / (expensive_hour.1 - cheapiest_hour.1)
        } else {
            self.height / expensive_hour.1
        };
        let zero_offset = (if *cheapiest_hour.1 < 0.0 {
            self.height - (cheapiest_hour.1 * scale)
        } else {
            self.height
        }) + 15.0;

        html! {
            svg width=(prices.len() * (self.bar_width + self.bar_spacing)) height=(self.height + 30.0) {
                g {
                    @for (hour, &price) in prices.iter().enumerate() {
                        rect x=(hour * (self.bar_width + self.bar_spacing)) y=(zero_offset - (price * scale))
                            width=(self.bar_width) height=(1.0_f32.max(price * scale))
                            fill=(colors(&(hour, price))) {}
                        text x=(hour * (self.bar_width + self.bar_spacing) + self.bar_width / 2) y=(zero_offset - (price * scale) - 3.0) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-300" {
                            (format!("{price:.0}"))
                        }

                        @if let Some(labels) = labels {
                            text x=(hour * (self.bar_width + self.bar_spacing) + self.bar_width / 2) y=(zero_offset - 10.0) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-100" {
                                (labels[hour])
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Condition {
    pub fn evaluate_all_in_chart(&self, ctx: &EvaluateContext) -> Markup {
        let results = self.evaluate_all(ctx);

        let labels = &results
            .iter()
            .map(|result| if *result { "T" } else { "F" })
            .collect::<Vec<&str>>();

        let chart = ChartSettings::default();
        chart.render(&ctx.prices.prices, Some(&labels), |(index, _price)| {
            if results[*index] {
                "var(--color-green-500)"
            } else {
                "var(--color-red-500)"
            }
        })
    }
}

fn format_price(price: f32) -> Markup {
    html! {
        (price.floor())
        span .text-neutral-500 .text-sm {
            "."(format!("{:02.0}", (price - price.floor()) * 100.0 ))
        }
    }
}

impl crate::web_server::state::DayPrices {
    pub(crate) fn render_table(&self, dist: &Distribution) -> Markup {
        let total_prices = self.total_prices(dist);

        let (_, &total_low) = PriceStats::cheapest_hour(&&(total_prices[..]));
        let (_, &total_high) = PriceStats::expensive_hour(&&(total_prices[..]));

        html! {
            table {
                tr {
                    th.pr-10 { "Hour" }
                    th colspan="2" { "Price EUR/MWh" }
                }
                tr {
                    th.pr-10 { "" }
                    th.pr-10 { "Market" }
                    th { "With Distribution" }
                }
                @for (hour, &price) in self.prices.iter().enumerate() {
                    tr
                        ."bg-green-100"[total_prices[hour] == total_low]
                        ."dark:bg-green-900"[total_prices[hour] == total_low]
                        .bg-red-100[total_prices[hour] == total_high]
                        ."dark:bg-red-900"[total_prices[hour] == total_high]
                    {

                        td .text-right .font-mono .pr-10 {
                            (hour)
                            span .text-neutral-500 .text-sm {
                                " : 00 - 59"
                            }
                        }
                        td .text-right .text-green-700[price<0.0] .font-mono .pr-10 {
                            (format_price(price))
                        }
                        td .text-right .text-green-700[price<0.0] .font-mono {
                            (format_price(total_prices[hour]))
                        }
                    }
                }
            }
        }
    }
}

pub trait RenderHtml {
    fn render_html(&self) -> Markup;
}

impl RenderHtml for Condition {
    fn render_html(&self) -> Markup {
        match self {
            Condition::And(conditions) => html! {
                div .ml-4 {
                    "AND"
                    ul {
                        @for condition in conditions {
                            li { (condition.render_html()) }
                        }
                    }
                }
            },
            Condition::Or(conditions) => html! {
                div .ml-4 {
                    "OR"
                    ul {
                        @for condition in conditions {
                            li { (condition.render_html()) }
                        }
                    }
                }
            },
            Condition::Not(condition) => html! {
                div .ml-4 {
                    "NOT"
                    (condition.render_html())
                }
            },
            Condition::Price(price) => html! {
                div .ml-4 {
                    "Price: " (price)
                }
            },
            Condition::Hours(from, to) => html! {
                div .ml-4 {
                    "Hours: " (from) " - " (to)
                }
            },
            Condition::Percentile { value, range } => html! {
                div .ml-4 {
                    "Percentile: " (value) " in range "
                    @match range {
                        Range::Today => { "today" }
                        Range::Future => { "future" }
                        Range::PlusMinusHours(hours) => { "+-" (hours) " hours" }
                        Range::FromTo(from, to) => { (from) " - " (to) }
                    }
                }
            },
            Condition::Cheap { hours, from, to } => html! {
                div .ml-4 {
                    "Cheap: " (hours) " cheapiest hours in hours " (from) " - " (to)
                }
            },

            #[cfg(test)]
            Condition::Debug(_) => todo!(),
        }
    }
}
