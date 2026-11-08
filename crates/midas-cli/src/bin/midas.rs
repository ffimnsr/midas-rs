//! # Midas
//! The main entry point for the migration command.

use anyhow::Result;

/// The package name.
const PKG_NAME: &str = env!("CARGO_PKG_NAME");

fn main() -> Result<()> {
  midas_cli::cli::midas_entry(PKG_NAME, false)
}
