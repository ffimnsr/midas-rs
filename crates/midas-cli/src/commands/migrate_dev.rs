//! Development diff commands (implemented in the diff engine phase).

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};

use crate::commands::Context;

/// Register the `migrate-dev` command.
pub fn register(app: Command) -> Command {
  app.subcommand(
    Command::new("migrate-dev").about("Generate migrations from schema changes (not implemented yet)"),
  )
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  name == "migrate-dev"
}

/// Dispatch the command.
pub fn dispatch(name: &str, _: &ArgMatches, _: &Context) -> Result<()> {
  anyhow::bail!("`{name}` is not implemented yet")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_only_migrate_dev() {
    assert!(owns("migrate-dev"));
    assert!(!owns("schema"));
  }
}
