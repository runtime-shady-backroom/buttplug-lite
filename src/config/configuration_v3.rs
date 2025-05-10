// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::{Display, Formatter};

use buttplug::core::message::ActuatorType as ButtplugActuatorType;
use serde::{Deserialize, Serialize};

use crate::config::v2::{ConfigurationV2, MotorConfigurationV2, MotorTypeV2};

use super::CONFIG_VERSION;

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
        // find any devices that contain a contraction type, because we can't safely port over ANY of their motors
        let bad_device_names: HashSet<String> = configuration_v2
            .tags
            .values()
            .filter(|value| value.feature_type == MotorTypeV2::Contraction)
            .map(|value| value.device_name.to_owned())
            .collect();

        ConfigurationV3 {
            version: configuration_v2.version,
            port: configuration_v2.port,
            tags: configuration_v2
                .tags
                .into_iter()
                .filter(|(_key, value)| !bad_device_names.contains(&value.device_name))
                .filter_map(|(key, value)| value.try_into().ok().map(|value| (key, value)))
                .collect(),
        }
    }
}

// encodes the "address" of a specific motor
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct MotorConfigurationV3 {
    pub device_name: String,
    pub device_identifier: Option<String>,
    pub feature_index: u32,
    pub feature_type: MotorTypeV3,
}

impl Display for MotorConfigurationV3 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.device_identifier {
            Some(_identifier) => write!(f, "{} {}#{}", self.device_name, self.feature_type, self.feature_index),
            None => write!(
                f,
                "{} {}#{} [LEGACY]",
                self.device_name, self.feature_type, self.feature_index
            ),
        }
    }
}

impl TryFrom<MotorConfigurationV2> for MotorConfigurationV3 {
    type Error = ();

    fn try_from(config_v2: MotorConfigurationV2) -> Result<Self, Self::Error> {
        config_v2.feature_type.try_into().map(|type_v3| MotorConfigurationV3 {
            device_name: config_v2.device_name,
            device_identifier: None,
            feature_type: type_v3,
            feature_index: config_v2.feature_index,
        })
    }
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
#[serde(tag = "type")]
pub enum MotorTypeV3 {
    Linear,
    Rotation,
    Scalar { actuator_type: ActuatorType },
}

impl TryFrom<MotorTypeV2> for MotorTypeV3 {
    type Error = ();

    fn try_from(type_v2: MotorTypeV2) -> Result<Self, Self::Error> {
        match type_v2 {
            MotorTypeV2::Linear => Ok(MotorTypeV3::Linear),
            MotorTypeV2::Rotation => Ok(MotorTypeV3::Rotation),
            MotorTypeV2::Vibration => Ok(MotorTypeV3::Scalar {
                actuator_type: ActuatorType::Vibrate,
            }),
            MotorTypeV2::Contraction => Err(()),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum ActuatorType {
    Vibrate,
    Rotate,
    Oscillate,
    Constrict,
    Inflate,
    Position,
    Unknown,
}

impl ActuatorType {
    pub fn to_buttplug(&self) -> ButtplugActuatorType {
        match self {
            ActuatorType::Vibrate => ButtplugActuatorType::Vibrate,
            ActuatorType::Rotate => ButtplugActuatorType::Rotate,
            ActuatorType::Oscillate => ButtplugActuatorType::Oscillate,
            ActuatorType::Constrict => ButtplugActuatorType::Constrict,
            ActuatorType::Inflate => ButtplugActuatorType::Inflate,
            ActuatorType::Position => ButtplugActuatorType::Position,
            ActuatorType::Unknown => ButtplugActuatorType::Unknown,
        }
    }
}

impl From<&ButtplugActuatorType> for ActuatorType {
    fn from(at: &ButtplugActuatorType) -> Self {
        match at {
            ButtplugActuatorType::Vibrate => ActuatorType::Vibrate,
            ButtplugActuatorType::Rotate => ActuatorType::Rotate,
            ButtplugActuatorType::Oscillate => ActuatorType::Oscillate,
            ButtplugActuatorType::Constrict => ActuatorType::Constrict,
            ButtplugActuatorType::Inflate => ActuatorType::Inflate,
            ButtplugActuatorType::Position => ActuatorType::Position,
            ButtplugActuatorType::Unknown => ActuatorType::Unknown,
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
            ActuatorType::Unknown => write!(f, "unknown"),
        }
    }
}

impl Display for MotorTypeV3 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MotorTypeV3::Linear => write!(f, "linear"),
            MotorTypeV3::Rotation => write!(f, "rotation"),
            MotorTypeV3::Scalar { actuator_type } => write!(f, "scalar ({actuator_type})"),
        }
    }
}
