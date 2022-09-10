use serde::Deserialize;
use crate::CONFIG_VERSION;

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
