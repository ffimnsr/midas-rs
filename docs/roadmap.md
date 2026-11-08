# Roadmap: Win Migrations and Schema, Interoperate with SQLx

> **Historical planning document.** The canonical backlog lives in
> [ISSUES.md](ISSUES.md). Database scope is intentionally narrow: Postgres
> (including CockroachDB and Postgres-wire compatible servers) and SQLite.
> MySQL and MariaDB are no longer supported, so MySQL items below are retained
> only as design history.
>
> Status: accepted direction.
>
> Target: Midas owns the schema and migration lifecycle. SQLx owns runtime queries.
> We do not reimplement SQLx.

---

## 1. Direction

### One-sentence strategy

**Midas is the schema-first migration and type-generation engine; SQLx remains the
runtime query toolkit. We win by being the best place to define, diff, verify, and
evolve a database schema, and by producing artifacts SQLx can consume.**

### What we will do

1. Make the migration engine strictly better than `sqlx migrate` / `sqlx-cli`.
2. Make the schema DSL, diffing, and introspection first-class.
3. Interoperate with SQLx at the file, metadata, and code-generation layers.
4. Stay synchronous, lean, and CLI-first; no runtime query API.

### What we will not do (for 1.0)

- No async connection pool, query builder, or ORM runtime.
- No `query!`-style compile-time checking that requires a live database.
- No MSSQL, Oracle, Cassandra, MySQL, or MariaDB drivers.
- No scripting DSL (Lua/Starlark) and no YAML front-end yet.
- No hosted service.

### The adoption promise

A team using SQLx should be able to adopt Midas for schema and migrations without
changing a single query:

```bash
midas migrate import --from sqlx
midas migrate baseline --from sqlx
midas schema pull
midas migrate dev --name add_jobs
# SQLx queries continue to work unchanged
```

---

## 2. Definition of done

### Migration layer

- Feature parity with SQLx migrations, plus:
  - checksums and edited-migration detection;
  - advisory locking across processes;
  - transactional DDL where supported;
  - dirty-state recovery;
  - baseline, skip, repair, resolve;
  - `--dry-run`, `--target`, `--json`;
  - embedded migrations;
  - drift detection and a shadow database;
  - migration linting.
- No panics or `unwrap()` in driver paths.
- Correct multi-statement execution on all dialects.
- Backward compatible with existing `-- !UP` / `-- !DOWN` files.
- Verified with a concurrency test and a SQLx interop test in CI.

### Schema layer

- TOML DSL from `docs/dsl.md` covers the existing `sample_migrations/` schema.
- Deterministic IR, diff, and plan.
- `schema validate`, `schema sql`, `schema format`.
- `db pull`, `db push`, `migrate dev`.
- Shadow-DB replay, drift detection, rename detection, destructive-change flags.
- Round-trip test: DSL → SQL → introspect → diff is empty.

### SQLx interop

- Import and export SQLx migrations.
- Adopt an existing `_sqlx_migrations` table.
- Provision a schema-only database for `cargo sqlx prepare`.
- Generate Rust types with SQLx derives.
- Optional `.sqlx` metadata generation, validated against SQLx's own output.

---

## 3. Current state and gaps

| Area | Today | Needed |
| --- | --- | --- |
| Drivers | Sync `postgres` 0.19, `mysql` 25, `rusqlite` 0.32; all compiled in | Feature flags; shared `Executor`/`Store` traits |
| Metadata | `midas.__schema_migrations` / `__schema_migrations` with only `migration` | SQLx-compatible metadata columns |
| Integrity | No checksums | SHA-384 checksums, edited-migration detection |
| Concurrency | No locking | Advisory locks (same key scheme as SQLx) |
| Transactions | Migration SQL runs without explicit transaction | Transactional DDL, `-- no-transaction` |
| Failure state | Errors leave partial state and no marker | `success = false` + resolve command |
| SQLite | `execute()` only runs one statement | `execute_batch()` |
| Postgres drop | Drops the database from a connection to that database | Connect to `postgres`/`template1` |
| MySQL/SQLite | `unwrap()` on empty metadata table | Return `-1` / typed errors |
| UX | Random 40–300 ms sleeps; implicit SQLite URL | Remove sleeps; require an explicit URL |
| CLI | `faker` and `update` are `unimplemented!()` | Implement or remove from help |
| Library | `anyhow` everywhere; no config object | Typed errors, `Config`, stable API |
| Schema | None | DSL, diff, introspection, codegen |
| SQLx | None | Import/export, adopt, prepare DB, codegen |

---

## 4. Workstreams

### WS1 — Core library foundation

**Goal:** make `midas_core` a dependable library before adding features.

- [ ] Convert to a Cargo workspace:
  - `midas` — core library (current `src/lib.rs`, `commander`, `lookup`, `sequel`, `schema`, `sqlx`, `codegen`)
  - `midas-cli` — the `midas` and `cargo-migrate` binaries
  - `midas-macros` — `migrate!` and, later, `query!`
- [ ] Add `src/error.rs` with a `thiserror` enum. Stop exposing `anyhow::Result` in
      public APIs.
- [ ] Add driver feature flags:
  ```toml
  [features]
  default = ["postgres", "sqlite", "mysql"]
  postgres = ["dep:postgres"]
  mysql = ["dep:mysql"]
  sqlite = ["dep:rusqlite"]
  macros = ["dep:midas-macros"]
  rustls = ["postgres/with-rustls"]
  sqlx-interop = []
  ```
- [ ] Add `Config`:
  ```rust
  pub struct Config {
      pub database_url: String,
      pub migrations_dir: PathBuf,
      pub metadata_table: String,
      pub shadow_database_url: Option<String>,
      pub lock_timeout: Duration,
      pub statement_timeout: Option<Duration>,
      pub transaction: TransactionMode,
  }
  ```
- [ ] Split the `Driver` trait:
  - `Executor` — `execute`, `execute_batch`, `begin`, `commit`, `rollback`
  - `MigrationStore` — metadata and locking
  - `Introspector` — `pg_catalog` / `information_schema`
  - `Dialect` — SQL emission
- [ ] Move command logic from `src/midas/bin/cli.rs` into `midas_core::commands`.
- [ ] Remove vendored OpenSSL from the default build; make TLS optional
      (`rustls` preferred, `native-tls` optional).
- [ ] Establish an MSRV policy and test it in CI.

**Acceptance:** `cargo check` and `cargo test` pass for every feature combination;
public APIs return typed errors; the CLI is a thin wrapper over `commands`.

---

### WS2 — Migration engine

**Goal:** be a strict superset of SQLx's migration runner.

#### Metadata v2

Mirror SQLx's `_sqlx_migrations` shape so adoption and export are trivial:

```sql
version       BIGINT PRIMARY KEY,
description   TEXT NOT NULL,
installed_on  TIMESTAMPTZ NOT NULL DEFAULT now(),
success       BOOLEAN NOT NULL,
checksum      BYTEA NOT NULL,        -- SHA-384
execution_time BIGINT NOT NULL       -- nanoseconds, matching SQLx
```

Midas-specific optional columns:

```sql
down_checksum BYTEA,
source_hash   TEXT,
applied_by    TEXT
```

- [ ] Upgrade existing tables in place (`ALTER TABLE ... ADD COLUMN`).
- [ ] Keep the existing table names for backward compatibility.
- [ ] Add `metadata_table` to `Config` so Midas can use `_sqlx_migrations` directly.
- [ ] Treat any `success = false` row as dirty and refuse to run until resolved.

#### Integrity

- [ ] Compute SHA-384 over the exact SQL content, matching SQLx.
- [ ] Store the UP checksum in `checksum`; store the DOWN checksum separately.
- [ ] Reject edited applied migrations with a clear error.
- [ ] Add `--ignore-checksum` for emergencies.

#### Locking

- [ ] Postgres: `pg_advisory_lock` using the same key derivation as SQLx
      (`crc32(database_name) * 0x3d32ad9e`), so Midas and SQLx cannot run
      concurrently.
- [ ] MySQL/MariaDB: `GET_LOCK('midas:<database>', <timeout>)`.
- [ ] SQLite: `BEGIN IMMEDIATE` on the migration transaction.
- [ ] Add `--no-lock` for CockroachDB and special cases.

#### Transactions and failure handling

- [ ] Wrap each migration and its metadata insert in one transaction where the
      dialect supports transactional DDL.
- [ ] Honor `-- no-transaction` at the top of a migration file (SQLx-compatible).
- [ ] On failure, mark `success = false`; provide `midas migrate resolve`.
- [ ] Document that MySQL DDL implicitly commits.

#### Runner features

- [ ] `migrate run` (alias `up`), `migrate run --target <version>`.
- [ ] `migrate info` / `status` with `--json`.
- [ ] `migrate add --reversible` (creates `.up.sql` / `.down.sql` or Midas markers).
- [ ] `migrate revert --to <version>` (SQLx `undo` semantics).
- [ ] `migrate baseline --to <version>` (SQLx `skip` semantics).
- [ ] `migrate repair`, `migrate resolve`, `migrate reset`.
- [ ] `--dry-run` prints SQL without executing.
- [ ] `migrate lint` checks destructive operations, missing DOWN, `NOT NULL`
      without default, non-concurrent indexes, enum removal.
- [ ] Embedded migrations via `midas::migrate!("./migrations")`.

#### Correctness fixes

- [ ] SQLite: use `execute_batch()` for multi-statement migrations.
- [ ] Postgres: fix `drop_database` by connecting to `postgres` (or `template1`
      when dropping `postgres`).
- [ ] MySQL/SQLite: return `-1` from `get_last_completed_migration` when empty.
- [ ] Remove the artificial `thread::sleep` in `src/commander.rs`.
- [ ] Require an explicit database URL; stop defaulting to `./data.db3`.
- [ ] Implement or remove `faker` and `update`.

#### Drift and shadow database

- [ ] Create a temporary shadow database, replay migrations, introspect it.
- [ ] Compare the shadow schema with the live schema.
- [ ] Refuse to generate a migration on drift unless `--force`.

**Acceptance:** migration behavior matches or exceeds SQLx; concurrent runners are
safe; failed migrations are recoverable; checksums detect edits; integration tests
cover Postgres, MySQL, and SQLite.

---

### WS3 — Schema layer

**Goal:** implement `docs/dsl.md` and make schema changes reviewable.

- [ ] AST (`serde` + `toml`), IR, type parser, expression wrapper.
- [ ] Validation: duplicates, FK targets, type references, partition bounds,
      identifier length, expression safety.
- [ ] Postgres emitter for schemas, extensions, tables, columns, PK/FK/unique/check,
      indexes, comments.
- [ ] `midas schema validate`, `midas schema sql`, `midas schema format`.
- [ ] Diff engine and plan ordering (create → alter → drop).
- [ ] `midas migrate dev --name <name>` with shadow-DB replay.
- [ ] `midas migrate diff --from <source> --to <source>`.
- [ ] `midas db pull` (introspection) and `midas db push`.
- [ ] Rename detection via `was` / `previous_name`.
- [ ] Destructive-change classification and `--accept-data-loss`.
- [ ] Advanced Postgres objects: enums, domains, composites, ranges, sequences,
      views, materialized views, functions, triggers, policies, roles, grants,
      partitions, publications, extended statistics.
- [ ] Raw SQL escape hatch.
- [ ] Round-trip tests: DSL → SQL → apply → introspect → diff is empty.

**Acceptance:** the schema in `sample_migrations/` can be expressed in the DSL;
regenerating SQL from a pulled schema produces an empty diff; destructive changes
require an explicit flag.

---

### WS4 — SQLx interoperability

**Goal:** make Midas the schema/migration engine for SQLx users.

Target SQLx **0.9.x**. Pin the version used in conformance tests.

#### 4.1 Import and export migrations

SQLx 0.9 uses:

- `<version>_<description>.sql` for one-way migrations;
- `<version>_<description>.up.sql` and `.down.sql` for reversible migrations;
- `-- no-transaction` as the first line to opt out of a transaction;
- SHA-384 checksums over the exact SQL content;
- `version` as a positive `i64`.

- [ ] `midas migrate export --format sqlx --out <dir>`:
  - emit `.up.sql` / `.down.sql` when a DOWN exists;
  - preserve version and description;
  - add `-- no-transaction` when required;
  - verify with `sqlx migrate info`.
- [ ] `midas migrate import --from sqlx --path <dir>`:
  - parse both one-way and reversible files;
  - detect `-- no-transaction`;
  - generate Midas’s `-- !UP` / `-- !DOWN` files, or run SQLx files natively.
- [ ] Support the SQLx filename rules: non-`.sql` files ignored; malformed `.sql`
      files are an error by default.

#### 4.2 Adopt the SQLx metadata table

- [ ] `midas migrate baseline --from sqlx` reads `_sqlx_migrations`, matches
      versions to local files, and marks them applied.
- [ ] `midas migrate adopt --table _sqlx_migrations` switches Midas to use the
      SQLx table directly.
- [ ] Honor `sqlx.toml` settings when present:
      custom table name, extra schemas, ignored hashing characters.
- [ ] Match SQLx's dirty semantics (`success = false`).

#### 4.3 Schema-only database for `cargo sqlx prepare`

The simplest, lowest-risk offline story:

- [ ] `midas db create --from-schema <schema.toml>` creates a schema-only database.
- [ ] `midas schema apply --url <url>` applies the DSL without writing migrations.
- [ ] Document: create the schema-only database with Midas, then run
      `cargo sqlx prepare` exactly as before.
- [ ] CI example: service container + `midas schema apply` + `cargo sqlx prepare --check`.

#### 4.4 Native `.sqlx` metadata (later)

- [ ] `midas prepare` scans Rust `sqlx::query!` macros, validates them against the
      DSL IR, and writes `.sqlx/query-*.json`.
- [ ] Conformance test: compare Midas output with `cargo sqlx prepare` for the same
      queries and schema.
- [ ] Treat this as optional until 4.3 is shipped and validated.

**Acceptance:** a SQLx project can `import` or `adopt`, keep all queries unchanged,
run `cargo sqlx prepare` against a Midas-provisioned database, and export back to
SQLx without errors.

---

### WS5 — Code generation

**Goal:** remove hand-written row types and enum mappings.

- [ ] `midas generate --sqlx --out src/db`.
- [ ] Generate:
  - structs per table, with `#[derive(sqlx::FromRow)]`;
  - enums with `#[derive(sqlx::Type)]` and `#[sqlx(type_name = "...")]`;
  - newtypes with `#[sqlx(transparent)]` when the DSL declares a domain/newtype;
  - optional per-table select/insert/update functions (phase 2).
- [ ] Generate code from the DSL IR, not from a live database.
- [ ] Add a test fixture crate that compiles the generated code against SQLx.
- [ ] Support `#[cfg(feature = "sqlx")]` output so non-SQLx users are unaffected.

**Acceptance:** generated code compiles with SQLx and maps every column and enum
correctly; regenerating produces no diff.

---

### WS6 — Quality, CI, and docs

- [ ] Integration tests with Testcontainers for Postgres and MySQL; temp files for
      SQLite.
- [ ] Concurrency test: two `migrate run` processes, one wins, no double-apply.
- [ ] Snapshot tests for generated SQL and DSL errors.
- [ ] Fuzz tests for the migration parser and DSL parser.
- [ ] CI matrix for driver features and MSRV.
- [ ] SQLx conformance job:
  - export migrations, run `sqlx migrate info` and `sqlx migrate run`;
  - import SQLx migrations and baseline;
  - compile generated code against SQLx.
- [ ] Documentation:
  - “Midas + SQLx” guide;
  - migration safety guide;
  - DSL reference (already in `docs/dsl.md`);
  - CLI reference.
- [ ] `midas doctor`: checks connectivity, permissions, shadow DB, metadata state,
      and detects `_sqlx_migrations`.

---

## 5. Milestones

Sizes are indicative, not commitments.

### M0 — Foundation (S/M)

- Workspace, typed errors, feature flags, `Config`, split traits.
- Remove artificial sleeps, implicit SQLite URL, unimplemented commands.
- Fix SQLite multi-statement execution and Postgres drop-database.
- Remove driver `unwrap()` panics.
- CI: feature matrix + MSRV.

**Exit:** no panics; all feature combinations build; CLI behavior unchanged.

### M1 — Migration engine (M)

- Metadata v2, checksums, locks, transactions, dirty state.
- `run`/`info`/`add`/`revert --to`/`baseline`/`repair`/`resolve`/`reset`.
- `--dry-run`, `--target`, `--json`.
- `midas::migrate!` macro.
- Migration linting.
- Concurrency and failure tests.

**Exit:** feature parity with SQLx migrations plus checksums, locking, dirty
recovery, and linting.

### M2 — SQLx interop (M)

- Export/import migrations.
- `baseline --from sqlx` and `adopt --table _sqlx_migrations`.
- `db create --from-schema` + `schema apply` for `cargo sqlx prepare`.
- “Midas + SQLx” guide.
- CI conformance job.

**Exit:** a SQLx project can adopt Midas without changing queries.

### M3 — Schema DSL core (L)

- Parser, IR, validator, type parser, Postgres create emitter.
- `schema validate` / `schema sql` / `schema format`.
- Snapshot tests.

**Exit:** the sample schema round-trips from TOML to SQL.

### M4 — Diff engine (L)

- Diff, plan, UP/DOWN generation.
- `migrate dev`, `migrate diff`.
- Shadow DB, drift detection, destructive flags, rename detection.

**Exit:** diff is idempotent; changes generate correct reversible SQL; drift is
detected before writes.

### M5 — Introspection (L)

- `db pull` for the v1 object set.
- Round-trip: pull → DSL → SQL → diff empty.
- Advanced Postgres objects.

**Exit:** an existing Postgres database can be adopted into the DSL.

### M6 — Code generation and offline metadata (M)

- `generate --sqlx` for structs, enums, newtypes, `FromRow`.
- Optional `midas prepare` for `.sqlx` metadata.
- Compile fixtures.

**Exit:** generated code compiles; `cargo sqlx prepare` flow is documented and
tested.

### M7 — 1.0 (M)

- Stable public API, docs, performance, security review.
- Release notes and migration guide from the current CLI-only workflow.

---

## 6. Concrete file changes

```text
Cargo.toml                         # features, workspace, TLS
src/lib.rs                         # public API surface
src/error.rs                       # typed errors (new)
src/config.rs                      # Config (new)
src/sequel/mod.rs                  # split Driver into Executor/Store/Introspector
src/sequel/postgres.rs             # metadata v2, locks, transactions, drop fix
src/sequel/mysql.rs                # metadata v2, GET_LOCK, no panics
src/sequel/sqlite.rs               # execute_batch, locks, no panics
src/commander.rs                   # remove sleeps, add config, checksums, dirty
src/lookup.rs                      # checksums, SQLx formats, no-transaction
src/schema/                        # AST, IR, types, diff, plan, emit, introspect (new)
src/sqlx/                          # import/export/adopt/prepare interop (new)
src/codegen/                       # Rust/SQLx code generation (new)
src/midas/bin/cli.rs               # migrate/schema/db/generate namespaces
midas-macros/                      # migrate! macro (new)
docs/roadmap.md                    # this document
docs/dsl.md                        # schema DSL design
tests/                             # testcontainers, concurrency, snapshots, interop
```

---

## 7. Risks and decisions

| Risk / decision | Mitigation |
| --- | --- |
| Scope explosion | Hard non-goals; ship M1/M2 before the DSL |
| Async expectations | Document that Midas is sync and CLI/library-first; offer `spawn_blocking` guidance |
| Metadata table compatibility | Mirror SQLx columns; keep old tables; support `_sqlx_migrations` |
| MySQL implicit commits | Document; use `-- no-transaction`; test failure recovery |
| Checksum drift across platforms | Force LF via `.gitattributes`; offer ignored-character config |
| SQLx version churn | Pin a SQLx version for conformance tests; target 0.9.x |
| Introspection fidelity | Conservative normalization; prefer warnings over silent destructive diffs |
| Compile-time query checking | Defer; use `cargo sqlx prepare` against a Midas-provisioned DB |
| Dependency/MSRV growth | Feature flags; no scripting runtime; optional TLS |

---

## 8. Success metrics

1. A SQLx project can adopt Midas in under 30 minutes with no query changes.
2. `midas migrate export --format sqlx` passes `sqlx migrate info` and `sqlx migrate run`.
3. `midas db pull` → DSL → SQL → introspect produces an empty diff.
4. Zero panics in the Postgres/MySQL/SQLite integration suites.
5. Two concurrent `migrate run` processes never double-apply a migration.
6. Edited applied migrations are always detected.
7. Generated `sqlx::FromRow` / `sqlx::Type` code compiles against SQLx 0.9.
8. CI covers every driver feature combination and the MSRV.

---

## 9. Second-pass audit (gaps found)

The first draft covered the strategy but missed several correctness, safety, and
interop details. These are folded into M0–M2; the priority list at the end is the
minimum bar before M1 can be considered complete.

### 9.1 Migration file fidelity

- [ ] **Preserve raw UP/DOWN SQL exactly.** `src/lookup.rs` splits files into lines,
      drops empty lines (`get_content_string!`), and rejoins them. This changes the SQL
      sent to the database and makes SQLx checksum compatibility impossible. Store the
      raw section as `String`/`Vec<u8>`; do not normalize content for execution.
- [ ] **Canonicalize for checksums only, not for execution.** Define one canonical form
      (UTF-8, LF, optional BOM stripping) and compute SHA-384 over it. Document that
      the SQLx-compatible checksum is over the UP script bytes.
- [ ] Add `.gitattributes` with `*.sql text eol=lf` so Windows checkouts do not change
      hashes.
- [ ] Support one-way migrations (no `-- !DOWN`). `revert` must error clearly or require
      `--force`; one-way files must still run.
- [ ] Broaden filename parsing. SQLx accepts any positive `i64` version, not only
      13-digit timestamps, and descriptions may contain hyphens, dots, and spaces.
      Support `<version>_<description>.sql`, `.up.sql`, and `.down.sql`.
- [ ] Fix `create`: it replaces spaces but not hyphens, while the current parser regex
      rejects hyphens. Validate the slug, reject path separators, collapse repeats, and
      keep `create` and `parse_file` in sync.
- [ ] Detect duplicate versions, empty descriptions, and non-positive versions.
- [ ] Handle UTF-8 BOM, no trailing newline, and mixed line endings deterministically.

### 9.2 Metadata and runner parity

- [ ] Add SQLx semantics that were missing from the first draft:
      `ignore_missing`, `locking = false` (CockroachDB), `create_schemas`, `skip`
      (baseline), `run_to` (target), and `undo` (revert-to).
- [ ] Store `description`, `installed_on`, `success`, `checksum`, and
      `execution_time` (nanoseconds, matching SQLx) in addition to `version`.
- [ ] Upgrade existing `midas.__schema_migrations` / `__schema_migrations` tables in
      place; preserve already-applied versions and never re-run them.
- [ ] Treat `success = false` as dirty; refuse to run until `migrate resolve`.
- [ ] Add `--ignore-checksum`, `--repair`, `--baseline`, and `--skip` equivalents with
      explicit warnings.
- [ ] Define out-of-order migration policy and missing-version detection.
- [ ] Release advisory locks on panic/crash with an RAII guard; choose session-level vs
      transaction-level locking deliberately.
- [ ] Fix `down` / `revert` / `redo` semantics. The current `MIGRATIONS_SKIP_LAST` and
      `completed_migrations.first()` logic is undocumented and likely wrong; replace it
      with explicit flags and tests.

### 9.3 Connections, TLS, and operational safety

- [ ] Postgres currently uses `NoTls`. Add TLS (`rustls` preferred, `native-tls`
      optional), SSL modes, connect timeout, `application_name`, `search_path`,
      statement timeout, and lock timeout.
- [ ] MySQL needs TLS configuration and a `GET_LOCK` timeout.
- [ ] SQLite needs a busy timeout, optional WAL mode, and robust URI parsing.
- [ ] PgBouncer transaction pooling: use a dedicated connection for advisory locks and
      migrations; document the limitation.
- [ ] Detect or document transaction-incompatible statements:
      `CREATE INDEX CONCURRENTLY`, `VACUUM`, `CREATE DATABASE`, and older
      `ALTER TYPE ... ADD VALUE`.
- [ ] Remove `grant all on schema midas to public` from
      `src/sequel/postgres.rs`; use least privilege for the metadata schema.
- [ ] Fix database create/drop: connect to a maintenance database (`postgres` or
      `template1`), support force-drop of active connections, and add
      `midas db create` / `midas db drop` parity with SQLx.
- [ ] Redact passwords and connection strings from logs and error messages.
- [ ] Fix cross-platform SQLite path handling (Windows drive letters are not handled by
      the current `starts_with("/")` / `starts_with(".")` logic).

### 9.4 CLI and backward compatibility

- [ ] `seed` is registered in the CLI but has no match arm in `midas_entry`; implement
      it or remove it from help. The same applies to `setup` in the README.
- [ ] `faker` and `update` are `unimplemented!()` and will panic. Remove them from help
      until implemented, or implement `update` safely.
- [ ] `drop` has no confirmation and is immediately destructive; require `--force` and
      an interactive confirmation.
- [ ] `init` overwrites `.env.midas` and implicitly creates a SQLite database. Replace
      it with `midas.toml` + `schema.toml` scaffolding, refuse to overwrite, and do not
      create a database unless asked.
- [ ] Remove or repurpose `.migrations-state`. It is created but unused; `.gitignore`
      also has a typo (`.migration-state` vs `.migrations-state`).
- [ ] Remove the hidden `MIGRATIONS_SKIP_LAST` environment behavior or replace it with
      an explicit flag.
- [ ] Add `--json`, `--quiet`, `--verbose`, `--no-color` / `NO_COLOR`, and `--yes`.
- [ ] Define exit codes (success, user error, database error, dirty state, drift).
- [ ] Show checksum, applied time, and success/dirty state in `status` and `list`.
- [ ] Add `midas doctor` for connectivity, permissions, shadow DB, metadata state, and
      SQLx detection.
- [ ] Define config precedence: CLI > environment > `midas.toml` > defaults.
- [ ] Update `README.md` command list and remove stale claims (MSRV 1.37, `setup`,
      unimplemented commands).

### 9.5 CockroachDB, MariaDB, and SQLite

- [ ] CockroachDB is claimed as supported but does not support Postgres advisory locks.
      Either support it with `locking = false` plus DDL tests, or downgrade the claim.
- [ ] MariaDB: verify `GET_LOCK`, implicit DDL commits, and `DROP DATABASE` behavior.
- [ ] SQLite: no schemas, no RLS, limited `ALTER`; the DSL and diff engine must return
      clear capability errors instead of emitting invalid SQL.
- [ ] Add a dialect capability matrix and tests for each supported engine.

### 9.6 Schema layer

- [ ] Decide whether data migrations/backfills and `seed.toml` ship in 1.0; the DSL doc
      mentions them, but the roadmap did not.
- [ ] `schema format`: deterministic TOML formatting and stable ordering.
- [ ] JSON Schema for editor validation and `taplo` integration.
- [ ] DSL versioning (`version = 1`) and a backward-compatibility policy.
- [ ] Multi-file merge rules, duplicate detection, and clear file/line errors.
- [ ] Ownership, grants, comments, and extension ownership in diff and introspection.
- [ ] Expression normalization for defaults/checks, and `USING` expressions for type
      changes that need a cast.
- [ ] Enum value removal strategy, partition attach/detach, concurrent indexes, and
      `NOT VALID` constraints.
- [ ] Introspection warnings for unsupported objects, plus `[[ignore]]` and managed
      schemas.
- [ ] `db push` production guard (confirmation + `--force`).
- [ ] Dialect capability errors for MySQL and SQLite features that Postgres supports.

### 9.7 SQLx interop and codegen

- [ ] Read `sqlx.toml` when present: custom metadata table name, extra schemas, and
      ignored hashing characters.
- [ ] Support `_sqlx_migrations` semantics fully: `success = false` dirty handling,
      `ignore_missing`, `locking = false`, and `create_schemas`.
- [ ] Verify checksums byte-for-byte: SHA-384, LF normalization, BOM handling, and the
      exact UP script content.
- [ ] Export/import reversible `.up.sql` / `.down.sql` and `-- no-transaction` exactly.
- [ ] Map `run_to`, `skip`, and `undo` to `run --target`, `baseline`, and `revert --to`.
- [ ] `cargo sqlx prepare` flow: test `--check`, `--workspace`,
      `--all-targets --all-features`, and `SQLX_OFFLINE=true` against a
      Midas-provisioned schema-only database.
- [ ] Codegen type mapping: nullable columns -> `Option<T>`, arrays, enums, domains,
      JSONB, numeric, UUID, chrono/time; warn that SQLx has no native composite-type
      support.
- [ ] Generate `#[sqlx(rename)]`, `#[sqlx(default)]`, `#[sqlx(transparent)]`, and
      `#[sqlx(type_name)]` correctly.
- [ ] Pin SQLx for `.sqlx` metadata conformance tests; the format is not a public
      stability guarantee.
- [ ] `midas prepare` must handle workspaces, feature flags, and queries inside tests.

### 9.8 CI, release, and security

- [ ] Add database-backed integration tests (Postgres + MySQL via Testcontainers,
      SQLite via temp files). Current CI runs no database.
- [ ] Add concurrency tests for advisory locks and double-apply prevention.
- [ ] Add an MSRV job and fix the stale MSRV claim in `README.md`.
- [ ] Add a feature-combination matrix; ensure `--all-features` does not enable
      conflicting TLS backends.
- [ ] Add a SQLx conformance job (import, export, baseline, `sqlx migrate info/run`,
      `cargo sqlx prepare --check`).
- [ ] Add a Windows line-ending/checksum test.
- [ ] Re-verify cross-compilation after making OpenSSL optional and adding TLS
      backends.
- [ ] Test Docker, distroless, `.deb`, and `install.sh` outputs.
- [ ] License audit for new dependencies (`thiserror`, `sha2`, `crc`, `testcontainers`,
      `rustls`).
- [ ] Document the trust model for migrations and `[[raw]]`: they execute arbitrary SQL
      by design; do not run untrusted schema files.
- [ ] Security review: path traversal in `create`, symlinked migration dirs, secret
      redaction, `search_path` hijacking, and `SECURITY DEFINER` functions.
- [ ] Dependency upkeep: replace abandoned `dotenv` with `dotenvy`, upgrade
      `tracing-subscriber` 0.2, remove `rand` if only used for artificial sleeps, and
      evaluate the `mk_ext_*` crates.

### 9.9 Minimum bar before calling M1 complete

1. Raw SQL is preserved and checksums match SQLx byte-for-byte.
2. Filename parsing supports SQLx versions and reversible/one-way files.
3. Metadata v2 with `success`, `checksum`, `installed_on`, `execution_time`.
4. Advisory locks, dirty-state handling, and transactional DDL verified.
5. TLS, timeouts, and secret redaction implemented.
6. Dead or panic-prone CLI paths removed or implemented.
7. Database-backed CI tests, concurrency tests, and SQLx conformance tests pass.
8. `.gitattributes` and line-ending normalization are in place.
