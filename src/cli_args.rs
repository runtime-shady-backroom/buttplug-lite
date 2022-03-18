use clap::Parser;

#[derive(Parser)]
#[clap(author = "runtime", version, about, long_about = None)]
pub struct CliArgs {
    /// Sets the level of verbosity
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,
}
