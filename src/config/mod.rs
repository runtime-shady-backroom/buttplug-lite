// Copyright 2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

pub use configuration_minimal::ConfigurationMinimal;
pub use util::*;

mod configuration_minimal;
mod configuration_v2;
mod configuration_v3;
mod util;

pub mod v2 {
    pub use super::configuration_v2::*;
}

pub mod v3 {
    pub use super::configuration_v3::*;
}

pub const CONFIG_VERSION: i32 = 3;
