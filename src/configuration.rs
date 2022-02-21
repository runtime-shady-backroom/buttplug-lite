use core::slice::Iter;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use buttplug::core::messages::ButtplugCurrentSpecDeviceMessageType;
use serde::{Deserialize, Serialize};

const CURRENT_VERSION: i32 = 2;
const DEFAULT_PORT: u16 = 3031;

fn default_version() -> i32 {
    1
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct Configuration {
    #[serde(default = "default_version")]
    pub version: i32,
    pub port: u16,
    /// map of tag name to motor struct
    pub tags: HashMap<String, Motor>,
}

impl Configuration {
    pub fn new(port: u16, tags: HashMap<String, Motor>) -> Configuration {
        Configuration {
            version: CURRENT_VERSION,
            port,
            tags,
        }
    }

    pub fn new_with_current_version(&self) -> Configuration {
        Configuration {
            version: CURRENT_VERSION,
            port: self.port,
            tags: self.tags.clone(),
        }
    }

    pub fn motor_from_tag(&self, tag: &str) -> Option<&Motor> {
        self.tags.get(tag)
    }

    pub fn is_version_outdated(version: i32) -> bool {
        version < CURRENT_VERSION
    }

    pub fn is_outdated(&self) -> bool {
        Configuration::is_version_outdated(self.version)
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            version: CURRENT_VERSION,
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
