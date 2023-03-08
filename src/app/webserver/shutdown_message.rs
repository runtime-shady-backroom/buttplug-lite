// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

#[derive(Debug)]
pub enum ShutdownMessage {
    Restart,
    Shutdown,
}
