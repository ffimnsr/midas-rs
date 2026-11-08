//! # Midas
//! The main entry point for the migration command.

use std::process::ExitCode;

/// The package name.
const PKG_NAME: &str = env!("CARGO_PKG_NAME");

fn main() -> ExitCode {
  midas_cli::cli::main_entry(PKG_NAME, false)
}
