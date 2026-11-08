//! Procedural macros for `midas`.

use std::str::FromStr;

use proc_macro::TokenStream;

const EXPANSION: &str = "::std::vec::Vec::<::midas::migration::Migration>::new()";

/// Embed migrations at compile time.
///
/// Phase 0 provides a placeholder that expands to an empty migration list. The
/// migration engine phase replaces the expansion with directory discovery.
#[proc_macro]
pub fn migrate(_input: TokenStream) -> TokenStream {
  TokenStream::from_str(EXPANSION).unwrap_or_else(|_| TokenStream::new())
}
