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
