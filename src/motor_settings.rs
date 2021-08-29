use std::collections::HashMap;

#[derive(Default)]
pub struct MotorSettings {
    pub speed_map: HashMap<u32, f64>,
    pub rotate_map: HashMap<u32, (f64, bool)>,
    pub linear_map: HashMap<u32, (u32, f64)>,
    pub contraction_hack: Option<u8>,
}
