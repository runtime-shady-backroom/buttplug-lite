use core::slice::Iter;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use buttplug::core::messages::ButtplugCurrentSpecDeviceMessageType;
use serde::{Deserialize, Serialize};

const DEFAULT_PORT: u16 = 3031;

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct Configuration {
    pub port: u16,
    pub tags: HashMap<String, Motor>,
}

impl Configuration {
    pub fn motor_from_tag(&self, tag: &String) -> Option<&Motor> {
        self.tags.get(tag)
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            port: DEFAULT_PORT,
            tags: Default::default(),
        }
    }
}

// encodes the "address" of a specific motor
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Motor {
    pub device_name: String,
    pub feature_type: MotorType,
    pub feature_index: u32,
}

impl Display for Motor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}#{}", self.device_name, self.feature_type, self.feature_index)
    }
}

const MOTOR_TYPES: [MotorType; 4] = [
    MotorType::Vibration,
    MotorType::Linear,
    MotorType::Rotation,
    MotorType::Contraction,
];

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum MotorType {
    Linear,
    Rotation,
    Vibration,
    Contraction,
}

impl MotorType {
    pub fn get_type(&self) -> Option<ButtplugCurrentSpecDeviceMessageType> {
        match self {
            MotorType::Vibration => Some(ButtplugCurrentSpecDeviceMessageType::VibrateCmd),
            MotorType::Linear => Some(ButtplugCurrentSpecDeviceMessageType::LinearCmd),
            MotorType::Rotation => Some(ButtplugCurrentSpecDeviceMessageType::RotateCmd),
            MotorType::Contraction => None
        }
    }

    pub fn iter<'a>() -> Iter<'a, MotorType> {
        MOTOR_TYPES.iter()
    }
}

impl Display for MotorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MotorType::Linear => write!(f, "linear"),
            MotorType::Rotation => write!(f, "rotation"),
            MotorType::Vibration => write!(f, "vibration"),
            MotorType::Contraction => write!(f, "contraction"),
        }
    }
}
