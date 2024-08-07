// Copyright 2022-2024 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use iced::{Border, Color};
use iced::widget::text_input;

use crate::gui::tagged_motor::TaggedMotorState;
use crate::gui::theme::Theme;

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

// check `theme.rs`'s `impl text_input::StyleSheet for Theme` for a reference implementation
impl text_input::StyleSheet for ElementAppearance {
    type Style = Theme;

    fn active(&self, style: &Self::Style) -> text_input::Appearance {
        let palette = style.extended_palette();

        let border_color = match self {
            ElementAppearance::Invalid => palette.danger.strong.color,
            _ => palette.background.strong.color,
        };

        let icon_color = match self {
            ElementAppearance::Invalid => palette.danger.weak.text,
            _ => palette.background.weak.text,
        };

        text_input::Appearance {
            background: palette.background.base.color.into(),
            border: Border {
                radius: 2.0.into(),
                width: 1.0,
                color: border_color,
            },
            icon_color,
        }
    }

    fn focused(&self, style: &Self::Style) -> text_input::Appearance {
        let palette = style.extended_palette();

        let border_color = match self {
            ElementAppearance::Invalid => palette.danger.strong.color,
            _ => palette.primary.strong.color,
        };

        let icon_color = match self {
            ElementAppearance::Invalid => palette.danger.weak.text,
            _ => palette.background.weak.text,
        };

        text_input::Appearance {
            background: palette.background.base.color.into(),
            border: Border {
                radius: 2.0.into(),
                width: 1.0,
                color: border_color,
            },
            icon_color,
        }
    }

    fn placeholder_color(&self, style: &Self::Style) -> Color {
        let palette = style.extended_palette();

        match self {
            ElementAppearance::Invalid => palette.danger.strong.color,
            _ => palette.background.strong.color,
        }
    }

    fn value_color(&self, style: &Self::Style) -> Color {
        let palette = style.extended_palette();

        match self {
            ElementAppearance::Invalid => palette.danger.base.text,
            _ => palette.background.base.text,
        }
    }

    fn disabled_color(&self, style: &Self::Style) -> Color {
        self.placeholder_color(style)
    }

    fn selection_color(&self, style: &Self::Style) -> Color {
        let palette = style.extended_palette();

        match self {
            ElementAppearance::Invalid => palette.danger.weak.color,
            _ => palette.primary.weak.color,
        }
    }

    fn hovered(&self, style: &Self::Style) -> text_input::Appearance {
        let palette = style.extended_palette();

        let border_color = match self {
            ElementAppearance::Invalid => palette.danger.base.text,
            _ => palette.background.base.text,
        };

        let icon_color = match self {
            ElementAppearance::Invalid => palette.danger.weak.text,
            _ => palette.background.weak.text,
        };

        text_input::Appearance {
            background: palette.background.base.color.into(),
            border: Border {
                radius: 2.0.into(),
                width: 1.0,
                color: border_color,
            },
            icon_color,
        }
    }

    fn disabled(&self, style: &Self::Style) -> text_input::Appearance {
        let palette = style.extended_palette();

        let border_color = match self {
            ElementAppearance::Invalid => palette.danger.strong.color,
            _ => palette.background.strong.color,
        };

        let icon_color = match self {
            ElementAppearance::Invalid => palette.danger.strong.color,
            _ => palette.background.strong.color,
        };

        text_input::Appearance {
            background: palette.background.weak.color.into(),
            border: Border {
                radius: 2.0.into(),
                width: 1.0,
                color: border_color,
            },
            icon_color,
        }
    }
}
