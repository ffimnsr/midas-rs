//! SQLx interop commands (implemented in the SQLx interop phase).

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};

use crate::commands::Context;

/// Register the `sqlx` command group.
pub fn register(app: Command) -> Command {
  app.subcommand(
    Command::new("sqlx")
      .about("SQLx interop commands (not implemented yet)")
      .arg_required_else_help(true)
      .subcommand(Command::new("prepare").about("Prepare SQLx offline metadata"))
      .subcommand(Command::new("import").about("Import SQLx migrations"))
      .subcommand(Command::new("export").about("Export migrations for SQLx")),
  )
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  name == "sqlx"
}

/// Dispatch the command.
pub fn dispatch(name: &str, _: &ArgMatches, _: &Context) -> Result<()> {
  anyhow::bail!("`{name}` is not implemented yet")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_only_sqlx() {
    assert!(owns("sqlx"));
    assert!(!owns("generate"));
  }
}
