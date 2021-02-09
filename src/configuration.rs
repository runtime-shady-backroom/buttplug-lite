use core::slice::Iter;
use std::collections::HashMap;
use std::sync::Arc;

use buttplug::core::messages::ButtplugCurrentSpecDeviceMessageType;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::configuration::MotorType::Vibration;

const DEFAULT_PORT: u16 = 3031;

pub type ConfigurationDb = Arc<RwLock<Configuration>>;

#[derive(Deserialize, Serialize, Debug)]
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
            tags: [
                ("o".into(), Motor { device_name: "Lovense Edge".into(), feature_index: 0, feature_type: Vibration }),
                ("i".into(), Motor { device_name: "Lovense Edge".into(), feature_index: 1, feature_type: Vibration }),
            ].iter().cloned().collect(), // TODO: replace with Default::default()
        }
    }
}

// encodes the "address" of a specific motor
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
pub struct Motor {
    pub device_name: String,
    pub feature_index: u32,
    pub feature_type: MotorType,
}

const MOTOR_TYPES: [MotorType; 3] = [
    MotorType::Vibration,
    MotorType::Linear,
    MotorType::Rotation,
];

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
pub enum MotorType {
    Vibration,
    Linear,
    Rotation,
}

impl MotorType {
    pub fn get_type(&self) -> ButtplugCurrentSpecDeviceMessageType {
        match self {
            Vibration => ButtplugCurrentSpecDeviceMessageType::VibrateCmd,
            MotorType::Linear => ButtplugCurrentSpecDeviceMessageType::LinearCmd,
            MotorType::Rotation => ButtplugCurrentSpecDeviceMessageType::RotateCmd,
        }
    }

    pub fn iter<'a>() -> Iter<'a, MotorType> {
        MOTOR_TYPES.iter()
    }
}
