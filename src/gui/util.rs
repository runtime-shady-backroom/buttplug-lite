// Copyright 2022-2025 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use crate::gui::constants::TEXT_INPUT_PADDING;
use iced::application::Title;
use iced::widget::{Container, Text};
use iced::Element;
use iced_futures::core::text;

pub fn input_label<'a, S: text::IntoFragment<'a>, T: 'a>(label: S) -> Element<'a, T> {
    let text = Text::new(label);

    Container::new(text)
        .padding(TEXT_INPUT_PADDING)
        .into()
}

/// Helper struct because for some reason iced does not provide a default `Title` impl for `String` or even `&str`, they only provide it for `&'static str`
pub struct ConstantTitle(pub String);

impl <State> Title<State> for ConstantTitle {
    fn title(&self, _state: &State) -> String {
        self.0.clone()
    }
}
