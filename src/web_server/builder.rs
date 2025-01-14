use maud::{html, Markup};

use super::conditions::Condition;

#[derive(Clone)]
struct Position(Vec<u8>);

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let position_str = self
            .0
            .iter()
            .map(|p| format!("[{}]", p))
            .collect::<Vec<String>>()
            .join("");
        write!(f, "{}", position_str)
    }
}

impl Position {
    fn new() -> Self {
        Position(vec![])
    }
    fn extend(&self, position: u8) -> Position {
        let mut new_vec = self.clone();
        new_vec.0.push(position);

        new_vec
    }
}

pub fn builder(condition: &Condition) -> Markup {
    html! {
        form #builder method="post" hx-post="" hx-target="body" {
            (inside_builder(condition, Position::new()))
        }
    }
}

fn inside_builder(condition: &Condition, position: Position) -> Markup {
    match condition {
        Condition::And(vec) => {
            html! {
                div .border-l .pl-2 {
                    div .font-bold .text-xl { "And" }
                    div { "All this conditions must match together" }
                    ol .list-decimal.list-inside {
                        @for (index, condition) in vec.iter().enumerate() {
                            li .pt-2 { (inside_builder(condition, position.extend(index as u8))) }
                        }
                    }
                    (add_condition(position))
                }
            }
        }
        Condition::Or(vec) => {
            html! {
                div .border-l .pl-2 {
                    "Or"
                    ol .list-decimal.list-inside {
                        @for (index, condition) in vec.iter().enumerate() {
                            li .pt-2 { (inside_builder(condition, position.extend(index as u8))) }
                        }
                    }
                    (add_condition(position))
                }
            }
        }
        Condition::Not(condition) => {
            html! {
                div .inline-block .border-l .pl-2 {
                    "Not"
                    (inside_builder(condition, position.extend(0)))
                }
            }
        }
        Condition::PriceLowerThan(value) => {
            html! {
                div .inline-block {
                    "Price lower than"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(value) name={"exp" (position) "[price]"};
                }
            }
        }
        Condition::Hours(from, to) => {
            html! {
                div .inline-block {
                    "Hours"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(from) name={"exp" (position) "[hours][from]"};
                    "to"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(to) name={"exp" (position) "[hours][to]"};
                }
            }
        }
        Condition::PercentileInRange { value, range } => todo!(),

        #[cfg(test)]
        Condition::Debug(_) => todo!(),
    }
}

fn add_condition(position: Position) -> Markup {
    html! {
        div .pt-2 {
            "+"

            select .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" name={"exp" (position) "[extend]"} hx-post hx-trigger="change" {
                option { "-- Add condition --" }
                option value="or" { "Or" }
                option value="price" { "Price" }
                option value="hours" { "Hours" }
                option value="not" { "Not" }
            }
        }
    }
}
