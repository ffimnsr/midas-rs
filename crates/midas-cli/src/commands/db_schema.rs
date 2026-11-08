//! Database schema commands composed under `midas db`.

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};

use crate::commands::Context;

/// Subcommands owned by the schema module.
pub fn subcommands() -> Vec<Command> {
  vec![
    Command::new("pull").about("Introspect the database into the schema DSL"),
    Command::new("push").about("Apply the schema DSL to the database"),
    Command::new("diff").about("Report drift between the schema DSL and the database"),
  ]
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  matches!(name, "pull" | "push" | "diff")
}

/// Dispatch a schema subcommand.
pub fn dispatch(name: &str, _: &ArgMatches, _: &Context) -> Result<()> {
  anyhow::bail!("`db {name}` is not implemented yet")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_schema_commands() {
    for name in ["pull", "push", "diff"] {
      assert!(owns(name));
    }
    assert!(!owns("create"));
  }
}
