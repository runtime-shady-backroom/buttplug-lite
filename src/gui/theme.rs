// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use iced::{Color, theme};
use lazy_static::lazy_static;

pub type Theme = iced::Theme;

const DARK_PALETTE: theme::Palette = theme::Palette {
    background: Color::from_rgb(
        0x36 as f32 / 255.0,
        0x39 as f32 / 255.0,
        0x3F as f32 / 255.0,
    ),
    text: Color::from_rgb(1.0, 1.0, 1.0),
    primary: Color::from_rgb(
        0x72 as f32 / 255.0,
        0x89 as f32 / 255.0,
        0xDA as f32 / 255.0,
    ),
    success: Color::from_rgb(
        0x12 as f32 / 255.0,
        0x66 as f32 / 255.0,
        0x4F as f32 / 255.0,
    ),
    danger: Color::from_rgb(
        0xC3 as f32 / 255.0,
        0x42 as f32 / 255.0,
        0x3F as f32 / 255.0,
    ),
};

lazy_static! {
    pub static ref THEME: Theme = Theme::custom(DARK_PALETTE);
}
