// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! Tokio-related utilities

use lazy_static::lazy_static;

lazy_static! {
    /// A global containing the tokio runtime used by this application
    pub static ref GLOBAL_TOKIO_RUNTIME: tokio::runtime::Runtime = create_tokio_runtime();
}

fn create_tokio_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
}
