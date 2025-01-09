mod cli;

use anyhow::Result as AnyhowResult;

/// The main entry point for the migration command
fn main() -> AnyhowResult<()> {
  cli::midas_entry("migrate", true)?;
  Ok(())
}
