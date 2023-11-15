use std::process::Command;

fn main() {
    // record git commit hash
    {
        let output = Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap();
        let git_commit_hash = String::from_utf8(output.stdout).unwrap();
        println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_commit_hash);
        println!("cargo:rustc-env=CLAP_VERSION={} {}", env!("CARGO_PKG_VERSION"), git_commit_hash);
    }
}
