// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::collections::HashMap;

use buttplug::core::message::ActuatorType;

/// Desired settings for all the motors in a single device
#[derive(Default)]
pub struct MotorSettings {
    pub scalar_map: HashMap<u32, (f64, ActuatorType)>,
    pub rotate_map: HashMap<u32, (f64, bool)>,
    pub linear_map: HashMap<u32, (u32, f64)>,
}
