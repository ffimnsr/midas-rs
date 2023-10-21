# Midas (cargo-migrate)

[![Crates.io Package](https://img.shields.io/crates/v/midas?style=flat-square)](https://crates.io/crates/midas)
[![Crates.io Downloads](https://img.shields.io/crates/d/midas?style=flat-square)](https://crates.io/crates/midas)
[![License](https://img.shields.io/crates/l/midas?style=flat-square)](https://github.com/ffimnsr/midas-rs/blob/master/LICENSE-APACHE)
[![Github Workflow Status](https://img.shields.io/github/actions/workflow/status/ffimnsr/midas-rs/ci.yaml?style=flat-square)](https://github.com/ffimnsr/midas-rs/blob/master/.github/workflows/ci.yaml)


> So Midas, king of Lydia, swelled at first with pride
> when he found he could transform everything he touched
> to gold; but when he beheld his food grow rigid and his
> drink harden into golden ice then he understood that
> this gift was a bane and in his loathing for gold, cursed
> his prayer.
> - from In Rufinem, Claudian

Do painless migrations.

## Supported Database

Currently, the only supported database is `Postgres`.

## Usage

### Using CLI

Here is a sample command line usage of `midas`.

```shell
midas --database postgres://postgres@localhost:5432/postgres --source migrations up
```

or you could also use the `cargo migrate` to integrate it on your cargo workflow.

The command will execute all **special** (up) SQL migrations files to the database. \
Here are the available subcommands:

```shell
create    Creates a timestamped migration file
down      Remove all applied migrations
drop      Drops everything inside the database
redo      Redo the last migration
revert    Reverts the last migration
init      Setups and creates initial file directory and env
status    Checks the status of the migration
up        Apply all non-applied migrations
```

For more info see `--help`.

## Installation

If you're into **Rust** then you can use `cargo` to install.

* The minimum supported version of Rust is `1.37.0`.

```shellbash
cargo install midas
```

Binary for different OS distribution can be downloaded [here](https://github.com/ffimnsr/midas/releases).

## What's in the Roadmap

- [ ] TOML DSL for creating database objects
- [ ] Setup multiple drivers (e.g. MySQL, Sqlite3, etc.)
- [ ] Add functionality for `setup` and `drop` commands.
- [ ] More to come.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
