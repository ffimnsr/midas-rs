//! # Midas
//!
//! Migration library with pluggable database backends. The [`commander`]
//! module coordinates migration files with a [`sequel::Backend`], while
//! [`config`] and [`error`] define the shared contracts used by every phase.
//!
//! Driver modules are compiled only when their feature is enabled. The
//! following doctests prove the module is absent otherwise.
#![forbid(unsafe_code)]
#![cfg_attr(
  not(feature = "postgres"),
  doc = "```compile_fail\nuse midas::sequel::postgres;\n```"
)]
#![cfg_attr(
  not(feature = "sqlite"),
  doc = "```compile_fail\nuse midas::sequel::sqlite;\n```"
)]

pub mod commander;
pub mod config;
pub mod error;
pub mod migration;
pub mod sequel;

// Allows the `migrate!` placeholder to expand to `::midas::...` paths even
// inside this crate.
extern crate self as midas;

pub use error::{
  Error,
  Result,
};

#[cfg(feature = "macros")]
pub use midas_macros::migrate;

use indicatif::ProgressStyle;

/// Build the shared progress-bar style.
pub fn progress_style() -> Result<ProgressStyle> {
  let style = ProgressStyle::default_bar()
    .template("{spinner:.green} [{prefix:.bold.dim}] {wide_msg:.cyan/blue} ")
    .map_err(|error| Error::migration(format!("invalid progress template: {error}")))?
    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⦿");

  Ok(style)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn progress_style_builds() {
    assert!(progress_style().is_ok());
  }

  #[cfg(feature = "macros")]
  #[test]
  fn migrate_macro_embeds_migrations() {
    let migrations: Vec<crate::migration::Migration> = crate::migrate!("tests/fixtures/embedded/migrations");
    assert_eq!(migrations.len(), 1);
    assert_eq!(migrations[0].version, 1);
    assert!(migrations[0].up.contains("embedded_widgets"));
    assert!(migrations[0].down.is_some());
  }
}
