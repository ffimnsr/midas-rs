# Midas

[![Crates.io](https://img.shields.io/crates/v/midas?style=flat-square)](https://crates.io/crates/midas)
[![Crates.io](https://img.shields.io/crates/l/midas?style=flat-square)](https://crates.io/crates/midas)
[![Crates.io](https://img.shields.io/crates/d/midas?style=flat-square)](https://crates.io/crates/midas)

> Space is big. You just won't believe how vastly, hugely,
> mind-bogglingly big it is. I mean, you may think it's a
> long way down the road to the chemist's, but that's just
> peanuts to space.
> - from The Hitchhicker's Guide To The Galaxy by Douglas Adams

Do painless migrations.

NOTE: This README is still under construction.

## Supported Database

Currently, the only supported database is `Postgres`.

## Usage

### Using CLI

Here is a sample command line usage of `midas`.

~~~
$ midas --source postgres://postgres@localhost:5432/postgres up
~~~

## Development

### Dependencies

- Rust and Cargo.

## Installation

If you're into **Rust** then you can use `cargo` to install.

* The minimum supported version of Rust is 1.37.0.

~~~
cargo install midas
~~~

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
