// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

/// extension functions for floating-point numbers
pub trait FloatExtensions {
    fn filter_nan(self) -> Self;
}

impl FloatExtensions for f32 {
    fn filter_nan(self) -> f32 {
        if self.is_nan() {
            0.0
        } else {
            self
        }
    }
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
