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
        let battery = if let Some(battery) = self.battery_level {
            format!("battery={:.0}%", battery * 100.0)
        } else {
            String::new()
        };
        let rssi = if let Some(rssi) = self.rssi_level {
            format!("rssi={}", rssi)
        } else {
            String::new()
        };
        if battery.is_empty() && rssi.is_empty() {
            write!(f, "{}", self.name)
        } else if rssi.is_empty() {
            // yes battery, no rssi
            write!(f, "{} ({})", self.name, battery)
        } else if battery.is_empty() {
            // yes rssi, no battery
            write!(f, "{} ({})", self.name, rssi)
        } else {
            // yes battery, yes rssi
            write!(f, "{} ({}, {})", self.name, battery, rssi)
        }
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
