use maud::{html, Markup};

use crate::web_server::state::{Distribution, PriceStats};

use super::conditions::Condition;

pub fn render_layout(content: Markup) -> Markup {
    html! {
        html {
            head {
                title { "OTE CR Price Checker" }
                script src="https://cdn.tailwindcss.com" {}
                script src="https://unpkg.com/htmx.org@2.0.4" integrity="sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+" crossorigin="anonymous" {}
                script defer src="https://cdn.jsdelivr.net/npm/alpinejs@3.x.x/dist/cdn.min.js" {}
            }
            body .p-4.text-center."dark:bg-gray-900"."dark:text-gray-300" {
                (content)
            }
        }
    }
}

impl crate::web_server::state::DayPrices {
    pub(crate) fn render_graph(&self, dist: &Distribution, active_hour: usize) -> Markup {
        let prices = self.total_prices(&dist);
        let cheapiest_hour = prices.cheapest_hour();
        let expensive_hour = prices.expensive_hour();

        const GRAPH_HEIGHT: f32 = 300.0;
        const BAR_WIDTH: usize = 24;
        const BAR_SPACING: usize = 1;
        let scale = if cheapiest_hour.1 < 0.0 {
            GRAPH_HEIGHT / (expensive_hour.1 - cheapiest_hour.1)
        } else {
            GRAPH_HEIGHT / expensive_hour.1
        };
        let zero_offset = (if cheapiest_hour.1 < 0.0 {
            GRAPH_HEIGHT - (cheapiest_hour.1 * scale)
        } else {
            GRAPH_HEIGHT
        }) + 15.0;

        let dist_high_hours = dist.by_hours();
        let active_hour_index = active_hour;

        html! {
            svg width=(24 * (BAR_WIDTH + BAR_SPACING)) height=(GRAPH_HEIGHT + 30.0) {
                g {
                    @for (hour, &price) in prices.iter().enumerate() {
                        rect x=(hour * (BAR_WIDTH + BAR_SPACING)) y=(zero_offset - (price * scale))
                            width=(BAR_WIDTH) height=(1.0_f32.max(price * scale))
                            .fill-blue-500[active_hour_index != hour] .fill-green-500[active_hour_index == hour] {}
                        text x=(hour * (BAR_WIDTH + BAR_SPACING) + BAR_WIDTH / 2) y=(zero_offset - (price * scale) - 3.0) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-300" {
                            (format!("{price:.0}"))
                        }

                        text x=(hour * (BAR_WIDTH + BAR_SPACING) + BAR_WIDTH / 2) y=(zero_offset - 10.0) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-100" {
                            (if dist_high_hours[hour] { "V" } else { "N" })
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn render_table(&self, dist: &Distribution) -> Markup {
        let total_prices = self.total_prices(dist);

        // Find low and high price in total prices
        let (total_low, total_high) = total_prices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), &price| {
                (low.min(price), high.max(price))
            });

        html! {
            table {
                tr {
                    th.pr-10 { "Hour" }
                    th.pr-10 { "Market" }
                    th { "Total EUR/MWh" }
                }
                @for (hour, &price) in self.prices.iter().enumerate() {
                    tr
                        ."bg-green-100"[total_prices[hour] == total_low]
                        ."dark:bg-green-900"[total_prices[hour] == total_low]
                        .bg-red-100[total_prices[hour] == total_high]
                        ."dark:bg-red-900"[total_prices[hour] == total_high]
                    {

                        td .text-center .font-mono .pr-10 {
                            (hour)" - "(hour+1)
                        }
                        td .text-right .text-green-700[price<0.0] .font-mono .pr-10 {
                            (format!("{:2.2}", price))
                        }
                        td .text-right .text-green-700[price<0.0] .font-mono {
                            (format!("{:2.2}", total_prices[hour]))
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
            Condition::Percentile { value: _, range: _ } => todo!(),

            #[cfg(test)]
            Condition::Debug(_) => todo!(),
        }
    }
}
