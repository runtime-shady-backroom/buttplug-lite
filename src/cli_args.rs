// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use clap::Parser;

#[derive(Parser)]
#[clap(author = "runtime", version, about, long_about = None)]
pub struct CliArgs {
    /// Sets the level of verbosity
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,
}
