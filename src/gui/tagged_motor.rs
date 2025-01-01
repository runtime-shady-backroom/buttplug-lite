// Copyright 2022-2025 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::cmp::Ordering;

use iced::widget::{Button, Row, Text, TextInput};
use iced::{Alignment, Element, Length};

use crate::config::v3::MotorConfigurationV3;
use crate::gui::constants::*;
use crate::gui::element_appearance::ElementAppearance;
use crate::gui::structs::MotorMessage;
use crate::gui::util;

/// an optionally tagged motor
#[derive(Clone, Debug)]
pub struct TaggedMotor {
    pub motor: MotorConfigurationV3,
    pub state: TaggedMotorState,
}

impl PartialEq for TaggedMotor {
    fn eq(&self, other: &Self) -> bool {
        (&self.motor, &self.tag()) == (&other.motor, &other.tag())
    }
}

impl Eq for TaggedMotor {}

impl PartialOrd for TaggedMotor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaggedMotor {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.motor, &self.tag()).cmp(&(&other.motor, &other.tag()))
    }
}

impl TaggedMotor {
    pub fn new(motor: MotorConfigurationV3, tag: Option<String>) -> Self {
        let state = match tag {
            Some(tag) => TaggedMotorState::Tagged {
                tag,
                valid: true,
            },
            None => TaggedMotorState::Untagged,
        };

        TaggedMotor {
            motor,
            state,
        }
    }

    pub fn tag(&self) -> Option<&str> {
        match &self.state {
            TaggedMotorState::Tagged { tag, .. } => Some(tag),
            TaggedMotorState::Untagged => None
        }
    }

    pub fn update(&mut self, message: MotorMessage) {
        match message {
            MotorMessage::TagUpdated { tag, valid } => {
                if tag.is_empty() {
                    self.state = TaggedMotorState::Untagged;
                } else {
                    self.state = TaggedMotorState::Tagged { tag, valid };
                }
            }
            MotorMessage::TagDeleted => {
                self.state = TaggedMotorState::Untagged;
            }
        }
    }

    pub fn view(&self) -> Element<MotorMessage> {
        let row = Row::new()
            .spacing(EOL_INPUT_SPACING)
            .align_y(Alignment::Center)
            .push(util::input_label(format!("{}", &self.motor)));

        let row = match &self.state {
            TaggedMotorState::Tagged { tag, valid } => {
                row.push(
                    TextInput::new("motor tag", tag)
                        .on_input(|text| MotorMessage::TagUpdated { tag: text, valid: *valid })
                        .on_paste(|text| MotorMessage::TagUpdated { tag: text, valid: *valid })
                        .width(Length::Fixed(TAG_INPUT_WIDTH))
                        .padding(TEXT_INPUT_PADDING)
                        .style(|theme, status| {
                            // example: https://github.com/iced-rs/iced/blob/master/examples/scrollable/src/main.rs
                            ElementAppearance::from(&self.state).text_input_custom_style(theme, status)
                        })
                )
                    .push(
                        Button::new(Text::new("x")) // font doesn't support funny characters like "✕"
                            .on_press(MotorMessage::TagDeleted)
                    )
            }
            TaggedMotorState::Untagged => {
                row.push(
                    TextInput::new("motor tag", "")
                        .on_input(|text| MotorMessage::TagUpdated { tag: text, valid: true })
                        .on_paste(|text| MotorMessage::TagUpdated { tag: text, valid: true })
                        .width(Length::Fixed(TAG_INPUT_WIDTH))
                        .padding(TEXT_INPUT_PADDING)
                )
            }
        };

        row.into()
    }
}

#[derive(Clone, Debug)]
pub enum TaggedMotorState {
    Tagged {
        tag: String,
        valid: bool,
    },
    Untagged,
}
