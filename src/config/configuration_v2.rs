// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::collections::HashMap;

use serde::Deserialize;

fn default_version() -> i32 {
    1
}

#[derive(Deserialize)]
pub struct ConfigurationV2 {
    #[serde(default = "default_version")]
    pub version: i32,
    pub port: u16,
    /// map of tag name to motor struct
    pub tags: HashMap<String, MotorConfigurationV2>,
}

// encodes the "address" of a specific motor
#[derive(Deserialize)]
pub struct MotorConfigurationV2 {
    pub device_name: String,
    pub feature_type: MotorTypeV2,
    pub feature_index: u32,
}

#[derive(Deserialize, Eq, PartialEq)]
pub enum MotorTypeV2 {
    Linear,
    Rotation,
    Vibration,
    Contraction,
}
