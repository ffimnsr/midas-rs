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

- [x] PostgresSQL
- [x] CockroachDB
- [x] MySQL
- [x] MariaDB
- [x] SQLite3
- [ ] MSSQL
- [ ] Cassandra
- [ ] OracleDB

## Usage

### Using CLI

Here is a sample command line usage of `midas`.

```shell
# PostgresSQL
midas --database 'postgres://postgres:postgres@localhost:5432/startup' --source migrations up

# MySQL/MariaDB
midas --database 'mysql://root:mysql@localhost:3306/startup' --source migrations up

# SQLite
midas --database './data.db3' --source migrations up
```

or you could also use the `cargo migrate` to integrate it on your cargo workflow.

> **NOTE:** For SQLite use correct URI filenames as stated here: https://www.sqlite.org/c3ref/open.html#urifilenameexamples

### Using on container

Here is a basic setup:

```bash
MIGRATION_DIR=$PWD/migrations
mkdir -p $MIGRATION_DIR
docker run --rm -v $MIGRATION_DIR:/app/migrations ghcr.io/ffimnsr/midas-rs:latest --database 'postgres://postgres:postgres@localhost:5432/startup' --source migrations status
```

You can omit the `--source migrations` as the source flag would default to migrations. In case you plan to change the mounted volume migrations path then append the `--source <path/to/migrations>` argument.

### Command arguments

The command will execute all **special** (up) SQL migrations files to the database. \
Here are the available subcommands:

```shell
create    Creates a timestamped migration file
down      Remove all applied migrations
drop      Drops everything inside the database (NOTE: must have create/drop privilege)
redo      Redo the last migration
revert    Reverts the last migration
init      Setups and creates initial file directory and env
status    Checks the status of the migration
up        Apply all non-applied migrations
faker     Generate fake data for the database (WIP)
setup     Setup the database (WIP)
```

For more info see `--help`.

## Installation

The binary name for midas are `midas` and `cargo-migrate`.

Binary for different OS distribution can be downloaded [here](https://github.com/ffimnsr/midas/releases). Linux, macOS, and Windows are supported.

### Install using script

`midas` runs on most major platforms. If your platform isn't listed below, please [open an issue](https://github.com/ffimnsr/midas-rs/issues/new).

<details>
  <summary>Linux / WSL / MSYS2 / Cygwin / Git Bash</summary>

  > The recommended way to install midas is via the install script:
  >
  >
  > ```sh
  > curl -sSfL https://raw.githubusercontent.com/ffimnsr/midas-rs/main/install.sh | sh
  > ```
</details>

<details>
  <summary>BSD / Android</summary>

  > The recommended way to install midas is via the install script:
  >
  >
  > ```sh
  > curl -sS https://raw.githubusercontent.com/ffimnsr/midas-rs/main/install.sh | bash
  > ```
</details>

### From source

If you're into **Rust**, then midas can be installed with `cargo`. The minimum supported version of Rust is `1.37.0`. The binaries produce may be bigger than expected as it contains debug symbols.

```bash
cargo install midas
```

### For containers

Docker / Podman installation:

```bash
# Docker
docker pull ghcr.io/ffimnsr/midas-rs:latest

# Podman
podman pull ghcr.io/ffimnsr/midas-rs:latest
```

### For package managers

If you're a Debian user (or a user of a Debian derivative like Ubuntu), then midas can be installed using a binary .deb file provided in each midas release.

```bash
curl -LO https://github.com/ffimnsr/midas-rs/releases/download/0.6.6/midas_0.6.6-1_amd64.deb
sudo dpkg -i midas_0.6.6-1_amd64.deb
```

### Manual installation

Follow the instruction below to install and use `midas` on your system.

1. Download the binary for your OS distribution [here](https://github.com/ffimnsr/midas/releases).
2. Copy it to your system binary directory (`/usr/local/bin`) or to your userspace binary directory (`$HOME/.local/bin`).

## What's in the Roadmap

- [ ] TOML DSL for creating database objects
- [ ] Add functionality for `setup` and `faker` commands.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
