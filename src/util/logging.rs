// Copyright 2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! Logging-related utilities

use std::{fs, io};
use std::path::{Path, PathBuf};

use chrono::Local;
use directories::ProjectDirs;
use tracing::warn;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::util::SubscriberInitExt;

const MAXIMUM_LOG_FILES: usize = 50;
static LOG_DIR_NAME: &str = "logs";

#[must_use = "this `WorkerGuard` should live until the application shuts down"]
pub fn init(verbosity_level: u8, log_filter: Option<String>, use_stdout: bool) -> Option<WorkerGuard> {
    let log_filter = get_log_filter(verbosity_level, log_filter);

    if use_stdout {
        init_console_logging(log_filter);
        None
    } else {
        try_init_file_logging(log_filter)
    }
}

#[cfg(test)]
pub fn init_console() {
    let log_filter = get_log_filter(1, None);
    init_console_logging(log_filter);
}

fn init_console_logging(log_filter: EnvFilter) {
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .finish()
        .init()
}

#[must_use = "this `WorkerGuard` should live until the application shuts down"]
fn try_init_file_logging(log_filter: EnvFilter) -> Option<WorkerGuard> {
    match create_log_dir_path() {
        Ok(log_dir_path) => {
            let file_appender = tracing_appender::rolling::never(log_dir_path, get_log_file_name());
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            init_file_logging(log_filter, non_blocking);
            Some(guard)
        }
        Err(e) => {
            init_console_logging(log_filter);
            warn!("File-based logging failed. Falling back to stdout: {e}");
            None
        }
    }
}

fn init_file_logging(log_filter: EnvFilter, non_blocking: NonBlocking) {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(non_blocking)
        .with_env_filter(log_filter)
        .finish()
        .init();
}

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
