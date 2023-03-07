//! Simple structs used locally by the buttplug code

use crate::app::structs::DeviceStatus;
use crate::config::v3::MotorConfigurationV3;

/// intermediate struct used to return partially processed device info
pub(super) struct DeviceList {
    pub(super) motors: Vec<MotorConfigurationV3>,
    pub(super) devices: Vec<DeviceStatus>,
}
