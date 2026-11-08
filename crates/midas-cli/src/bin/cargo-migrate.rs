//! # Cargo-Migrate
//! The entry point for `cargo migrate`.

use anyhow::Result;

fn main() -> Result<()> {
  midas_cli::cli::midas_entry("migrate", true)
}
