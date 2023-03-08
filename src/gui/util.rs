// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::borrow::Cow;
use iced::Element;
use iced::widget::{Container, Text};
use crate::gui::constants::TEXT_INPUT_PADDING;

pub fn input_label<'a, S: Into<Cow<'a, str>>, T: 'a>(label: S) -> Element<'a, T> {
    let text = Text::new(label);

    Container::new(text)
        .padding(TEXT_INPUT_PADDING)
        .into()
}
