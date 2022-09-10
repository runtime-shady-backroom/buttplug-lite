use core::slice::Iter;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use buttplug::core::message::ButtplugDeviceMessageType;
use serde::{Deserialize, Serialize};
use crate::{CONFIG_VERSION, ConfigurationV2};
use buttplug::core::message::ActuatorType as ButtplugActuatorType;

const DEFAULT_PORT: u16 = 3031;

fn default_version() -> i32 {
    1
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct ConfigurationV3 {
    #[serde(default = "default_version")]
    pub version: i32,
    pub port: u16,
    /// map of tag name to motor struct
    pub tags: HashMap<String, MotorConfigurationV3>,
}

impl ConfigurationV3 {
    pub fn new(port: u16, tags: HashMap<String, MotorConfigurationV3>) -> ConfigurationV3 {
        ConfigurationV3 {
            version: CONFIG_VERSION,
            port,
            tags,
        }
    }

    pub fn new_with_current_version(&self) -> ConfigurationV3 {
        ConfigurationV3 {
            version: CONFIG_VERSION,
            port: self.port,
            tags: self.tags.clone(),
        }
    }

    pub fn motor_from_tag(&self, tag: &str) -> Option<&MotorConfigurationV3> {
        self.tags.get(tag)
    }

    pub fn is_version_outdated(version: i32) -> bool {
        version < CONFIG_VERSION
    }

    pub fn is_outdated(&self) -> bool {
        ConfigurationV3::is_version_outdated(self.version)
    }
}

impl Default for ConfigurationV3 {
    fn default() -> Self {
        ConfigurationV3 {
            version: CONFIG_VERSION,
            port: DEFAULT_PORT,
            tags: Default::default(),
        }
    }
}

impl From<ConfigurationV2> for ConfigurationV3 {
    fn from(configuration_v2: ConfigurationV2) -> Self {
        todo!()
    }
}

// encodes the "address" of a specific motor
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct MotorConfigurationV3 {
    pub device_name: String,
    pub feature_type: MotorTypeV3,
    pub feature_index: u32,
}

impl Display for MotorConfigurationV3 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}#{}", self.device_name, self.feature_type, self.feature_index)
    }
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum MotorTypeV3 {
    Linear,
    Rotation,
    Scalar(ActuatorType),
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum ActuatorType {
    Vibrate,
    Rotate,
    Oscillate,
    Constrict,
    Inflate,
    Position,
}

impl ActuatorType {
    pub fn from_buttplug(at: &ButtplugActuatorType) -> ActuatorType {
        match at {
            ButtplugActuatorType::Vibrate => ActuatorType::Vibrate,
            ButtplugActuatorType::Rotate => ActuatorType::Rotate,
            ButtplugActuatorType::Oscillate => ActuatorType::Oscillate,
            ButtplugActuatorType::Constrict => ActuatorType::Constrict,
            ButtplugActuatorType::Inflate => ActuatorType::Inflate,
            ButtplugActuatorType::Position => ActuatorType::Position,
        }
    }

    pub fn to_buttplug(&self) -> ButtplugActuatorType {
        match self {
            ActuatorType::Vibrate => ButtplugActuatorType::Vibrate,
            ActuatorType::Rotate => ButtplugActuatorType::Rotate,
            ActuatorType::Oscillate => ButtplugActuatorType::Oscillate,
            ActuatorType::Constrict => ButtplugActuatorType::Constrict,
            ActuatorType::Inflate => ButtplugActuatorType::Inflate,
            ActuatorType::Position => ButtplugActuatorType::Position,
        }
    }
}

impl Display for ActuatorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ActuatorType::Vibrate => write!(f, "vibrate"),
            ActuatorType::Rotate => write!(f, "rotate"),
            ActuatorType::Oscillate => write!(f, "oscillate"),
            ActuatorType::Constrict => write!(f, "constrict"),
            ActuatorType::Inflate => write!(f, "inflate"),
            ActuatorType::Position => write!(f, "position"),
        }
    }
}

impl MotorTypeV3 {
    pub fn get_type(&self) -> Option<ButtplugDeviceMessageType> {
        match self {
            MotorTypeV3::Scalar(_) => Some(ButtplugDeviceMessageType::ScalarCmd),
            MotorTypeV3::Linear => Some(ButtplugDeviceMessageType::LinearCmd),
            MotorTypeV3::Rotation => Some(ButtplugDeviceMessageType::RotateCmd),
        }
    }
}

impl Display for MotorTypeV3 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MotorTypeV3::Linear => write!(f, "linear"),
            MotorTypeV3::Rotation => write!(f, "rotation"),
            MotorTypeV3::Scalar(actuator_type) => write!(f, "scalar({})", actuator_type),
        }
    }
}
