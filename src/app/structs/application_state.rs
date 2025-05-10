// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use crate::config::v3::ConfigurationV3;
use buttplug::client::ButtplugClient;
use buttplug::server::device::ServerDeviceManager;
use std::sync::Arc;
use tokio::sync::RwLock;

// global state types
pub type ApplicationStateDb = Arc<RwLock<Option<ApplicationState>>>;

// eventually I'd like some way to get a ref to the server in here
pub struct ApplicationState {
    pub client: ButtplugClient,
    pub configuration: ConfigurationV3,
    pub device_manager: Arc<ServerDeviceManager>,
}
