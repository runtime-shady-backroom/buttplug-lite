// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use clap::Parser;

/// struct used to derive Clap arguments
#[derive(Parser)]
#[command(author = "runtime", version, about, long_about = None)]
pub struct CliArgs {
    /// Sets the level of verbosity. Repeating this argument up to four times will apply increasingly verbose log_filter presets.
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Log to stdout instead of the default log file.
    #[arg(short = 'c', long)]
    pub stdout: bool,

    /// Custom logging filter: https://docs.rs/tracing-subscriber/0.3.16/tracing_subscriber/filter/struct.EnvFilter.html. This completely overrides the `--verbose` setting.
    #[arg(short = 'f', long)]
    pub log_filter: Option<String>,

    /// Run self-checks then immediately exit. This is for internal use and not designed for end users.
    #[arg(long)]
    pub self_check: bool,

    /// Emit periodic ApplicationStatusEvent ticks every <SECONDS> seconds. These "ticks" force the UI to update device state, which for example can be used to poll device battery levels.
    #[arg(long, id = "SECONDS")]
    pub debug_ticks: Option<u64>,

    /// Disables the custom panic handler in the log file. Has no effect if used with `--stdout`.
    #[arg(long)]
    pub no_panic_handler: bool,

    /// Enables the custom panic handler in stdout logs. Has no effect if file logging is used. Note that file logging is the default without an explicit `--stdout`.
    #[arg(long)]
    pub force_panic_handler: bool,
}
