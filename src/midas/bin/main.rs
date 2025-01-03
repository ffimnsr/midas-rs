mod cli;

use anyhow::Result as AnyhowResult;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

fn main() -> AnyhowResult<()> {
  cli::midas_entry(PKG_NAME, false)?;
  Ok(())
}
