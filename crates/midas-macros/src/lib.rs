//! Procedural macros for `midas`.

#![forbid(unsafe_code)]

use std::str::FromStr;

use proc_macro::TokenStream;

const EXPANSION: &str = "::std::vec::Vec::<::midas::migration::Migration>::new()";

/// Embed migrations at compile time.
///
/// Phase 0 provides a placeholder that expands to an empty migration list. The
/// migration engine phase replaces the expansion with directory discovery.
#[proc_macro]
pub fn migrate(_input: TokenStream) -> TokenStream {
  match TokenStream::from_str(EXPANSION) {
    Ok(stream) => stream,
    Err(error) => {
      // Never fail silently: surface a real compiler error if the built-in
      // expansion ever becomes invalid.
      let message = format!("midas migrate! placeholder expansion failed: {error}");
      TokenStream::from_str(&format!("compile_error!({message:?})")).unwrap_or_default()
    },
  }
}
