use std::sync::Arc;
use buttplug::client::ButtplugClient;
use buttplug::server::device::ServerDeviceManager;
use tokio::sync::RwLock;
use crate::config::v3::ConfigurationV3;

// global state types
pub type ApplicationStateDb = Arc<RwLock<Option<ApplicationState>>>;

// eventually I'd like some way to get a ref to the server in here
pub struct ApplicationState {
    pub client: ButtplugClient,
    pub configuration: ConfigurationV3,
    pub device_manager: Arc<ServerDeviceManager>,
}
