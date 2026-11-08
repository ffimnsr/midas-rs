//! Schema DSL commands (implemented in the schema DSL phase).

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};

use crate::commands::Context;

/// Register the `schema` command group.
pub fn register(app: Command) -> Command {
  app.subcommand(
    Command::new("schema")
      .about("Validate, format, and generate the schema DSL (not implemented yet)")
      .arg_required_else_help(true)
      .subcommand(Command::new("validate").about("Validate the schema DSL"))
      .subcommand(Command::new("fmt").about("Format the schema DSL"))
      .subcommand(Command::new("generate").about("Generate schema artifacts")),
  )
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  name == "schema"
}

/// Dispatch the command.
pub fn dispatch(name: &str, _: &ArgMatches, _: &Context) -> Result<()> {
  anyhow::bail!("`{name}` is not implemented yet")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_only_schema() {
    assert!(owns("schema"));
    assert!(!owns("generate"));
  }
}
