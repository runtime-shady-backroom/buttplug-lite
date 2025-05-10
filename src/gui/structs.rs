// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! various simple structs used by the GUI

#[derive(Clone, Debug)]
pub enum MotorMessage {
    TagUpdated { tag: String, valid: bool },
    TagDeleted,
}

impl MotorMessage {
    pub fn tag(&self) -> Option<&str> {
        match self {
            MotorMessage::TagUpdated { tag, .. } => Some(tag),
            MotorMessage::TagDeleted => None,
        }
    }
}
