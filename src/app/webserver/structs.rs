// Copyright 2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! Simple structs used locally by the route code

use crate::config::v3::MotorConfigurationV3;

#[derive(Eq, PartialEq, Hash)]
pub struct DeviceId {
    pub name: String,
    pub identifier: Option<String>,
}

impl DeviceId {
    pub fn without_identifier(&self) -> DeviceId {
        DeviceId {
            name: self.name.to_owned(),
            identifier: None,
        }
    }
}

impl From<&MotorConfigurationV3> for DeviceId {
    fn from(motor_config: &MotorConfigurationV3) -> Self {
        DeviceId {
            name: motor_config.device_name.to_owned(),
            identifier: motor_config.device_identifier.to_owned(),
        }
    }
}
