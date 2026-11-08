//! # Cargo-Migrate
//! The entry point for `cargo migrate`.

use std::process::ExitCode;

fn main() -> ExitCode {
  midas_cli::cli::main_entry("migrate", true)
}
