//! # Midas
//! The main entry point for the migration command

mod cli;

use anyhow::Result as AnyhowResult;

/// The package name
const PKG_NAME: &str = env!("CARGO_PKG_NAME");

/// The main entry point for the migration command
fn main() -> AnyhowResult<()> {
  cli::midas_entry(PKG_NAME, false)?;
  Ok(())
}
