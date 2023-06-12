// Copyright 2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! Logging-related utilities

use std::{fs, io};
use std::path::{Path, PathBuf};

use chrono::Local;
use directories::ProjectDirs;
use tracing::{debug, info, warn};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::util::SubscriberInitExt;

use crate::util;

const MAXIMUM_LOG_FILES: usize = 50;
static LOG_DIR_NAME: &str = "logs";

/// Initialize logging framework
#[must_use = "this `WorkerGuard` should live until the application shuts down"]
pub fn init(
    verbosity_level: u8,
    log_filter: Option<String>,
    use_stdout: bool,
    stdout_custom_panic_handler:
    bool, file_custom_panic_handler: bool
) -> Option<WorkerGuard> {
    let log_filter = get_log_filter(verbosity_level, log_filter);

    if use_stdout {
        init_console_logging(log_filter);
        set_panic_hook_and_log(stdout_custom_panic_handler);
        None
    } else {
        try_init_file_logging(log_filter, stdout_custom_panic_handler, file_custom_panic_handler)
    }
}

/// Initialize console logging for use in tests
#[cfg(test)]
pub fn init_console(custom_panic_handler: bool) {
    let log_filter = get_log_filter(1, None);
    init_console_logging(log_filter);
    set_panic_hook_and_log(custom_panic_handler);
}

/// Attempt to log to a file, gracefully falling back to stdout logging on failure
#[must_use = "this `WorkerGuard` should live until the application shuts down"]
fn try_init_file_logging(log_filter: EnvFilter, stdout_custom_panic_handler: bool, file_custom_panic_handler: bool) -> Option<WorkerGuard> {
    match create_log_dir_path() {
        Ok(log_dir_path) => {
            let file_appender = tracing_appender::rolling::never(log_dir_path, get_log_file_name());
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            init_file_logging(log_filter, non_blocking);
            set_panic_hook_and_log(file_custom_panic_handler);
            Some(guard)
        }
        Err(e) => {
            init_console_logging(log_filter);
            set_panic_hook_and_log(stdout_custom_panic_handler);
            warn!("File-based logging failed. Falling back to stdout: {e}");
            None
        }
    }
}

/// Start logging framework for stdout
fn init_console_logging(log_filter: EnvFilter) {
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .finish()
        .init();
}

/// Start logging framework for buffered file output
fn init_file_logging(log_filter: EnvFilter, non_blocking: NonBlocking) {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(non_blocking)
        .with_env_filter(log_filter)
        .finish()
        .init();
}

/// Set up custom panic handling. By default we only use this for file-based logging,
/// as if you're using console you can just see the built in panic handling print things.
fn set_panic_hook_and_log(custom_panic_handler: bool) {
    if custom_panic_handler {
        debug!("Setting up custom panic hook...");
        util::panic::set_hook();
    } else {
        info!("NOT using custom panic hook!");
    }
}

/// Get the appropriate log filter for the configured verbosity
fn get_log_filter(verbosity_level: u8, log_filter: Option<String>) -> EnvFilter {
    if let Some(log_filter_string) = log_filter {
        // user is providing a custom filter and not using my verbosity presets at all
        EnvFilter::try_new(log_filter_string).expect("failed to parse user-provided log filter")
    } else if verbosity_level == 0 {
        // I get info, everything else gets warn
        EnvFilter::try_new("warn,buttplug_lite=info").unwrap()
    } else if verbosity_level == 1 {
        // my debug logging, buttplug's info logging, everything gets warn
        EnvFilter::try_new("warn,buttplug=info,buttplug::server::device::server_device_manager_event_loop=warn,buttplug_derive=info,buttplug_lite=debug").unwrap()
    } else if verbosity_level == 2 {
        // my + buttplug's debug logging, everything gets info
        EnvFilter::try_new("info,buttplug=debug,buttplug_derive=debug,buttplug_lite=debug").unwrap()
    } else if verbosity_level == 3 {
        // everything gets debug
        EnvFilter::try_new("debug").unwrap()
    } else {
        // dear god everything gets trace
        EnvFilter::try_new("trace").unwrap()
    }
}

fn get_log_file_name() -> String {
    //TODO: this will cause problems if you launch the program twice in the same second...
    Local::now().format("%Y-%m-%d_%H-%M-%S.log").to_string()
}

fn get_log_dir() -> PathBuf {
    ProjectDirs::from("io.github", "runtime-shady-backroom", env!("CARGO_PKG_NAME"))
        .expect("unable to locate configuration directory")
        .data_dir()
        .join(LOG_DIR_NAME)
}

fn create_log_dir_path() -> io::Result<PathBuf> {
    let log_dir_path: PathBuf = get_log_dir();
    fs::create_dir_all(log_dir_path.as_path())?;
    clean_up_old_logs(log_dir_path.as_path())?;

    // new log file
    Ok(log_dir_path)
}

/// Delete oldest logs, retaining up to `MAXIMUM_LOG_FILES` files in the directory
fn clean_up_old_logs(path: &Path) -> io::Result<()> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map(|ext| ext == "log").unwrap_or(false) {
            paths.push(path);
        }
    }
    paths.sort_unstable();
    if let Some(logs_to_delete) = paths.len().checked_sub(MAXIMUM_LOG_FILES) {
        for path in paths.into_iter().take(logs_to_delete) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
