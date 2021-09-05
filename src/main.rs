mod commander;
mod cli;
mod lookup;
mod sequel;

pub(crate) use cli::GenericError;

fn main() -> Result<(), GenericError> {
    cli::midas_entry("migrate", true)?;
    Ok(())
}
