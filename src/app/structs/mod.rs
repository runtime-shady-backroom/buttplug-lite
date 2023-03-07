// Copyright 2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! This module contains the larger structs used by this application.
//! Simpler structs that are just used to pack return values are declared where needed.

pub use application_state::*;
pub use cli_args::CliArgs;
pub use device_status::DeviceStatus;
pub use motor_settings::MotorSettings;

mod application_state;
mod cli_args;
mod device_status;
mod motor_settings;
