use serde::{Deserialize, Serialize};
use crate::CONFIG_VERSION;

fn default_version() -> i32 {
    1
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct ConfigurationMinimal {
    #[serde(default = "default_version")]
    pub version: i32,
}

impl ConfigurationMinimal {
    pub fn is_version_outdated(version: i32) -> bool {
        version < CONFIG_VERSION
    }

    pub fn is_outdated(&self) -> bool {
        ConfigurationMinimal::is_version_outdated(self.version)
    }
}

impl Default for ConfigurationMinimal {
    fn default() -> Self {
        ConfigurationMinimal {
            version: CONFIG_VERSION,
        }
    }
}
