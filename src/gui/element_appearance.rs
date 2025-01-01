// Copyright 2022-2025 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use iced::widget::text_input;
use iced::{Background, Border, Theme};
use iced::widget::text_input::Status;
use crate::gui::tagged_motor::TaggedMotorState;

pub enum ElementAppearance {
    Valid,
    Invalid,
}

impl From<&TaggedMotorState> for ElementAppearance {
    fn from(value: &TaggedMotorState) -> Self {
        match value {
            TaggedMotorState::Tagged { valid: false, .. } => ElementAppearance::Invalid,
            _ => ElementAppearance::Valid,
        }
    }
}

impl ElementAppearance {

    // example: https://github.com/iced-rs/iced/blob/master/examples/scrollable/src/main.rs
    pub fn text_input_custom_style(&self, theme: &Theme, status: Status) -> text_input::Style {

        // see https://github.com/iced-rs/iced/blob/master/widget/src/text_input.rs for defaults
        let palette = theme.extended_palette();


        let default_background_color = Background::Color(palette.background.base.color);
        let default_value_color = match self {
            ElementAppearance::Invalid => palette.danger.base.text,
            _ => palette.background.base.text,
        };
        let (background_color, border_color, value_color) = match status {
            Status::Active => { // the "base" style
                let background_color = default_background_color;
                let border_color = match self {
                    ElementAppearance::Invalid => palette.danger.strong.color,
                    _ => palette.background.strong.color,
                };
                let value_color = default_value_color;
                
                (background_color, border_color, value_color)
            }
            Status::Hovered => {
                let background_color = default_background_color;
                let border_color = match self {
                    ElementAppearance::Invalid => palette.danger.strong.color,
                    _ => palette.background.base.text, // different border color from active
                };
                let value_color = default_value_color;
                
                (background_color, border_color, value_color)
            }
            Status::Focused => {
                let background_color = default_background_color;
                let border_color = match self {
                    ElementAppearance::Invalid => palette.danger.strong.color,
                    _ => palette.primary.strong.color, // different border color from active
                };
                let value_color = default_value_color;
                
                (background_color, border_color, value_color)
            }
            Status::Disabled => {
                let background_color = Background::Color(palette.background.weak.color); // different background color from active
                let border_color = match self {
                    ElementAppearance::Invalid => palette.danger.strong.color,
                    _ => palette.background.strong.color,
                };
                let value_color = palette.background.strong.color; // different value color from active
                
                (background_color, border_color, value_color)
            }
        };

        let icon_color = match self {
            ElementAppearance::Invalid => palette.danger.weak.text,
            _ => palette.background.weak.text,
        };

        let placeholder_color = match self {
            ElementAppearance::Invalid => palette.danger.strong.color,
            _ => palette.background.strong.color,
        };

        let selection_color = match self {
            ElementAppearance::Invalid => palette.danger.weak.color,
            _ => palette.primary.weak.color,
        };

        text_input::Style {
            background: background_color,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 2.0.into(),
            },
            icon: icon_color,
            placeholder: placeholder_color,
            value: value_color,
            selection: selection_color,
        }
    }
}
