pub mod commander;
pub mod lookup;
pub mod sequel;

use std::fs;
use std::path::Path;

use anyhow::{
  Context as _,
  Result as AnyhowResult,
};

use indicatif::ProgressStyle;

/// Ensure the migration state directory exists
pub fn ensure_migration_state_dir_exists() -> AnyhowResult<()> {
  let migration_dir = Path::new(".migrations-state");

  // If the directory does not exist, create it
  // Otherwise, do nothing
  if !migration_dir.exists() {
    fs::create_dir_all(migration_dir).context("Failed to create migrations directory")?;
  }

  Ok(())
}

/// Setup the progress style
pub fn progress_style() -> AnyhowResult<ProgressStyle> {
  let style = ProgressStyle::default_bar()
    .template("{spinner:.green} [{prefix:.bold.dim}] {wide_msg:.cyan/blue} ")?
    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⦿");

  Ok(style)
}
