//! various simple structs used by the GUI

#[derive(Clone, Debug)]
pub enum MotorMessage {
    TagUpdated {
        tag: String,
        valid: bool,
    },
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
