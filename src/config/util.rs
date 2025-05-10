// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::ops::DerefMut as _;
use std::path::PathBuf;
use std::{convert, fs};

use directories::ProjectDirs;
use lazy_static::lazy_static;
use tokio::sync::mpsc;
use tokio::task;
use tracing::{info, warn};

use crate::config::v2::ConfigurationV2;
use crate::config::v3::ConfigurationV3;
use crate::config::ConfigurationMinimal;
use crate::config::CONFIG_VERSION;
use crate::{ApplicationState, ApplicationStateDb, ShutdownMessage};

static CONFIG_FILE_NAME: &str = "config.toml";

lazy_static! {
    pub static ref CONFIG_DIR_FILE_PATH: PathBuf = create_config_file_path();
}

fn get_config_dir() -> PathBuf {
    ProjectDirs::from("io.github", "runtime-shady-backroom", env!("CARGO_PKG_NAME"))
        .expect("unable to locate configuration directory")
        .config_dir()
        .into()
}

fn create_config_file_path() -> PathBuf {
    let config_dir_path: PathBuf = get_config_dir();
    fs::create_dir_all(config_dir_path.as_path()).expect("failed to create configuration directory");
    config_dir_path.join(CONFIG_FILE_NAME)
}

pub fn get_backup_config_file_path(version: i32) -> PathBuf {
    get_config_dir().join(format!("backup_config_v{version}.toml"))
}

/// update in-memory configuration
pub async fn update_configuration(
    application_state_db: &ApplicationStateDb,
    configuration: ConfigurationV3,
    warp_shutdown_tx: &mpsc::UnboundedSender<ShutdownMessage>,
) -> Result<ConfigurationV3, String> {
    save_configuration(&configuration).await?;
    let mut lock = application_state_db.write().await;
    let previous_state = lock.deref_mut().take();
    match previous_state {
        Some(ApplicationState {
            client,
            configuration: previous_configuration,
            device_manager,
        }) => {
            let new_port = configuration.port;
            *lock = Some(ApplicationState {
                client,
                configuration: configuration.clone(),
                device_manager,
            });
            drop(lock);

            // restart warp if necessary
            if new_port != previous_configuration.port {
                warp_shutdown_tx
                    .send(ShutdownMessage::Restart)
                    .map_err(|e| format!("{e:?}"))?;
            }

            Ok(configuration)
        }
        None => Err("cannot update configuration until after initial haptic server startup".into()),
    }
}

/// save configuration to disk
pub async fn save_configuration(configuration: &ConfigurationV3) -> Result<(), String> {
    // config serialization should never fail, so we should be good to panic
    let serialized_config = toml::to_string(configuration).expect("failed to serialize configuration");
    task::spawn_blocking(|| fs::write(CONFIG_DIR_FILE_PATH.as_path(), serialized_config).map_err(|e| format!("{e:?}")))
        .await
        .map_err(|e| format!("{e:?}"))
        .and_then(convert::identity)
}

pub async fn load_configuration() -> ConfigurationV3 {
    info!("Attempting to load config from {:?}", *CONFIG_DIR_FILE_PATH);
    let loaded_configuration: Result<ConfigurationMinimal, String> = fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
        .map_err(|e| format!("{e:?}"))
        .and_then(|string| toml::from_str(&string).map_err(|e| format!("{e:?}")));
    let configuration: ConfigurationV3 = match loaded_configuration {
        Ok(configuration) => {
            let loaded_configuration: Result<ConfigurationV3, String> = if configuration.version < 3 {
                fs::copy(
                    CONFIG_DIR_FILE_PATH.as_path(),
                    get_backup_config_file_path(configuration.version),
                )
                .expect("failed to back up config");
                info!("converting v{} config to v{}", configuration.version, CONFIG_VERSION);
                fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                    .map_err(|e| format!("{e:?}"))
                    .and_then(|string| toml::from_str::<ConfigurationV2>(&string).map_err(|e| format!("{e:?}")))
                    .map(|config| config.into())
            } else {
                fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                    .map_err(|e| format!("{e:?}"))
                    .and_then(|string| toml::from_str::<ConfigurationV3>(&string).map_err(|e| format!("{e:?}")))
            };

            match loaded_configuration {
                Ok(configuration) => configuration,
                Err(e) => {
                    // attempt to backup old config file when read fails
                    fs::copy(
                        CONFIG_DIR_FILE_PATH.as_path(),
                        get_backup_config_file_path(configuration.version),
                    )
                    .expect("failed to back up config");
                    warn!("falling back to default config due to error: {e}");
                    ConfigurationV3::default()
                }
            }
        }
        Err(e) => {
            warn!("falling back to default config due to error: {e}");
            ConfigurationV3::default()
        }
    };
    info!("Loaded configuration v{} from disk", configuration.version);

    if configuration.is_outdated() {
        let new_configuration = configuration.new_with_current_version();
        match save_configuration(&new_configuration).await {
            Ok(_) => {
                info!("Migrated configuration to new directory");
                new_configuration
            }
            Err(e) => {
                warn!("Error migrating configuration to new directory: {e}");
                configuration
            }
        }
    } else {
        configuration
    }
}
