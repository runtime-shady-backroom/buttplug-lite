// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

pub trait FloatExtensions {
    fn filter_nan(self) -> Self;
}

impl FloatExtensions for f64 {
    fn filter_nan(self) -> f64 {
        if self.is_nan() {
            0.0
        } else {
            self
        }
    }
}
