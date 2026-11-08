//! Code generation commands (implemented in the codegen phase).

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};

use crate::commands::Context;

/// Register the `generate` command.
pub fn register(app: Command) -> Command {
  app.subcommand(Command::new("generate").about("Generate Rust types from the schema (not implemented yet)"))
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  name == "generate"
}

/// Dispatch the command.
pub fn dispatch(name: &str, _: &ArgMatches, _: &Context) -> Result<()> {
  anyhow::bail!("`{name}` is not implemented yet")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_only_generate() {
    assert!(owns("generate"));
    assert!(!owns("schema"));
  }
}
