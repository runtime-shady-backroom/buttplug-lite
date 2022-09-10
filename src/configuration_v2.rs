use core::slice::Iter;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use buttplug::core::message::ButtplugDeviceMessageType;
use serde::{Deserialize, Serialize};
use crate::CONFIG_VERSION;

const DEFAULT_PORT: u16 = 3031;

fn default_version() -> i32 {
    1
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct ConfigurationV2 {
    #[serde(default = "default_version")]
    pub version: i32,
    pub port: u16,
    /// map of tag name to motor struct
    pub tags: HashMap<String, MotorConfigurationV2>,
}

impl ConfigurationV2 {
    pub fn new(port: u16, tags: HashMap<String, MotorConfigurationV2>) -> ConfigurationV2 {
        ConfigurationV2 {
            version: CONFIG_VERSION,
            port,
            tags,
        }
    }

    pub fn new_with_current_version(&self) -> ConfigurationV2 {
        ConfigurationV2 {
            version: CONFIG_VERSION,
            port: self.port,
            tags: self.tags.clone(),
        }
    }

    pub fn motor_from_tag(&self, tag: &str) -> Option<&MotorConfigurationV2> {
        self.tags.get(tag)
    }

    pub fn is_version_outdated(version: i32) -> bool {
        version < CONFIG_VERSION
    }

    pub fn is_outdated(&self) -> bool {
        ConfigurationV2::is_version_outdated(self.version)
    }
}

impl Default for ConfigurationV2 {
    fn default() -> Self {
        ConfigurationV2 {
            version: CONFIG_VERSION,
            port: DEFAULT_PORT,
            tags: Default::default(),
        }
    }
}

// encodes the "address" of a specific motor
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct MotorConfigurationV2 {
    pub device_name: String,
    pub feature_type: MotorTypeV2,
    pub feature_index: u32,
}

impl Display for MotorConfigurationV2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}#{}", self.device_name, self.feature_type, self.feature_index)
    }
}

const MOTOR_TYPES: [MotorTypeV2; 4] = [
    MotorTypeV2::Vibration,
    MotorTypeV2::Linear,
    MotorTypeV2::Rotation,
    MotorTypeV2::Contraction,
];

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum MotorTypeV2 {
    Linear,
    Rotation,
    Vibration,
    Contraction,
}

impl MotorTypeV2 {
    pub fn get_type(&self) -> Option<ButtplugDeviceMessageType> {
        match self {
            MotorTypeV2::Vibration => Some(ButtplugDeviceMessageType::VibrateCmd),
            MotorTypeV2::Linear => Some(ButtplugDeviceMessageType::LinearCmd),
            MotorTypeV2::Rotation => Some(ButtplugDeviceMessageType::RotateCmd),
            MotorTypeV2::Contraction => None
        }
    }

    pub fn iter<'a>() -> Iter<'a, MotorTypeV2> {
        MOTOR_TYPES.iter()
    }
}

impl Display for MotorTypeV2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MotorTypeV2::Linear => write!(f, "linear"),
            MotorTypeV2::Rotation => write!(f, "rotation"),
            MotorTypeV2::Vibration => write!(f, "vibration"),
            MotorTypeV2::Contraction => write!(f, "contraction"),
        }
    }
}
