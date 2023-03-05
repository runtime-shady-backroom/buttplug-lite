// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use serde::Deserialize;

use super::CONFIG_VERSION;

fn default_version() -> i32 {
    1
}

#[derive(Deserialize)]
pub struct ConfigurationMinimal {
    #[serde(default = "default_version")]
    pub version: i32,
}

impl Default for ConfigurationMinimal {
    fn default() -> Self {
        ConfigurationMinimal {
            version: CONFIG_VERSION,
        }
    }
}
