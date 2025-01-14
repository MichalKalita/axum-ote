use maud::{html, Markup};

use super::conditions::Condition;

trait CopyPush {
    fn copy_push(&self, position: u8) -> Vec<u8>;
}

impl CopyPush for Vec<u8> {
    fn copy_push(&self, position: u8) -> Vec<u8> {
        let mut new_vec = self.clone();
        new_vec.push(position);

        new_vec
    }
}

pub fn builder(condition: &Condition, position: Vec<u8>) -> Markup {
    match condition {
        Condition::And(vec) => {
            html! {
                div {
                    "And"
                    ol .list-decimal.list-inside {
                        @for (index, condition) in vec.iter().enumerate() {
                            li .pt-2 { (builder(condition, position.copy_push(index as u8))) }
                        }
                    }
                    (add_condition(position))
                }
            }
        }
        Condition::Or(vec) => {
            html! {
                div {
                    "Or"
                    ol .list-decimal.list-inside {
                        @for (index, condition) in vec.iter().enumerate() {
                            li .pt-2 { (builder(condition, position.copy_push(index as u8))) }
                        }
                    }
                    (add_condition(position))
                }
            }
        }
        Condition::Not(condition) => {
            html! {
                div .inline-block {
                    "Not"
                    (builder(condition, position.copy_push(1)))
                }
            }
        }
        Condition::PriceLowerThan(value) => {
            html! {
                div .inline-block {
                    "Price lower than"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(value);
                }
            }
        }
        Condition::Hours(from, to) => {
            html! {
                div .inline-block {
                    "Hours"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(from);
                    "to"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(to);
                }
            }
        }
        Condition::PercentileInRange { value, range } => todo!(),

        #[cfg(test)]
        Condition::Debug(_) => todo!(),
    }
}

fn add_condition(position: Vec<u8>) -> Markup {
    html! {
        div .pt-2 {
            "Add next condition"
            button .font-bold.px-2.text-sky-400.underline { "Or" }
            button .font-bold.px-2.text-sky-400.underline { "Price" }
            button .font-bold.px-2.text-sky-400.underline { "Hours" }
            button .font-bold.px-2.text-sky-400.underline { "Not" }
        }
    }
}
