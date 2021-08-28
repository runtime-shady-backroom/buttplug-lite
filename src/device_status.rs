use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::fmt;

/// status of a single device
#[derive(Clone, Debug)]
pub struct DeviceStatus {
    pub name: String,
    pub battery_level: Option<f64>,
    pub rssi_level: Option<i32>,
}

impl Display for DeviceStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} battery={:?} rssi={:?}", self.name, self.battery_level, self.rssi_level)
    }
}

impl Eq for DeviceStatus {}

impl PartialEq for DeviceStatus {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Ord for DeviceStatus {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for DeviceStatus {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
