mod cli;

use anyhow::Result as AnyhowResult;

fn main() -> AnyhowResult<()> {
    cli::midas_entry("migrate", true)?;
    Ok(())
}
