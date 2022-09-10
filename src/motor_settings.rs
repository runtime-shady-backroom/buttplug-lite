use std::collections::HashMap;
use buttplug::core::message::ActuatorType;

#[derive(Default)]
pub struct MotorSettings {
    pub scalar_map: HashMap<u32, (f64, ActuatorType)>,
    pub rotate_map: HashMap<u32, (f64, bool)>,
    pub linear_map: HashMap<u32, (u32, f64)>,
}
