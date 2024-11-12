use maud::{html, Markup};

impl crate::web_server::state::DayPrices {
    pub(crate) fn render_graph(&self) -> Markup {
        let cheapiest_hour = self.cheapest_hour();
        let expensive_hour = self.expensive_hour();

        const GRAPH_HEIGHT: f32 = 300.0;
        const BAR_WIDTH: usize = 24;
        const BAR_SPACING: usize = 1;
        let scale = if *cheapiest_hour.1 < 0.0 {
            GRAPH_HEIGHT / (expensive_hour.1 - cheapiest_hour.1)
        } else {
            GRAPH_HEIGHT / expensive_hour.1
        };
        let zero_offset = (if *cheapiest_hour.1 < 0.0 {
            GRAPH_HEIGHT - (cheapiest_hour.1 * scale)
        } else {
            GRAPH_HEIGHT
        }) + 15.0;

        html! {
            svg width=(24 * (BAR_WIDTH + BAR_SPACING)) height=(GRAPH_HEIGHT + 30.0) {
                g {
                    @for (hour, &price) in self.prices.iter().enumerate() {
                        rect x=(hour * (BAR_WIDTH + BAR_SPACING)) y=(zero_offset - (price * scale)) width=(BAR_WIDTH) height=(1.0_f32.max(price * scale)) .fill-blue-500 {}
                        text x=(hour * (BAR_WIDTH + BAR_SPACING) + BAR_WIDTH / 2) y=(zero_offset - (price * scale) - 3.0) text-anchor="middle" .font-mono.text-xs."dark:fill-gray-300" {
                            (format!("{price:.0}"))
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn render_table(&self) -> Markup {
        let cheapiest_hour = self.cheapest_hour().0;
        let expensive_hour = self.expensive_hour().0;

        html! {
            table {
                tr {
                    th { "Hour" }
                    th { "Price EUR/MWh" }
                }
                @for (hour, &price) in self.prices.iter().enumerate() {
                    tr
                        ."bg-green-100"[hour == cheapiest_hour]
                        ."dark:bg-green-900"[hour == cheapiest_hour]
                        .bg-red-100[hour == expensive_hour]
                        ."dark:bg-red-900"[hour == expensive_hour]
                    {

                        td .text-center .font-mono {
                            (hour)" - "(hour+1)
                        }
                        td .text-right .text-green-700[price<0.0] .font-mono {
                            (format!("{:2.2}", price))
                        }
                    }
                }
            }
        }
    }
}
