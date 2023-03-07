use crate::app::structs::DeviceStatus;
use crate::config::v3::ConfigurationV3;
use crate::gui::TaggedMotor;

/// full list of all device information we could ever want
#[derive(Clone, Debug)]
pub struct ApplicationStatus {
    pub motors: Vec<TaggedMotor>,
    pub devices: Vec<DeviceStatus>,
    pub configuration: ConfigurationV3,
}
