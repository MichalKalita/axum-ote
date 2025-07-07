use maud::{html, Markup};

use crate::web_server::state::{Distribution, PriceStats};

use super::conditions::{CheapCondition, Condition, Eval, EvaluateContext};

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

struct ChartMetrics {
    scale: f32,
    zero_offset: f32,
    svg_width: usize,
    svg_height: f32,
}

impl ChartSettings {
    fn calculate_metrics(&self, prices: &[f32]) -> ChartMetrics {
        let cheapiest_hour = PriceStats::cheapest_hour(&prices);
        let expensive_hour = PriceStats::expensive_hour(&prices);

        let scale = if *cheapiest_hour.1 < 0.0 {
            self.height / (expensive_hour.1 - cheapiest_hour.1)
        } else {
            self.height / expensive_hour.1
        };

        let zero_offset = if *cheapiest_hour.1 < 0.0 {
            15.0 + (expensive_hour.1 * scale)
        } else {
            self.height + 15.0
        };

        ChartMetrics {
            scale,
            zero_offset,
            svg_width: prices.len() * (self.bar_width + self.bar_spacing),
            svg_height: self.height + 30.0,
        }
    }

    fn calculate_bar_x(&self, hour: usize) -> usize {
        hour * (self.bar_width + self.bar_spacing)
    }

    fn calculate_bar_y(&self, price: f32, metrics: &ChartMetrics) -> f32 {
        if price >= 0.0 {
            metrics.zero_offset - (price * metrics.scale)
        } else {
            metrics.zero_offset
        }
    }

    fn calculate_bar_height(&self, price: f32, metrics: &ChartMetrics) -> f32 {
        1.0_f32.max(price.abs() * metrics.scale)
    }

    fn calculate_text_x(&self, hour: usize) -> usize {
        hour * (self.bar_width + self.bar_spacing) + self.bar_width / 2
    }

    fn calculate_price_text_y(&self, price: f32, metrics: &ChartMetrics) -> f32 {
        metrics.zero_offset - (price * metrics.scale) - 3.0
    }

    fn calculate_label_text_y(&self, metrics: &ChartMetrics) -> f32 {
        metrics.zero_offset - 10.0
    }

    pub fn render(
        &self,
        prices: &[f32],
        labels: Option<&[&str]>,
        color: impl for<'a> Fn(&'a (usize, f32)) -> &'a str,
    ) -> Markup {
        let metrics = self.calculate_metrics(prices);

        html! {
            svg width=(metrics.svg_width) height=(metrics.svg_height) {
                g {
                    @for (hour, &price) in prices.iter().enumerate() {
                        rect x=(self.calculate_bar_x(hour)) y=(self.calculate_bar_y(price, &metrics))
                            width=(self.bar_width) height=(self.calculate_bar_height(price, &metrics))
                            class=(color(&(hour, price))) {}
                        text x=(self.calculate_text_x(hour)) y=(self.calculate_price_text_y(price, &metrics)) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-300" {
                            (format!("{price:.0}"))
                        }

                        @if let Some(labels) = labels {
                            text x=(self.calculate_text_x(hour)) y=(self.calculate_label_text_y(&metrics)) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-100" {
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
                "fill-green-600"
            } else {
                "fill-red-600"
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

pub fn link(url: &str, text: &str) -> Markup {
    html! {
        a href=(url) a .underline ."hover:text-red-400" { (text) }
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
            Condition::Cheap(CheapCondition { hours, from, to }) => html! {
                div .ml-4 {
                    "Cheap: " (hours) " cheapiest hours in hours " (from) " - " (to)
                }
            },

            #[cfg(test)]
            Condition::Debug(_) => todo!(),
        }
    }
}

impl RenderHtml for Option<CheapCondition> {
    fn render_html(&self) -> Markup {
        let actual = self.as_ref().unwrap_or_else(|| &CheapCondition {
            hours: 1,
            from: 0,
            to: 24,
        });

        html! {
            form method="GET" class="flex space-x-2 items-center" {
                label for="cheap_hours" { "Cheap Hours:" }
                input type="number" id="cheap_hours" name="hours" value=(actual.hours) min="1" max="24" step="1" class="w-16 p-1 border rounded" {}
                label for="cheap_from" { "From:" }
                input type="number" id="cheap_from" name="from" value=(actual.from) min="0" max="23" step="1" class="w-16 p-1 border rounded" {}
                label for="cheap_to" { "To:" }
                input type="number" id="cheap_to" name="to" value=(actual.to) min="1" max="24" step="1" class="w-16 p-1 border rounded" {}
                button type="submit" class="px-4 py-1 bg-blue-500 text-white rounded cursor-pointer" { "Update" }
            }
        }
    }
}

#[cfg(test)]
mod chart_settings_tests {
    use super::*;

    #[test]
    fn test_chart_settings_with_prices_negative_zero_positive() {
        let settings = ChartSettings::default();
        let prices = vec![-10.0, 0.0, 10.0];
        let metrics = settings.calculate_metrics(&prices);

        // Verify scale calculation
        assert_eq!(metrics.scale, 15.0); // 300.0 / 20.0

        // Verify zero offset
        assert_eq!(metrics.zero_offset, 165.0); // 15.0 + (10.0 * 15.0)

        // Verify bar heights
        assert_eq!(settings.calculate_bar_height(-10.0, &metrics), 150.0); // same height as +10.0
        assert_eq!(settings.calculate_bar_height(0.0, &metrics), 1.0); // min height for zero
        assert_eq!(settings.calculate_bar_height(10.0, &metrics), 150.0); // 10.0 * 15.0

        // Verify bar Y positions
        assert_eq!(settings.calculate_bar_y(-10.0, &metrics), 165.0); // at zero line for negative
        assert_eq!(settings.calculate_bar_y(0.0, &metrics), 165.0); // at zero line for zero
        assert_eq!(settings.calculate_bar_y(10.0, &metrics), 15.0); // above zero line for positive
    }
}
