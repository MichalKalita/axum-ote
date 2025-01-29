use maud::{html, Markup};

use super::conditions::Condition;

#[derive(Clone, Debug, PartialEq)]
pub struct Position(Vec<u8>);

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let position_str = self
            .0
            .iter()
            .map(|p| format!("{}", p))
            .collect::<Vec<String>>()
            .join(".");
        write!(f, "{}", position_str)
    }
}

impl Position {
    pub fn new() -> Self {
        Position(vec![])
    }
    pub fn from(input: &Vec<u8>) -> Self {
        Position(input.clone())
    }
    pub fn extend(&self, position: u8) -> Position {
        let mut new_vec = self.clone();
        new_vec.0.push(position);

        new_vec
    }
    pub fn increment(&mut self) {
        if let Some(last) = self.0.last_mut() {
            *last += 1;
        }
    }
    fn element_id(&self) -> String {
        let id = self
            .0
            .iter()
            .map(|p| format!("{}", p))
            .collect::<Vec<String>>()
            .join("-");

        format!("form-part{}", id)
    }
}

#[cfg(test)]
mod position_tests {
    use super::Position;

    #[test]
    fn test_new() {
        let pos = Position::new();
        let expect: Vec<u8> = vec![];
        assert_eq!(pos.0, expect);
    }

    #[test]
    fn test_from() {
        let input = vec![1, 2, 3];
        let pos = Position::from(&input);
        assert_eq!(pos.0, input);
    }

    #[test]
    fn test_extend() {
        let pos = Position::from(&vec![1, 2]);
        let new_pos = pos.extend(3);
        assert_eq!(new_pos.0, vec![1, 2, 3]);
    }

    #[test]
    fn test_increment() {
        let mut pos = Position::from(&vec![1, 2, 3]);
        pos.increment();
        assert_eq!(pos.0, vec![1, 2, 4]);
    }

    #[test]
    fn test_element_id() {
        let pos = Position::from(&vec![1, 2, 3]);
        assert_eq!(pos.element_id(), "form-part1-2-3");
    }
}

pub fn builder(condition: &Condition) -> Markup {
    html! {
        // form #builder method="post" hx-post="" hx-target="body" {
            (inside_builder(condition, Position::new()))
        // }
    }
}

fn inside_builder(condition: &Condition, position: Position) -> Markup {
    match condition {
        Condition::And(vec) => {
            html! {
                div #{(position.element_id())} .border-l .pl-2 {
                    div .font-bold .text-xl { "And" }
                    div { "All this conditions must match together" }
                    ol .list-decimal.pl-6 {
                        @for (index, condition) in vec.iter().enumerate() {
                            (list_item(inside_builder(condition, position.extend(index as u8))))
                        }
                    }
                    (add_condition(position))
                }
            }
        }
        Condition::Or(vec) => {
            html! {
                div #{(position.element_id())} .border-l .pl-2 {
                    "Or"
                    ol .list-decimal.pl-6 {
                        @for (index, condition) in vec.iter().enumerate() {
                            (list_item(inside_builder(condition, position.extend(index as u8))))
                        }
                    }
                    (add_condition(position))
                }
            }
        }
        Condition::Not(condition) => {
            html! {
                div #{(position.element_id())} .inline-block .border-l .pl-2 {
                    "Not"
                    (inside_builder(condition, position.extend(0)))
                }
            }
        }
        Condition::PriceLowerThan(value) => {
            html! {
                form.m-0 hx-post {
                    input type="hidden" name="id" value=(position);

                    "Price lower than"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(value) name="price";
                    input type="submit" value="Save";
                }
            }
        }
        Condition::Hours(from, to) => {
            html! {
                div .inline-block {
                    "Hours"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(from) name="hours-from";
                    "to"
                    input .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" .text-right .w-20 type="number" value=(to) name="hours-to";
                }
            }
        }
        Condition::PercentileInRange { value: _, range: _ } => todo!(),

        #[cfg(test)]
        Condition::Debug(_) => todo!(),
    }
}

fn list_item(content: Markup) -> Markup {
    html! {
        li .pt-2.pl-2 { (content) }
    }
}

pub fn additional_condition(condition: &Condition, position: Position) -> Markup {
    list_item(inside_builder(condition, position))
}

fn add_condition(position: Position) -> Markup {
    html! {
        form hx-post hx-trigger="change" hx-target={"#"(position.element_id()) " ol"} hx-swap="beforeend" "hx-on::after-request"="this.reset()" .pt-2 {
            "+"

            input type="hidden" name="id" value=(position);
            select .bg-gray-800 .border .border-gray-700 .mx-1 ."p-0.5" name="extend" {
                option { "-- Add condition --" }
                option value="or" { "Or" }
                option value="price" { "Price" }
                option value="hours" { "Hours" }
                option value="not" { "Not" }
            }
        }
    }
}
