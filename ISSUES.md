# ISSUES

> Implementation backlog for [docs/roadmap.md](docs/roadmap.md) and
> [docs/dsl.md](docs/dsl.md).
>
> This file contains only automated, testable implementation work. Manual tasks
> (publishing releases, writing prose docs, manual reviews) are intentionally
> excluded. Non-actionable context is written as plain bullets, not checkboxes.

## How to use this file

- **Phase 0 is the only shared prerequisite.** Freeze its interfaces before starting
  other phases.
- After Phase 0, **Phase 1 and Phase 3 can run in parallel**.
- Phase 2 unlocks after Phase 1. Phase 4 unlocks after Phase 3. Phase 5 unlocks
  after Phase 3 and Phase 4. Phase 6 unlocks after Phase 3. Phase 7 can start after
  Phase 0 and grow as features land.
- Each phase lists a **write scope**. Do not edit files outside it without
  coordinating with the owner of the other phase.
- Every checked item must land with a test: unit, integration, snapshot, compile
  fixture, or CI matrix entry.
- Run `cargo test --quiet` before marking an item done, per repository rules.
- Acceptance criteria are plain bullets, not checkboxes. They gate the phase.
- Recommended decisions are already made. Do not reopen them in implementation PRs.

## Locked decisions

These are settled. Implement them as written.

- Schema authoring format: TOML.
- Schema runtime: synchronous library; no async query API.
- Migration checksum: SHA-384 over canonicalized UP/DOWN SQL.
- Metadata shape: SQLx-compatible columns.
- SQLx target: 0.9.x, file/metadata interop only; no SQLx production dependency.
- Workspace: `midas` library, `midas-cli` binaries, `midas-macros` proc macros.
- Database scope: Postgres and Postgres-wire compatible servers (CockroachDB and
  managed Postgres), plus SQLite. MySQL and MariaDB are out of scope.
- MSRV: 1.99.
- TLS: disabled by default; `native-tls` is the default backend when enabled;
  `tls-rustls` and `openssl-vendored` are optional.
- Previous schema state: `.midas/state.json`.
- Canonical migration markers: `-- !UP` and `-- !DOWN`.
- Canonical metadata tables: Postgres `midas.__schema_migrations`, SQLite
  `__schema_migrations`; SQLx adoption uses `_sqlx_migrations`.
- Diff source: previous schema state for offline diff; shadow DB replay for drift.
- Rename detection: explicit `was` / `previous_name` only.
- Unsupported Postgres features: `[[raw]]` only.

## Phase map

```text
Phase 0  Foundation & Shared Interfaces
   ├── Phase 1  Migration Engine
   │      └── Phase 2  SQLx Interop
   ├── Phase 3  Schema DSL Core
   │      ├── Phase 4  Diff Engine
   │      │      └── Phase 5  Introspection & Shadow DB
   │      └── Phase 6  Codegen & Offline Metadata
   └── Phase 7  Hardening, CI & Release Automation
```

| Phase | Name | Depends on | Primary write scope |
| --- | --- | --- | --- |
| 0 | Foundation & Shared Interfaces | none | workspace, `crates/midas/src/{lib,error,config}.rs`, `crates/midas/src/sequel/mod.rs`, `crates/midas-cli/src/commands/mod.rs` |
| 1 | Migration Engine | 0 | `crates/midas/src/{lookup,commander,migration}`, `crates/midas/src/sequel/*`, `crates/midas-cli/src/commands/{migrate,db_lifecycle}.rs` |
| 2 | SQLx Interop | 1 | `crates/midas/src/sqlx/*`, `crates/midas-cli/src/commands/sqlx.rs`, conformance fixture |
| 3 | Schema DSL Core | 0 | `crates/midas/src/schema/{ast,ir,types,expr,parse,validate,naming,emit/postgres}`, `crates/midas-cli/src/commands/schema.rs` |
| 4 | Diff Engine | 3 | `crates/midas/src/schema/{diff,plan,emit/alter}`, `crates/midas-cli/src/commands/migrate_dev.rs` |
| 5 | Introspection & Shadow DB | 3, 4 | `crates/midas/src/schema/introspect/*`, `crates/midas-cli/src/commands/db_schema.rs` |
| 6 | Codegen & Offline Metadata | 3 | `crates/midas/src/codegen/*`, `crates/midas/src/sqlx/offline.rs`, `crates/midas-cli/src/commands/generate.rs` |
| 7 | Hardening, CI & Release Automation | 0 | `.github/workflows/*`, `ci/*`, `deny.toml`, integration tests, container/deb smoke tests |

---

# Phase 0 — Foundation & Shared Interfaces

## Goals and overview

Create a workspace, freeze public error/config/driver interfaces, introduce driver
feature flags, remove dead or panic-prone code paths, and establish the test harness
that every later phase uses. This phase is deliberately small and must land first.

Roadmap reference: WS1, M0; audit sections 9.4 and 9.8.

## Rules

- Use Rust edition 2021 and set workspace MSRV to `1.99`.
- Use `thiserror` for public errors. `anyhow` is allowed only in binaries and tests.
- Use `sha2` for SHA-384 migration checksums.
- Use `crc` for the SQLx-compatible advisory-lock key.
- Use `toml` 0.8 for the schema DSL.
- Use `snapbox` / `trycmd` for snapshots, `assert_cmd` / `assert_fs` for CLI tests,
  `testcontainers` for Postgres integration tests, and temp files for SQLite.
- TLS is disabled by default. When TLS is enabled, `native-tls` is the default
  backend; `rustls` is available only behind the non-default `tls-rustls` feature.
  Keep `openssl-vendored` as a non-default feature for static builds.
- Workspace crates: `midas` (library), `midas-cli` (binaries), `midas-macros`
  (proc macros).
- Rename library imports from `midas_core` to `midas`.
- Keep canonical migration format: one `.sql` file with `-- !UP` and `-- !DOWN`.
- Do not add an async runtime to production dependencies.
- Do not expose `anyhow` types in `midas` public APIs.
- Do not use `unwrap()`, `expect()`, or `panic!()` in non-test code.
- Every CLI command module must be registered through `crates/midas-cli/src/commands/mod.rs`
  so phases can own disjoint files.
- Every new public item requires a test.

## Work items

### 0.1 Convert to a Cargo workspace

- [x] Create root `Cargo.toml` with `[workspace]`, `[workspace.package]`, and
      `[workspace.dependencies]`.
  - [x] Set `members = ["crates/midas", "crates/midas-cli", "crates/midas-macros"]`.
  - [x] Set `edition = "2021"` and `rust-version = "1.99"`.
  - [x] Move shared dependency versions into `[workspace.dependencies]`.
- [x] Create `crates/midas/` and move:
  - [x] `src/lib.rs` -> `crates/midas/src/lib.rs`
  - [x] `src/commander.rs` -> `crates/midas/src/commander.rs`
  - [x] `src/lookup.rs` -> `crates/midas/src/lookup.rs`
  - [x] `src/sequel/` -> `crates/midas/src/sequel/`
- [x] Create `crates/midas-cli/` and move:
  - [x] `src/midas/bin/main.rs` -> `crates/midas-cli/src/bin/midas.rs`
  - [x] `src/midas/bin/migrate_main.rs` -> `crates/midas-cli/src/bin/cargo-migrate.rs`
  - [x] `src/midas/bin/cli.rs` -> `crates/midas-cli/src/cli.rs`
- [x] Create `crates/midas-macros/` with `proc-macro = true`.
  - [x] Add `src/lib.rs` exporting a placeholder `migrate!` macro that expands to
        an empty `Vec<midas::migration::Migration>`.
- [x] Update all `use midas_core::` imports to `use midas::`.
- [x] Update `Cargo.toml` binary paths and `default-run`.
- [x] Update `Containerfile`, `Containerfile.distroless`, `install.sh`, `tasks.yaml`,
      and `.github/workflows/ci.yaml` paths.
- [x] Add `crates/midas-cli/src/commands/mod.rs` with submodules:
      `migrate`, `migrate_dev`, `schema`, `db_lifecycle`, `db_schema`, `generate`,
      `sqlx`.
  - [x] Add empty command enums/functions for each so later phases edit only their
        own file.
  - [x] Compose `db_lifecycle` and `db_schema` under the top-level `db` subcommand.
- [x] Add a CI check `cargo check --workspace --all-targets --all-features`.
- [x] Add a CI check `cargo metadata --no-deps` and assert the three package names.

### 0.2 Typed error contract

- [x] Add `crates/midas/src/error.rs`.
  - [x] Define `pub enum Error` with variants:
        `Config`, `Io`, `Database`, `Migration`, `Checksum`, `Lock`, `Schema`,
        `Drift`, `Codegen`, `Unsupported`.
  - [x] Add `pub type Result<T> = core::result::Result<T, Error>`.
  - [x] Derive `thiserror::Error` and implement `source()` for wrapped errors.
  - [x] Add `impl From<std::io::Error> for Error`.
  - [x] Add `impl From<toml::de::Error> for Error`.
  - [x] Add `impl From<postgres::Error>` and `From<rusqlite::Error>` behind the
        corresponding feature flags.
- [x] Add user-facing variants:
  - [x] `MigrationNotFound { version: i64 }`
  - [x] `ChecksumMismatch { version: i64, expected: String, actual: String }`
  - [x] `DirtyMigration { version: i64 }`
  - [x] `LockTimeout { backend: &'static str }`
  - [x] `UnsupportedFeature { dialect: &'static str, feature: &'static str }`
- [x] Replace `anyhow::Result` in `crates/midas/src/**` with `midas::Result`.
  - [x] Keep `anyhow` in `crates/midas-cli/src/**` and tests only.
  - [x] Add `anyhow::Context` at the CLI boundary.
- [x] Add unit tests asserting `Display` output and `source()` for each variant.
- [x] Add a CI grep check that `crates/midas/src` does not contain `anyhow::Result`.

### 0.3 Config contract

- [x] Add `crates/midas/src/config.rs`.
  - [x] Define `pub struct Config` with:
        `database_url`, `migrations_dir`, `metadata_schema: Option<String>`,
        `metadata_table`, `shadow_database_url`, `lock_timeout`,
        `statement_timeout`, `transaction: TransactionMode`,
        `tls: TlsMode`, `application_name`, `search_path`.
  - [x] Define `pub enum TransactionMode { PerMigration, None }`.
  - [x] Define `pub enum TlsMode { Disable, Prefer, Require }`.
  - [x] Implement `Config::from_env()` with precedence:
        CLI flags > environment > `midas.toml` > defaults.
  - [x] Implement `Config::from_file(path: &Path)` using `serde` and
        `#[serde(deny_unknown_fields)]`.
  - [x] Implement `Config::redacted_database_url()`.
  - [x] Implement `Config::validate()` for required fields and URL schemes.
- [x] Add `midas.toml` serde structs:
  - [x] `[project] dialect`, `default_schema`, `schema`, `schema_dir`.
  - [x] `[database] url_env`, `metadata_schema`, `metadata_table`, `shadow_url_env`.
  - [x] `[migrations] dir`, `lock_timeout_ms`, `statement_timeout_ms`, `transaction`.
  - [x] `[managed] schemas`, `ignore_objects`.
- [x] Add tests:
  - [x] Precedence test using injected env maps, not process-global env mutation.
  - [x] Unknown field rejection test with exact line number.
  - [x] Password redaction test for `postgres://user:secret@host/db`.
  - [x] Invalid scheme rejection test.

### 0.4 Driver feature flags

- [x] Add to `crates/midas/Cargo.toml`:
  - [x] `default = ["postgres", "sqlite"]`
  - [x] `postgres = ["dep:postgres"]`
  - [x] `sqlite = ["dep:rusqlite"]`
  - [x] `macros = ["dep:midas-macros"]`
  - [x] `tls-native-tls = ["dep:postgres-native-tls", "dep:native-tls"]`
  - [x] `tls-rustls = ["dep:postgres-rustls"]`
  - [x] `openssl-vendored = ["tls-native-tls", "native-tls/vendored"]`
- [x] Add optional TLS dependencies:
  - [x] `postgres-native-tls = { version = "0.5.3", optional = true }`.
  - [x] `native-tls = { version = "0.2", optional = true }`.
  - [x] `postgres-rustls = { version = "0.1.4", optional = true }`.
- [x] Make `openssl` and `rand` optional or remove them.
- [x] Add a test that `tls-rustls` is not enabled by default.
- [x] Add CI matrix entries:
  - [x] `--no-default-features`
  - [x] `--no-default-features --features postgres`
  - [x] `--no-default-features --features sqlite`
  - [x] `--all-features`
- [x] Add a compile-fail or `cfg` test that a driver module is absent when its
      feature is off.
- [x] Add `required-features` to binaries so `midas` builds with the expected set.
- [x] Remove the MySQL driver, feature, dependency, and tests; `mysql://` URLs now
      fail with `Error::Config`.
- [x] Add regression tests that the `mysql` feature, dependency, and module stay
      absent.
- [x] Accept `cockroach://` and `cockroachdb://` URL aliases, normalize them to
      the Postgres wire scheme, and reject MySQL/MariaDB URLs.
- [x] Add `sequel::DatabaseEngine` as the single source of truth for URL-scheme to
      engine resolution; configuration, driver selection, the CLI, and the runner
      now share it.

### 0.5 Split driver traits

- [x] Add `crates/midas/src/sequel/executor.rs`.
  - [x] Define `pub trait Executor` with `execute`, `execute_batch`,
        `begin`, `commit`, `rollback`.
- [x] Add `crates/midas/src/sequel/store.rs`.
  - [x] Define `pub trait MigrationStore` with `ensure_table`, `list_applied`,
        `get_last`, `insert`, `delete`, `mark_failed`, `repair_checksum`.
  - [x] Define `pub trait Locker` with `lock`, `unlock`, `lock_timeout`.
- [x] Add `crates/midas/src/sequel/introspect.rs`.
  - [x] Define `pub trait Introspector` with `introspect_schema`, `list_schemas`.
- [x] Add `crates/midas/src/sequel/dialect.rs`.
  - [x] Define `pub trait Dialect` with `quote_ident`, `quote_literal`,
        `emit_literal`, `supports(feature)`.
- [x] Add `crates/midas/src/sequel/admin.rs`.
  - [x] Define `pub trait DatabaseAdmin` with `create_database`, `drop_database`,
        `database_exists`, `force_drop_database`.
- [x] Remove the old `Driver` trait and update all call sites.
- [x] Add `crates/midas/src/sequel/memory.rs` with `MemoryStore` and `MemoryExecutor`
      for unit tests.
- [x] Add a unit test that `Migrator` applies a migration against `MemoryStore`.
- [x] Add object-safety compile tests for all traits.

### 0.6 Remove dead or unsafe CLI behavior

- [x] Remove `ensure_migration_state_dir_exists` and `.migrations-state` creation.
  - [x] Update `init` and CLI tests.
  - [x] Fix `.gitignore` entry from `.migration-state` to `.migrations-state` or
        remove it entirely.
- [x] Remove `MIGRATIONS_SKIP_LAST` branches from `down` and `revert`.
- [x] Remove `rand` usage and the artificial `thread::sleep` calls in
      `up`, `upto`, `down`, `revert`.
- [x] Remove `faker` and `update` subcommands from `build_cli`.
- [x] Remove `seed` from `build_cli` until a later phase implements it.
- [x] Remove the implicit `file://./data.db3` default. Return
      `Error::Config` when no database URL is provided for commands that need one.
- [x] Change `init` to:
  - [x] Create `migrations/`.
  - [x] Write `midas.toml` and `schema.toml` only if absent.
  - [x] Never overwrite existing files.
  - [x] Never create a SQLite database.
- [x] Add CLI tests:
  - [x] `init` in an empty temp dir creates only `midas.toml`, `schema.toml`,
        `migrations/`.
  - [x] Second `init` fails without `--force` and leaves files unchanged.
  - [x] `status` without a database URL exits non-zero with a config error.
- [x] Add a `--no-color` flag and honor `NO_COLOR`.

### 0.7 Dependency and maintenance cleanup

- [x] Replace `dotenv` with `dotenvy`.
- [x] Upgrade `tracing-subscriber` from `0.2` to `0.3`.
- [x] Replace direct `log` macro usage with `tracing` macros.
- [x] Add `cargo-deny` configuration at `deny.toml`.
  - [x] Allow MIT, Apache-2.0, BSD-3-Clause, ISC, Unicode-3.0.
  - [x] Deny unknown licenses and wildcard versions.
- [x] Add CI jobs:
  - [x] `cargo deny check licenses bans sources`.
  - [x] `cargo audit` with a pinned `cargo-audit` version.
- [x] Remove `rand` from dependencies if no other use remains.
- [x] Add a dependency inventory test that fails if `openssl` is a default feature.
- [x] Verify `mk_ext_prettytable` and `mk_ext_serde_yaml` still build; if they fail,
      replace `mk_ext_prettytable` with `prettytable-rs` and remove
      `mk_ext_serde_yaml` (YAML is not used in the plan).

### 0.8 Test harness

- [x] Add `crates/midas/tests/common/mod.rs`.
  - [x] `pub fn unique_name(prefix: &str) -> String` using pid + atomic counter.
  - [x] `pub struct TempDir` wrapper that removes itself.
  - [x] `pub fn run_sqlite() -> TempDb`.
  - [x] `pub fn run_postgres() -> TempDb` behind the `postgres` feature.
  - [x] `TempDb` RAII guard drops the database on `Drop`.
- [x] Add `crates/midas-cli/tests/common/mod.rs`.
  - [x] `run_midas(args, cwd) -> assert_cmd::assert::Assert`.
  - [x] `write_migration(dir, name, up, down)`.
- [x] Add a smoke test that starts Postgres with `testcontainers`, creates a
      database, and drops it.
- [x] Add `cargo test --quiet` to the phase checklist.

## Acceptance criteria

- AC-0.1 `cargo test --workspace --all-features --quiet` passes.
- AC-0.2 `cargo check` passes for `--no-default-features` and each driver feature alone.
- AC-0.3 `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- AC-0.4 `crates/midas/src` contains no `anyhow::Result` and no non-test `unwrap()`,
      `expect()`, or `panic!()`.
- AC-0.5 `init` creates only `midas.toml`, `schema.toml`, and `migrations/`, and a
      second run fails without `--force`.
- AC-0.6 CI runs MSRV 1.99, the feature matrix, `cargo deny`, and `cargo audit`.
- AC-0.7 The Postgres and SQLite test harness starts and cleans up in CI.
- AC-0.8 No default feature enables `openssl-vendored` or `rand`.

---

# Phase 1 — Migration Engine

## Goals and overview

Make the migration runner a strict superset of SQLx's migration runner: exact SQL
preservation, SHA-384 checksums, SQLx-compatible metadata, advisory locking,
transactional DDL, dirty recovery, reversible/one-way files, and embedded migrations.
Phase 2 builds on the metadata and file formats defined here.

Roadmap reference: WS2, M1; audit sections 9.1–9.3 and 9.5.

## Rules

- Preserve raw UP/DOWN SQL exactly as written. Normalize only for checksum
  computation, never for execution.
- Checksums are SHA-384, matching SQLx.
- Metadata columns mirror SQLx: `version`, `description`, `installed_on`, `success`,
  `checksum`, `execution_time`.
- `execution_time` is stored in nanoseconds, matching SQLx.
- Locking is enabled by default. `--no-lock` is required for CockroachDB.
- Each migration and its metadata insert run in one transaction where the dialect
  supports transactional DDL.
- Honor `-- no-transaction` at the top of a migration file.
- A failed migration must leave a dirty row (`success = false`) and refuse to rerun
  until `resolve`.
- Do not split SQL manually. Use driver-native batch execution.
- Never discard blank lines, CRLF information inside string literals, or trailing
  content.
- All migration commands support `--dry-run`; `--dry-run` must not execute SQL.
- Define and test exit codes: `0` success, `1` user/config error, `2` database error,
      `3` dirty state, `4` drift.

## Work items

### 1.1 Raw migration model

- [ ] Add `crates/midas/src/migration/mod.rs` and `migration/file.rs`.
- [ ] Define `pub struct Migration`:
  - [ ] `version: i64`
  - [ ] `description: String`
  - [ ] `up: String`
  - [ ] `down: Option<String>`
  - [ ] `no_tx: bool`
  - [ ] `checksum_up: [u8; 48]`
  - [ ] `checksum_down: Option<[u8; 48]>`
  - [ ] `source_path: PathBuf`
  - [ ] `format: MigrationFormat`
- [ ] Define `pub enum MigrationFormat { Midas, SqlxUpDown, SqlxOneWay }`.
- [ ] Add `migration::parse_bytes(path, bytes) -> Result<Migration>`.
  - [ ] Strip a UTF-8 BOM if present.
  - [ ] Decode UTF-8; return `Error::Migration` with path and byte offset on failure.
  - [ ] Split on exact lines `-- !UP` and `-- !DOWN`.
  - [ ] Preserve every line, including blank lines and trailing newline.
  - [ ] Allow a missing `-- !DOWN` section and set `down = None`.
  - [ ] Detect `-- no-transaction` at the beginning of the file or immediately after
        the UP marker.
  - [ ] Normalize `\r\n` and `\r` to `\n` for checksum input only.
  - [ ] Compute SHA-384 over the normalized UP and DOWN strings.
- [ ] Add unit tests with inline fixtures:
  - [ ] Blank line between two statements is preserved in `up`.
  - [ ] Dollar-quoted function body is byte-identical in `up`.
  - [ ] CRLF input yields the same checksum as LF input.
  - [ ] UTF-8 BOM input yields the same checksum as no-BOM input.
  - [ ] Missing DOWN yields `down = None` and does not error.
  - [ ] `-- no-transaction` sets `no_tx = true`.
  - [ ] Invalid UTF-8 returns a path-bearing error.

### 1.2 Filename parsing and creation

- [ ] Replace the regex in `lookup.rs` with a parser for:
  - [ ] `<version>_<description>.sql`
  - [ ] `<version>_<description>.up.sql`
  - [ ] `<version>_<description>.down.sql`
- [ ] Parse `version` as `i64`; reject zero and negative values.
- [ ] Parse `description` from the filename; permit `[A-Za-z0-9._-]+`.
- [ ] Add table-driven tests:
  - [ ] `1_init.sql` -> version 1.
  - [ ] `20240101000000_init.sql` -> version `20240101000000`.
  - [ ] `1_add-users.sql` -> description `add-users`.
  - [ ] `1_add.users.sql` -> description `add.users`.
  - [ ] `0_init.sql` -> error.
  - [ ] `-1_init.sql` -> error.
  - [ ] `schema.sql` -> error.
  - [ ] Duplicate version across files -> error.
- [ ] Update `create_migration_file`:
  - [ ] Trim and lowercase the slug.
  - [ ] Replace whitespace with `_`.
  - [ ] Reject path separators, `..`, and characters outside `[a-z0-9._-]`.
  - [ ] Generate a millisecond timestamp version; if it collides, increment until
        unused.
  - [ ] Default format: one `.sql` file with `-- !UP` and `-- !DOWN`.
  - [ ] `--reversible` creates `.up.sql` and `.down.sql`.
- [ ] Add tests:
  - [ ] `create "Add users table"` creates `*_add_users_table.sql`.
  - [ ] `create "../../etc/passwd"` exits non-zero and writes nothing.
  - [ ] Two `create` calls in the same millisecond produce distinct versions.
- [ ] Add `.gitattributes` at repository root:
  - [ ] `*.sql text eol=lf`
- [ ] Add a checksum test that reads a CRLF fixture and asserts it equals the LF
      checksum.

### 1.3 Metadata v2

- [ ] Add `crates/midas/src/migration/record.rs`.
  - [ ] Define `pub struct MigrationRecord` with the six SQLx columns plus
        `down_checksum: Option<[u8; 48]>`.
- [ ] Add exact DDL per dialect:
  - [ ] Postgres:
        `version BIGINT PRIMARY KEY`,
        `description TEXT NOT NULL`,
        `installed_on TIMESTAMPTZ NOT NULL DEFAULT now()`,
        `success BOOLEAN NOT NULL`,
        `checksum BYTEA NOT NULL`,
        `execution_time BIGINT NOT NULL`,
        `down_checksum BYTEA`.
  - [ ] SQLite:
        `version INTEGER PRIMARY KEY`,
        `description TEXT NOT NULL`,
        `installed_on TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))`,
        `success INTEGER NOT NULL`,
        `checksum BLOB NOT NULL`,
        `execution_time INTEGER NOT NULL`,
        `down_checksum BLOB`.
- [ ] Implement automatic upgrade from the current one-column tables:
  - [ ] Detect existing columns with `information_schema.columns` on Postgres
        and `PRAGMA table_info` on SQLite.
  - [ ] Add missing columns with `ALTER TABLE`.
  - [ ] Backfill `description = ''`, `success = TRUE`,
        `checksum = <48 zero bytes>`, `execution_time = -1`.
  - [ ] Use dialect-correct zero literals: Postgres `decode(repeat('00', 48), 'hex')`,
        SQLite `zeroblob(48)`.
  - [ ] Preserve all existing `migration`/`version` values.
  - [ ] Treat an all-zero checksum as a legacy/unverified row and skip checksum
        comparison until `migrate repair` populates it.
- [ ] Add `Config.metadata_schema` and `Config.metadata_table` support.
  - [ ] Postgres default: schema `midas`, table `__schema_migrations`.
  - [ ] SQLite default: no schema, table `__schema_migrations`.
  - [ ] Test adopting `_sqlx_migrations` by setting both fields.
- [ ] Add integration tests:
  - [ ] Fresh database creates the v2 table.
  - [ ] v1 table with two rows upgrades in place and preserves both rows.
  - [ ] Custom metadata table name is used.

### 1.4 Advisory locking

- [ ] Add `crates/midas/src/migration/lock.rs`.
  - [ ] Define `pub struct LockGuard<'a>` that unlocks on `Drop`.
- [ ] Postgres:
  - [ ] Compute the key as
        `crc32(current_database) as i64 * 0x3d32ad9e`, matching SQLx.
  - [ ] Execute `SELECT pg_advisory_lock($1)`.
  - [ ] Execute `SELECT pg_advisory_unlock($1)` on drop.
  - [ ] Add a unit test with a known database name and expected key.
- [ ] CockroachDB: report `Feature::AdvisoryLocks = false` (already done in Phase 0)
      and require `--no-lock`.
- [ ] SQLite:
  - [ ] Acquire `BEGIN IMMEDIATE` for the migration transaction.
  - [ ] For `-- no-transaction`, use a short `BEGIN IMMEDIATE` only for the metadata
        update and document that DDL still commits immediately in SQLite.
- [ ] Add `--no-lock` and `Config.lock_timeout`.
- [ ] Add tests:
  - [ ] Two concurrent `run` processes apply each migration exactly once.
  - [ ] A second process times out when the first holds the lock.
  - [ ] `LockGuard` releases the lock when dropped after a panic (catch with
        `std::panic::catch_unwind` in the test).
  - [ ] `--no-lock` skips locking and still applies migrations.

### 1.5 Transactions, dirty state, and recovery

- [ ] Implement `MigrationStore::apply`:
  - [ ] Begin a transaction.
  - [ ] Execute the UP script with `Executor::execute_batch`.
  - [ ] Insert the metadata row with `success = true` in the same transaction.
  - [ ] Commit.
- [ ] Implement `-- no-transaction`:
  - [ ] Execute the script outside a transaction.
  - [ ] Insert the metadata row with `success = true` after success.
  - [ ] On failure, insert or update `success = false`.
- [ ] Implement `dirty_version()`:
  - [ ] Return the lowest `version` where `success = false`.
  - [ ] Refuse to run any migration while dirty.
- [ ] Implement `migrate resolve`:
  - [ ] `--applied` marks the dirty version as `success = true`.
  - [ ] `--rolled-back` deletes the dirty row.
- [ ] Implement `migrate repair`:
  - [ ] Recompute `checksum` and `description` for applied versions.
- [ ] Add integration tests:
  - [ ] A failing Postgres migration rolls back DDL and leaves a dirty row.
  - [ ] A failing SQLite migration rolls back DDL and leaves a dirty row.
  - [ ] `resolve --applied` allows the next migration to run.
  - [ ] `resolve --rolled-back` allows the same migration to rerun.
  - [ ] `repair` updates a changed checksum.

### 1.6 Runner commands

- [ ] Add `crates/midas-cli/src/commands/migrate.rs` with subcommands:
  - [ ] `run` (alias `up`), with `--target <version>`, `--dry-run`, `--json`.
  - [ ] `info` (alias `status`), with `--json`.
  - [ ] `add` (alias `create`), with `--reversible`.
  - [ ] `revert --to <version>`.
  - [ ] `baseline --to <version>`.
  - [ ] `repair`.
  - [ ] `resolve --applied|--rolled-back`.
  - [ ] `reset`.
  - [ ] `down` (all) with `--force`.
  - [ ] `redo`.
- [ ] Implement exit code mapping in the CLI.
- [ ] Add `--json` output schema:
  - [ ] `{ "version": 1, "applied": [...], "pending": [...], "dirty": null }`.
  - [ ] Add a snapshot test for the JSON shape.
- [ ] Add tests:
  - [ ] `run --target 2` applies only versions 1 and 2.
  - [ ] `revert --to 1` reverts versions above 1 in reverse order.
  - [ ] `baseline --to 5` marks 1..5 applied without executing SQL.
  - [ ] `reset` drops the metadata table and replays all migrations.
  - [ ] `--dry-run` prints SQL and leaves the database unchanged.
  - [ ] `info --json` matches the snapshot.

### 1.7 Embedded migrations

- [ ] Implement `midas-macros::migrate!`.
  - [ ] Expand to `midas::Migrator::with_migrations(vec![...])`.
  - [ ] Use `include_str!` with absolute paths for every file in the directory.
  - [ ] Preserve file order by version.
  - [ ] Fail compilation on malformed filenames.
- [ ] Add a `build.rs` helper or documented `cargo:rerun-if-changed=migrations`
      snippet in the test fixture.
- [ ] Add a fixture crate `crates/midas/tests/fixtures/embedded/`:
  - [ ] `Cargo.toml` depending on `midas` with `features = ["macros", "sqlite"]`.
  - [ ] `migrations/1_init.sql`.
  - [ ] `src/main.rs` running the embedded migrator against `sqlite::memory:`.
- [ ] Add a CI test that runs the fixture.

### 1.8 Multi-statement execution

- [ ] Add `Executor::execute_batch`.
- [ ] SQLite:
  - [ ] Replace `Connection::execute` with `Connection::execute_batch`.
  - [ ] Add a test with three statements in one migration.
- [ ] Postgres:
  - [ ] Keep `simple_query` for multi-statement scripts.
  - [ ] Add a test with three statements.
- [ ] Add a test with a dollar-quoted function body containing semicolons on Postgres.

### 1.9 Database lifecycle fixes

- [ ] Postgres `drop_database`:
  - [ ] Connect to `postgres` (or `template1` when dropping `postgres`).
  - [ ] Execute `DROP DATABASE IF EXISTS`.
  - [ ] Add `force_drop_database` that terminates active backends.
- [ ] SQLite `drop_database`:
  - [ ] Delete the file and recreate an empty connection.
- [ ] Add `DatabaseAdmin` tests for create/drop/exists on both dialects.
- [ ] Add `crates/midas-cli/src/commands/db_lifecycle.rs` with `db create` and
      `db drop` using `DatabaseAdmin`.
- [ ] Implement `db create --from-migrations <dir>`:
  - [ ] Create the database if absent.
  - [ ] Apply all migrations from the directory.
- [ ] Add tests for `db create --from-migrations` on Postgres and SQLite.

## Acceptance criteria

- AC-1.1 SHA-384 checksum fixtures match SQLx for the same SQL bytes.
- AC-1.2 Blank lines, dollar-quoted bodies, and trailing newlines survive parsing
      byte-for-byte in `up`/`down`.
- AC-1.3 The filename table tests pass, including SQLx-style versions and reversible
      suffixes.
- AC-1.4 Metadata v2 upgrade preserves all applied versions and adds the new columns.
- AC-1.5 Two concurrent `run` processes apply each migration exactly once.
- AC-1.6 A failed migration leaves a dirty row; `resolve --applied` and
      `resolve --rolled-back` both recover correctly.
- AC-1.7 Multi-statement migrations pass on Postgres and SQLite.
- AC-1.8 `--dry-run` executes no SQL (assert database state unchanged).
- AC-1.9 The embedded `migrate!` fixture compiles and applies migrations.
- AC-1.10 No artificial sleeps remain; no implicit SQLite database URL remains.
- AC-1.11 Legacy rows with all-zero checksums are skipped during validation and
      `migrate repair` populates their real checksums.

---

# Phase 2 — SQLx Interop

## Goals and overview

Make Midas adoptable by SQLx users without changing queries. This phase consumes
Phase 1's migration model and metadata. It is file- and metadata-based; no SQLx
production dependency is added.

Roadmap reference: WS4, M2; audit section 9.7.

## Rules

- Target SQLx `0.9.x` and pin the version in conformance fixtures.
- Do not add SQLx to `midas` production dependencies.
- Export must be byte-compatible: `<version>_<description>.up.sql` and `.down.sql`,
  `-- no-transaction` first, SHA-384 checksums.
- Import must not rewrite SQL content; only split or wrap it into Midas format.
- Adoption must never re-run migrations already recorded in `_sqlx_migrations`.
- `cargo sqlx prepare` tests run in CI with a real database.
- `sqlx.toml` is read when present; unsupported fields are ignored with a warning.

## Work items

### 2.1 Migration import/export

- [ ] Add `crates/midas/src/sqlx/mod.rs`, `import.rs`, `export.rs`.
- [ ] Implement `export_sqlx(migrations, out_dir)`:
  - [ ] Write `<version>_<description>.up.sql` for every migration.
  - [ ] Write `<version>_<description>.down.sql` when `down` exists.
  - [ ] Preserve the exact `up`/`down` strings.
  - [ ] Prepend `-- no-transaction\n` when `no_tx` is true.
  - [ ] Reject descriptions that cannot be represented in a filename.
- [ ] Implement `import_sqlx(dir) -> Vec<Migration>`:
  - [ ] Parse `<version>_<description>.sql` as one-way.
  - [ ] Parse `<version>_<description>.up.sql` and `.down.sql` as reversible.
  - [ ] Detect `-- no-transaction`.
  - [ ] Compute SHA-384 over imported SQL.
- [ ] Add round-trip tests:
  - [ ] Export then import yields identical `up`, `down`, `no_tx`, and checksums.
  - [ ] Define round-trip equality on exported SQLx files, where `no_tx` is always
        the first line; do not compare against the original Midas marker placement.
  - [ ] Export a reversible migration and assert both files exist.
  - [ ] Import a one-way migration and assert `down = None`.
- [ ] Add a CI step that installs `sqlx-cli` and runs:
  - [ ] `sqlx migrate info --source <exported-dir>`
  - [ ] `sqlx migrate run --source <exported-dir>`

### 2.2 Adopt `_sqlx_migrations`

- [ ] Implement `midas migrate baseline --from sqlx`:
  - [ ] Read `_sqlx_migrations` (`version`, `description`, `success`, `checksum`).
  - [ ] Match versions to local migration files.
  - [ ] Insert matching rows into the configured Midas metadata table.
  - [ ] Fail if a local file is missing for an applied version.
- [ ] Implement `midas migrate adopt --table _sqlx_migrations`:
  - [ ] Set `metadata_schema = None` and `metadata_table = "_sqlx_migrations"`.
  - [ ] Verify the table has SQLx's columns.
  - [ ] Do not alter the SQLx-managed table. Detect whether `down_checksum` exists.
  - [ ] If `down_checksum` is absent, skip DOWN checksum storage and validate DOWN
        from the migration file on revert.
  - [ ] Refuse to adopt if `success = false` rows exist.
- [ ] Implement `sqlx.toml` reading using SQLx's kebab-case keys:
  - [ ] Read `[migrate] table-name`, `create-schemas`, `migrations-dir`,
        `ignored-chars`, and `unrecognized-sql-files`.
  - [ ] Read `[migrate.defaults] migration-type` and `migration-versioning` for
        import/add defaults.
  - [ ] Add tests with a fixture `sqlx.toml`.
- [ ] Add integration tests:
  - [ ] A database with SQLx-applied migrations is baselined without re-running SQL.
  - [ ] A dirty `_sqlx_migrations` row causes adoption to fail with a clear error.
  - [ ] A custom table name from `sqlx.toml` is honored.

### 2.3 Schema-only database for `cargo sqlx prepare`

- [ ] Use Phase 1's `midas db create --from-migrations <dir>` in the SQLx prepare
      workflow:
  - [ ] Create the database if absent.
  - [ ] Apply all migrations in the directory.
  - [ ] Do not create SQLx metadata; Midas metadata is allowed.
- [ ] Defer `db create --from-schema` to Phase 5, because it needs the DSL.
- [ ] Add a test that `db create --from-migrations` followed by
      `cargo sqlx prepare` works without a DSL file.
- [ ] Add a CI fixture `crates/midas/tests/fixtures/sqlx-prepare/`:
  - [ ] `Cargo.toml` with `sqlx 0.9` features `runtime-tokio`, `postgres`,
        `macros`, `migrate`.
  - [ ] `src/main.rs` with `sqlx::query!` and `query_as!` calls.
  - [ ] `.env` with `DATABASE_URL`.
- [ ] Add CI steps:
  - [ ] `midas db create --from-migrations <dir>`.
  - [ ] `cargo sqlx prepare --check --workspace`.
  - [ ] `SQLX_OFFLINE=true cargo check`.
- [ ] Add tests for `--workspace`, `--all-targets --all-features`, and queries inside
      `#[cfg(test)]`.

### 2.4 Conformance and compatibility

- [ ] Add a `sqlx-conformance` CI job with a Postgres service.
- [ ] Verify `execution_time` is stored in nanoseconds.
- [ ] Verify `success = false` is treated as dirty.
- [ ] Verify `ignore_missing` behavior when a local migration is removed.
- [ ] Verify `--no-lock` behavior against CockroachDB-like settings.
- [ ] Verify `create-schemas` places the metadata table in the requested schema.
- [ ] Add a `midas doctor --sqlx` check that detects `_sqlx_migrations` and prints
      the recommended next command.

## Acceptance criteria

- AC-2.1 `midas migrate export --format sqlx` output passes `sqlx migrate info` and
      `sqlx migrate run`.
- AC-2.2 Import then export is byte-identical for UP/DOWN content and checksums.
- AC-2.3 Baseline from `_sqlx_migrations` never re-runs applied SQL.
- AC-2.4 `midas migrate adopt --table _sqlx_migrations` works with SQLx 0.9.
- AC-2.5 `cargo sqlx prepare --check` passes against a database created with
      `midas db create --from-migrations`.
- AC-2.6 `SQLX_OFFLINE=true cargo check` passes in the fixture crate.
- AC-2.7 `sqlx.toml` `table-name`, `create-schemas`, `ignored-chars`, and
      `migrations-dir` are honored.
- AC-2.8 CI runs the conformance job against Postgres.
- AC-2.9 Adoption does not alter `_sqlx_migrations`; a missing `down_checksum`
      degrades gracefully and DOWN is validated from the file on revert.

---

# Phase 3 — Schema DSL Core

## Goals and overview

Implement the TOML schema language from `docs/dsl.md`: AST, IR, type parser,
validation, deterministic naming, multi-file merge, and a Postgres create-from-empty
emitter. This phase produces SQL and diagnostics but does not diff yet.

Roadmap reference: WS3, M3; audit section 9.6.

## Rules

- TOML is the only authoring format. Do not add Lua, Starlark, YAML, or JSON.
- Unknown fields are errors via `#[serde(deny_unknown_fields)]`.
- Always double-quote identifiers in emitted SQL.
- Expressions are trusted developer input; reject semicolons outside function bodies
  and `[[raw]]` blocks.
- The IR is dialect-neutral and fully resolved; no `toml::Value` leaks into it.
- Object order is preserved where semantically meaningful: columns, enum values,
  index columns, partitions.
- The emitter is deterministic: identical input produces identical bytes.
- Unsupported Postgres features belong in `[[raw]]`, not in ad-hoc fields.

## Work items

### 3.1 AST and parser

- [ ] Add `crates/midas/src/schema/ast.rs`.
  - [ ] Define serde structs for top-level `version`, `project`, `schema`,
        `extension`, `enum`, `domain`, `composite`, `range`, `collation`,
        `sequence`, `table`, `view`, `function`, `procedure`, `trigger`,
        `policy`, `role`, `grant`, `default_privilege`, `publication`,
        `statistics`, `raw`, `ignore`.
  - [ ] Add `#[serde(deny_unknown_fields)]` to every struct.
  - [ ] Represent shorthand fields with untagged enums:
        `bool | Table`, `String | Table`, `String | Vec<String>`.
- [ ] Add `crates/midas/src/schema/parse.rs`.
  - [ ] `parse_file(path) -> Result<ast::File>`.
  - [ ] `parse_dir(path) -> Result<ast::File>` merging `*.toml` in filename order.
  - [ ] Reject duplicate object names and duplicate top-level keys.
  - [ ] Report semantic errors with file path and TOML span.
- [ ] Add parser tests:
  - [ ] Full `docs/dsl.md` example parses.
  - [ ] Unknown field error includes the field name and line.
  - [ ] Duplicate table error includes both files.
  - [ ] Directory merge preserves column order.

### 3.2 IR and type parser

- [ ] Add `crates/midas/src/schema/ir.rs`.
  - [ ] Define resolved types: `Schema`, `Table`, `Column`, `Constraint`, `Index`,
        `Enum`, `Domain`, `Composite`, `Range`, `Sequence`, `View`, `Function`,
        `Trigger`, `Policy`, `Role`, `Grant`, `Raw`.
  - [ ] Use `IndexMap` for ordered collections.
  - [ ] Define `QualifiedName { schema: Option<String>, name: String }`.
- [ ] Add `crates/midas/src/schema/types.rs`.
  - [ ] Parse the type grammar from `docs/dsl.md`.
  - [ ] Normalize aliases (`int` -> `integer`, `bool` -> `boolean`,
        `timestamptz` -> `timestamp with time zone`).
  - [ ] Preserve precision and scale.
  - [ ] Support arrays and schema-qualified custom types.
  - [ ] Add table-driven tests for every built-in type and common aliases.
- [ ] Add `crates/midas/src/schema/expr.rs`.
  - [ ] Wrap SQL expressions in `SqlExpr(String)`.
  - [ ] Reject semicolons except when `allow_statement = true`.
  - [ ] Add tests for rejecting `drop table x;` in `default`.
- [ ] Add `crates/midas/src/schema/validate.rs`.
  - [ ] Check duplicate names, FK targets, PK/unique existence, type references,
        index columns, enum values, partition bounds, identifier length.
  - [ ] Add tests for each validation rule.
- [ ] Add `crates/midas/src/schema/naming.rs`.
  - [ ] Implement deterministic names for PK, unique, FK, check, index, sequence.
  - [ ] Implement 63-byte truncation with an 8-character hash suffix.
  - [ ] Add snapshot tests for long names and collisions.

### 3.3 Postgres emitter

- [ ] Add `crates/midas/src/schema/emit/mod.rs` and `emit/postgres.rs`.
  - [ ] Emit `create schema`, `create extension`, `create type ... as enum`,
        domains, composites, ranges, collations.
  - [ ] Emit `create table` with columns, nullability, defaults, identity,
        generated columns, collation, storage, compression.
  - [ ] Emit PK, unique, check, FK, exclusion constraints.
  - [ ] Emit indexes with method, expressions, opclass, order, nulls, include,
        where, with, tablespace, concurrent.
  - [ ] Emit comments.
  - [ ] Emit `[[raw]]` blocks in `order` order.
- [ ] Add snapshot tests:
  - [ ] Table with identity, default, check, unique, FK.
  - [ ] Partial expression GIN index.
  - [ ] Enum + composite type + domain.
  - [ ] Partitioned table with range/list/hash partitions.
- [ ] Add a test that emitting the same IR twice produces identical bytes.

### 3.4 CLI commands

- [ ] Add `crates/midas-cli/src/commands/schema.rs`.
  - [ ] `schema validate <path>` with `--json`.
  - [ ] `schema sql <path>` with `--output <file>` and `--dialect postgres`.
  - [ ] `schema format <path>` writes canonical TOML to stdout or `--check` mode.
- [ ] Add CLI tests:
  - [ ] `schema validate` succeeds on the full example.
  - [ ] `schema validate --json` emits a stable diagnostic shape.
  - [ ] `schema sql` output matches the snapshot.
  - [ ] `schema format --check` fails on non-canonical input.

### 3.5 JSON Schema and versioning

- [ ] Add `schemars` to generate `schema.schema.json` from the AST.
  - [ ] Add `schema json-schema` command.
  - [ ] Add a test that validates the full example and all fixtures against it.
- [ ] Enforce `version = 1`:
  - [ ] Reject missing `version`.
  - [ ] Reject unsupported versions with `Error::Unsupported`.
  - [ ] Add tests for both.

### 3.6 Canonical formatter

- [ ] Implement a canonical AST writer:
  - [ ] Stable key order per object kind.
  - [ ] Stable array order.
  - [ ] Two-space indentation.
  - [ ] Trailing newline.
- [ ] Add idempotence tests:
  - [ ] `format(format(x)) == format(x)`.
  - [ ] Parsing formatted output yields the same IR.

## Acceptance criteria

- AC-3.1 The full example in `docs/dsl.md` parses, validates, and emits SQL.
- AC-3.2 All snapshot tests for the Postgres emitter pass.
- AC-3.3 Malformed TOML and semantic errors include file path and line number.
- AC-3.4 Multi-file schema directories merge deterministically and reject duplicates.
- AC-3.5 `schema.schema.json` validates every fixture.
- AC-3.6 `schema format` is idempotent.
- AC-3.7 `schema validate --json` has a stable, snapshot-tested shape.
- AC-3.8 `schema sql` is byte-identical across repeated runs.

---

# Phase 4 — Diff Engine

## Goals and overview

Compute deterministic changes between two schema IRs and emit reversible `-- !UP` /
`-- !DOWN` migrations. This phase is offline: it diffs a previous schema snapshot
against the desired schema and does not require a live database.

Roadmap reference: WS3, M4; audit section 9.6.

## Rules

- Diff is desired-state vs previous-state, not live-database vs desired-state.
- A previous state is stored as `.midas/state.json` after each successful
  `migrate dev`.
- Explicit renames use `was` / `previous_name`; never infer renames heuristically.
- Unmanaged objects are never dropped.
- Destructive changes require `--accept-data-loss`.
- Plan order is fixed: schemas, extensions, types, sequences, tables, columns,
  constraints, indexes, views, functions, triggers, policies, grants, raw.
- Down migrations are the reverse of up changes, in reverse order.
- Do not emit `CASCADE` by default.
- Emit `USING` for type changes only when the DSL provides `using`; otherwise
  refuse the change.
- Enum value removal and other irreversible changes are marked irreversible and
  require `--accept-data-loss`.

## Work items

### 4.1 Change model and plan

- [ ] Add `crates/midas/src/schema/diff.rs`.
  - [ ] Define `pub enum Change` with create/drop/alter variants for every IR object.
  - [ ] Add `phase: Phase`, `destructive: bool`, `irreversible: bool`,
        `lossy_down: bool`.
  - [ ] Define `pub struct Plan { changes: Vec<Change> }`.
- [ ] Add `crates/midas/src/schema/plan.rs`.
  - [ ] Stable sort by phase and then by qualified name.
  - [ ] Topologically order FK creation after tables and FK drops before tables.
  - [ ] Add tests for ordering with cyclic FKs.
- [ ] Add `Plan::summary() -> PlanSummary` for JSON output.
- [ ] Add a test that `diff(schema, schema)` returns an empty plan.

### 4.2 Object diffing

- [ ] Implement diffs for:
  - [ ] Schemas, extensions, enums, domains, composites, ranges, collations.
  - [ ] Sequences, tables, columns, PKs, unique/check/FK/exclusion constraints.
  - [ ] Indexes, views, materialized views, functions, procedures, triggers,
        policies, roles, grants, default privileges, publications, statistics.
  - [ ] Raw blocks (append/remove by name).
- [ ] Implement rename detection:
  - [ ] Table/column `was` fields.
  - [ ] Refuse renames that conflict with an existing object.
- [ ] Implement type-change diffs:
  - [ ] `alter column ... type ... using ...` when `using` is provided.
  - [ ] Refuse narrowing changes without `using`.
- [ ] Implement enum changes:
  - [ ] Add values with `alter type ... add value`.
  - [ ] Mark removal as irreversible.
- [ ] Add unit tests for each change kind and rename case.

### 4.3 SQL emission

- [ ] Add `crates/midas/src/schema/emit/alter.rs`.
  - [ ] Emit UP and DOWN for every change kind.
  - [ ] Group `ALTER TABLE` statements per table.
  - [ ] Use `if not exists` / `if exists` where Postgres supports it.
  - [ ] Emit `NOT VALID` and separate `VALIDATE CONSTRAINT` when requested.
  - [ ] Emit `CREATE INDEX CONCURRENTLY` and mark the migration
        `-- no-transaction`.
- [ ] Add snapshot tests:
  - [ ] Add/drop column.
  - [ ] Add/drop table.
  - [ ] Add/drop index.
  - [ ] Add/drop FK, unique, check.
  - [ ] Type change with `using`.
  - [ ] Rename table and column.
  - [ ] Enum add value and removal refusal.
- [ ] Add an apply test on SQLite/Postgres:
  - [ ] Apply UP then DOWN returns to the original schema.

### 4.4 State snapshots

- [ ] Add `crates/midas/src/schema/state.rs`.
  - [ ] Serialize/deserialize the IR to `.midas/state.json`.
  - [ ] Include a `schema_hash` field.
  - [ ] Refuse to diff if the snapshot is corrupt.
- [ ] Add tests:
  - [ ] Round-trip IR through JSON yields an empty diff.
  - [ ] Corrupt state returns `Error::Schema`.

### 4.5 CLI commands

- [ ] Add `crates/midas-cli/src/commands/migrate_dev.rs`.
  - [ ] `migrate dev --name <name> --create-only`.
  - [ ] `migrate diff --from <toml|state.json> --to <toml|state.json> --script`.
  - [ ] `--accept-data-loss`, `--dry-run`, `--json`.
- [ ] Implement `migrate dev`:
  - [ ] Load `.midas/state.json` (empty schema if absent).
  - [ ] Load and validate `schema.toml`.
  - [ ] Diff.
  - [ ] Refuse destructive changes without `--accept-data-loss`.
  - [ ] Write a timestamped migration with `-- !UP` / `-- !DOWN`.
  - [ ] Update `.midas/state.json`.
  - [ ] Apply the migration unless `--create-only`.
- [ ] Add CLI tests:
  - [ ] First `migrate dev` creates a create-from-empty migration.
  - [ ] Second `migrate dev` with no changes writes no file.
  - [ ] Destructive change without the flag exits non-zero.
  - [ ] `migrate diff --script` prints UP/DOWN to stdout.
  - [ ] `--dry-run` writes no files and executes no SQL.

## Acceptance criteria

- AC-4.1 `diff(schema, schema)` is empty.
- AC-4.2 Add/drop table, column, constraint, and index snapshots match.
- AC-4.3 Rename via `was` emits `ALTER ... RENAME`, not drop/create.
- AC-4.4 Destructive changes fail without `--accept-data-loss`.
- AC-4.5 Applying UP then DOWN on SQLite and Postgres returns to the original schema.
- AC-4.6 `migrate dev --create-only` writes a valid `-- !UP` / `-- !DOWN` file.
- AC-4.7 Plan ordering is stable and handles cyclic foreign keys.
- AC-4.8 `.midas/state.json` round-trips and corrupt state is rejected.

---

# Phase 5 — Introspection & Shadow DB

## Goals and overview

Read an existing Postgres database into the schema IR, emit TOML with `db pull`, and
add shadow-database replay and drift detection to `migrate dev`. This phase turns the
schema layer into an adoption tool.

Roadmap reference: WS3, M5; audit section 9.6.

## Rules

- Introspect only schemas listed in `[managed].schemas`.
- Ignore `pg_catalog`, `information_schema`, `pg_toast`, `pg_temp_*`, and `midas`.
- Ignore extension-owned objects by default (`pg_depend.deptype = 'e'`), but include
  the extension itself.
- Use `pg_get_expr`, `pg_get_constraintdef`, `pg_get_indexdef`,
  `pg_get_viewdef`, `pg_get_functiondef`, and `pg_get_triggerdef` for canonical
  definitions.
- Expression normalization is conservative; report drift rather than silently
  accepting differences.
- `db pull` never overwrites an existing file without `--force`.
- Unsupported objects produce warnings and are not silently dropped.
- Shadow databases use unique names containing pid and an atomic counter.

## Work items

### 5.1 Postgres introspection queries

- [ ] Add `crates/midas/src/schema/introspect/mod.rs` and `postgres.rs`.
- [ ] Implement queries for:
  - [ ] Schemas, extensions, collations.
  - [ ] Enums, domains, composites, ranges.
  - [ ] Tables, columns, identity, generated columns, collation, storage.
  - [ ] PK, unique, check, FK, exclusion constraints.
  - [ ] Indexes (method, expressions, opclass, order, nulls, include, where, with).
  - [ ] Sequences and ownership.
  - [ ] Views, materialized views, options.
  - [ ] Functions, procedures, arguments, volatility, security, cost, rows, set.
  - [ ] Triggers, including constraint triggers and transition tables.
  - [ ] Policies and RLS flags.
  - [ ] Roles, membership, grants, default privileges.
  - [ ] Partitions and inheritance.
  - [ ] Publications and extended statistics.
  - [ ] Comments.
- [ ] Add a test fixture database created by `docs/dsl.md`'s full example.
- [ ] Add unit tests that each catalog query returns the expected IR nodes.

### 5.2 IR mapping and filtering

- [ ] Map catalog rows to the IR.
- [ ] Filter system schemas and extension-owned objects.
- [ ] Apply `[[ignore]]` patterns.
- [ ] Report unsupported objects as warnings with schema/name/type.
- [ ] Add tests:
  - [ ] Extension-owned tables are excluded.
  - [ ] Ignored objects are excluded.
  - [ ] Unsupported event triggers are reported.

### 5.3 `db pull`

- [ ] Add `crates/midas-cli/src/commands/db_schema.rs`.
  - [ ] `db pull --schema <name> --output <path> --force --include-raw`.
  - [ ] `db push --accept-data-loss --force`.
  - [ ] `db create --from-schema` (DSL-aware; extends Phase 1's lifecycle command).
  - [ ] `db drop --force` (owned by Phase 1's lifecycle command).
- [ ] Implement `db pull`:
  - [ ] Introspect.
  - [ ] Convert IR to `schema.toml` using the canonical formatter.
  - [ ] Print a unified diff when the output file exists.
  - [ ] Write only with `--force`.
- [ ] Add a test that `db create --from-schema` followed by
      `cargo sqlx prepare --check` passes for a DSL-backed project.
- [ ] Add CLI tests:
  - [ ] `db pull` on the fixture writes a parseable TOML file.
  - [ ] Second `db pull` without `--force` exits non-zero and leaves the file
        unchanged.
  - [ ] `db pull --include-raw` emits unsupported event triggers as `[[raw]]`.

### 5.4 Shadow DB and drift

- [ ] Add `crates/midas/src/schema/shadow.rs`.
  - [ ] `ShadowDb::create(config) -> ShadowDb`.
  - [ ] Replay all migrations from the migrations directory.
  - [ ] Introspect the shadow database.
  - [ ] Drop the shadow database on `Drop`.
- [ ] Extend `migrate dev`:
  - [ ] Create a shadow database.
  - [ ] Replay migrations.
  - [ ] Introspect the shadow schema.
  - [ ] Compare with `.midas/state.json`; fail on drift unless `--force`.
  - [ ] Diff desired vs shadow IR.
  - [ ] Generate the migration.
  - [ ] Apply to the development database unless `--create-only`.
- [ ] Add integration tests:
  - [ ] Drift detection catches an out-of-band column.
  - [ ] `--force` bypasses drift.
  - [ ] Shadow database is dropped after success and after failure.
  - [ ] Two concurrent `migrate dev` runs use unique shadow names.

### 5.5 Round-trip proof

- [ ] Add a test that:
  - [ ] Creates a database from `schema.toml`.
  - [ ] Runs `db pull`.
  - [ ] Diffs the pulled schema against the original.
  - [ ] Asserts an empty plan.
- [ ] Add the same test for the full `docs/dsl.md` example.
- [ ] Add a test that re-running `db pull` produces byte-identical TOML.

## Acceptance criteria

- AC-5.1 `db pull` on the fixture produces TOML that parses and validates.
- AC-5.2 DSL -> SQL -> apply -> introspect -> diff is empty.
- AC-5.3 Extension-owned objects are excluded by default.
- AC-5.4 Unsupported objects are reported and optionally emitted as `[[raw]]`.
- AC-5.5 Drift detection fails `migrate dev` without `--force`.
- AC-5.6 Shadow databases are always dropped, including on panic.
- AC-5.7 Two concurrent `migrate dev` runs do not collide.
- AC-5.8 Re-running `db pull` produces byte-identical output.

---

# Phase 6 — Codegen & Offline Metadata

## Goals and overview

Generate Rust types and SQLx derives from the schema IR, and provide an offline
metadata path so SQLx users can build without a live database. This phase depends on
Phase 3's IR only.

Roadmap reference: WS5, M6; audit section 9.7.

## Rules

- Generate code from the IR, never from a live database.
- Generated code must compile against SQLx 0.9.
- Nullable columns map to `Option<T>`.
- Arrays map to `Vec<T>`.
- Postgres enums map to Rust enums with `#[derive(sqlx::Type)]` and
  `#[sqlx(type_name = "...")]`.
- Domains map to newtypes with `#[sqlx(transparent)]`.
- Composite types are not supported by SQLx; skip them and emit a warning.
- Use `#[sqlx(rename = "...")]` when the Rust field name differs from the column.
- `.sqlx` metadata must match `cargo sqlx prepare` for the same queries and schema.
- Pin SQLx in conformance fixtures and test the exact version.

## Work items

### 6.1 Type mapping

- [ ] Add `crates/midas/src/codegen/types.rs`.
  - [ ] Map Postgres types to Rust:
        `bigint` -> `i64`, `integer` -> `i32`, `smallint` -> `i16`,
        `boolean` -> `bool`, `text`/`varchar` -> `String`, `bytea` -> `Vec<u8>`,
        `uuid` -> `uuid::Uuid`, `json`/`jsonb` -> `serde_json::Value`,
        `numeric` -> `rust_decimal::Decimal`, `timestamp*` -> `chrono::DateTime`,
        `date` -> `chrono::NaiveDate`, `time` -> `chrono::NaiveTime`.
  - [ ] Map arrays to `Vec<T>`.
  - [ ] Map nullable to `Option<T>`.
  - [ ] Map enums and domains.
  - [ ] Add table-driven tests for every mapping.
- [ ] Add feature flags `codegen-uuid`, `codegen-chrono`, `codegen-json`,
      `codegen-decimal` for optional type dependencies.
- [ ] Add a test that generated type aliases match the expected Rust types.

### 6.2 Rust emitter

- [ ] Add `crates/midas/src/codegen/rust.rs`.
  - [ ] Emit one struct per table with `#[derive(Debug, Clone, sqlx::FromRow)]`.
  - [ ] Emit enums with `#[derive(sqlx::Type)]`.
  - [ ] Emit newtypes with `#[sqlx(transparent)]`.
  - [ ] Emit `#[sqlx(rename)]` and `#[sqlx(default)]` where needed.
  - [ ] Emit `#[allow(dead_code)]` for unused fields.
  - [ ] Format output with `prettyplease`.
- [ ] Add `crates/midas-cli/src/commands/generate.rs`.
  - [ ] `generate --sqlx --out <dir>`.
  - [ ] `--dry-run` prints files to stdout.
- [ ] Add a compile fixture crate:
  - [ ] Depends on `sqlx` 0.9 with `postgres`, `runtime-tokio`, `macros`.
  - [ ] Includes generated code.
  - [ ] Runs `cargo check`.
- [ ] Add tests:
  - [ ] Generated structs compile.
  - [ ] Deserializing rows from SQLite/Postgres works for nullable, array, enum,
        JSON, and UUID columns.

### 6.3 Offline metadata

- [ ] Add `crates/midas/src/sqlx/offline.rs`.
  - [ ] Scan Rust source with `syn` for `sqlx::query!`, `query_as!`,
        `query_scalar!`, and `query_unchecked!`.
  - [ ] Extract the SQL string literal and bind parameters.
  - [ ] Parse SQL with `sqlparser` and resolve tables/columns against the IR.
  - [ ] Compute SQLx's query hash and write `.sqlx/query-<hash>.json`.
  - [ ] Handle workspaces, feature flags, and queries inside tests.
- [ ] Add `midas prepare` and `midas prepare --check`.
- [ ] Add a conformance test:
  - [ ] Run `cargo sqlx prepare` and `midas prepare` on the same fixture.
  - [ ] Assert the `.sqlx` JSON files are semantically equal.
- [ ] Add tests:
  - [ ] `prepare --check` fails when a query changes.
  - [ ] `prepare --check` passes when metadata is current.
  - [ ] Unknown column produces a compile-time-style error with file/line.

### 6.4 Schema-only DB fallback

- [ ] Keep the Phase 2 `db create --from-migrations` path as the default offline
      method.
- [ ] Add a test that documents `cargo sqlx prepare` works against the Phase 2
      provisioned database.
- [ ] Add a test that `midas prepare` produces the same result as
      `cargo sqlx prepare` for the fixture.

## Acceptance criteria

- AC-6.1 Generated code compiles against SQLx 0.9.
- AC-6.2 Nullable, array, enum, JSON, UUID, and numeric mappings are correct.
- AC-6.3 `FromRow` decoding works against SQLite and Postgres fixtures.
- AC-6.4 `.sqlx` metadata is semantically equal to `cargo sqlx prepare` output.
- AC-6.5 `midas prepare --check` detects stale metadata.
- AC-6.6 Composite types are skipped with a warning, not emitted as invalid code.
- AC-6.7 `--dry-run` prints all generated files and writes nothing.

---

# Phase 7 — Hardening, CI & Release Automation

## Goals and overview

Continuously add automated safety nets: database integration tests, concurrency
tests, fuzzing, feature/MSRV matrices, SQLx conformance, license/security checks,
cross-platform line-ending tests, and container/deb smoke tests. This phase can start
after Phase 0 and should grow with each feature.

Roadmap reference: WS6, M7; audit section 9.8.

## Rules

- CI must provision Postgres services or use Testcontainers, plus temp files for
  SQLite.
- Use unique database names containing pid and an atomic counter.
- Use `cargo test --quiet` in CI to keep logs focused.
- No fixed sleeps. Use bounded wait helpers with explicit assertions.
- Every spawned task in tests must be joined or cancelled.
- Feature matrix must cover `--no-default-features`, each driver, and
  `--all-features`.
- SQLx conformance runs on stable Rust because SQLx 0.9 may require a newer toolchain
  than Midas's MSRV.
- No release publishing or manual review tasks in this file.

## Work items

### 7.1 CI matrix and MSRV

- [ ] Add an MSRV job using Rust `1.99`.
  - [ ] Run `cargo check --workspace --all-features`.
  - [ ] Run `cargo test --workspace --all-features --quiet`.
- [ ] Add a feature matrix job:
  - [ ] `--no-default-features`
  - [ ] `--features postgres`
  - [ ] `--features sqlite`
  - [ ] `--features tls-native-tls`
  - [ ] `--features tls-rustls`
  - [ ] `--all-features`
- [ ] Add a cross-build job for musl, aarch64, macOS, and Windows.
  - [ ] Verify `openssl-vendored` builds for musl.
  - [ ] Verify `tls-rustls` builds without a C toolchain.
- [ ] Add `cargo fmt --all --check` and
      `cargo clippy --workspace --all-targets --all-features -- -D warnings`.

### 7.2 Database integration tests

- [ ] Add Postgres service or Testcontainers job:
  - [ ] Run migration apply/revert/resolve tests.
  - [ ] Run metadata upgrade tests.
  - [ ] Run shadow DB/drift tests.
  - [ ] Run introspection round-trip tests.
- [ ] Add SQLite job:
  - [ ] Run apply/revert/checksum tests.
  - [ ] Run multi-statement tests.
- [ ] Add a test that checks every dialect reports unsupported DSL features with
      `Error::Unsupported`.

### 7.3 Concurrency and failure tests

- [ ] Add a concurrency test that spawns two processes running `migrate run`.
  - [ ] Assert each migration row appears once.
  - [ ] Assert one process reports lock timeout when configured.
- [ ] Add a failure test that kills the process during a migration.
  - [ ] Assert the dirty row exists.
  - [ ] Assert `resolve` works.
- [ ] Add a test that a panic inside a migration releases the advisory lock.

### 7.4 Fuzzing and property tests

- [ ] Add `cargo-fuzz` targets:
  - [ ] `fuzz_migration_parser`
  - [ ] `fuzz_dsl_parser`
  - [ ] `fuzz_type_parser`
- [ ] Add property tests:
  - [ ] `format(parse(format(x))) == format(x)`.
  - [ ] `diff(schema, schema).is_empty()`.
  - [ ] `apply(up); apply(down)` returns to the original state for SQLite.
- [ ] Add a CI smoke job that runs each fuzz target for 30 seconds.

### 7.5 Security and dependency checks

- [ ] Add `cargo-deny` job.
  - [ ] License allowlist.
  - [ ] Banned crates (`openssl` in default features, `dotenv`).
  - [ ] Unknown source denial.
- [ ] Add `cargo-audit` job.
- [ ] Add tests:
  - [ ] `create` rejects path traversal.
  - [ ] Error messages redact database passwords.
  - [ ] `Config::redacted_database_url` redacts passwords in logs.
  - [ ] A migration containing `;` in a scalar expression is rejected by the DSL
        validator.
  - [ ] `[[raw]]` blocks are the only place multi-statement expressions are allowed.
- [ ] Add a test that `grant all on schema midas to public` is not present in the
      generated metadata schema SQL.

### 7.6 Platform and packaging smoke tests

- [ ] Add a Windows line-ending test:
  - [ ] Check out a CRLF migration fixture.
  - [ ] Assert the checksum equals the LF checksum.
- [ ] Add a Docker smoke test:
  - [ ] Build the container.
  - [ ] Run `midas --version`.
  - [ ] Run `midas status` against SQLite.
- [ ] Add a distroless smoke test with SQLite.
- [ ] Add a `.deb` package smoke test:
  - [ ] Install in a container.
  - [ ] Run `midas --version`.
- [ ] Add an `install.sh` smoke test in a container.

## Acceptance criteria

- AC-7.1 CI passes on Linux, macOS, and Windows.
- AC-7.2 The MSRV 1.99 job passes.- [ ] Add a Docker smoke test:

- AC-7.3 The feature matrix passes for every listed combination.
- AC-7.4 Postgres and SQLite integration suites pass.
- AC-7.5 Concurrency tests prove migrations apply exactly once.
- AC-7.6 Fuzz targets run without crashes for 30 seconds each.
- AC-7.7 `cargo deny` and `cargo audit` pass.
- AC-7.8 Windows CRLF checksums match LF checksums.
- AC-7.9 Container, distroless, `.deb`, and `install.sh` smoke tests pass.
- AC-7.10 No secret appears in logs or error messages in the redaction tests.
