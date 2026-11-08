//! Database lifecycle commands composed under `midas db`.

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};

use crate::commands::Context;

/// Subcommands owned by the lifecycle module.
pub fn subcommands() -> Vec<Command> {
  vec![
    Command::new("create").about("Create the configured database"),
    Command::new("drop").about("Drop the configured database"),
    Command::new("reset").about("Drop and recreate the configured database"),
    Command::new("exists").about("Check whether the configured database exists"),
  ]
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  matches!(name, "create" | "drop" | "reset" | "exists")
}

/// Dispatch a lifecycle subcommand.
pub fn dispatch(name: &str, _: &ArgMatches, _: &Context) -> Result<()> {
  anyhow::bail!("`db {name}` is not implemented yet")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_lifecycle_commands() {
    for name in ["create", "drop", "reset", "exists"] {
      assert!(owns(name));
    }
    assert!(!owns("pull"));
  }
}
