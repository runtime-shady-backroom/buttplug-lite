// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use clap::Parser;

/// struct used to derive Clap arguments
#[derive(Parser)]
#[command(author = "runtime", version, about, long_about = None)]
pub struct CliArgs {
    /// Sets the level of verbosity.
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Log to stdout instead of a log file
    #[arg(short = 'c', long)]
    pub stdout: bool,

    /// Custom logging filter: https://docs.rs/tracing-subscriber/0.3.16/tracing_subscriber/filter/struct.EnvFilter.html. This overrides `--verbose` setting.
    #[arg(short = 'f', long)]
    pub log_filter: Option<String>,

    /// Run self-checks then immediately exit
    #[arg(long)]
    pub self_check: bool,

    /// Emit periodic ApplicationStatusEvent ticks every <SECONDS> seconds
    #[arg(long, id = "SECONDS")]
    pub debug_ticks: Option<u64>,
}
