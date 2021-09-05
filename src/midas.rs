mod commander;
mod cli;
mod lookup;
mod sequel;

pub(crate) use cli::GenericError;

const PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

fn main() -> Result<(), GenericError> {
    cli::midas_entry(PKG_NAME, false)?;
    Ok(())
}
