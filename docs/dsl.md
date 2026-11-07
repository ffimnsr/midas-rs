# Midas DSL — Design Proposal

> Status: proposal, not implemented.
>
> Goal: a declarative, Prisma-like schema language that compiles to Postgres DDL and
> integrates with Midas' existing `-- !UP` / `-- !DOWN` migration workflow.

---

## 1. Summary

Midas should describe the **desired state** of a database in TOML, then let the tool
compute the difference against the current managed state and emit migration SQL.

The recommended shape is:

- **TOML as the authoring format.** It matches the roadmap, is already easy to support
  in Rust via `serde`, gives free syntax errors, and is friendly to editors and CI.
- **Objects are explicit.** No pluralization, no implicit naming, no magic relation
  fields. A table is a table; a foreign key is a foreign key.
- **SQL expressions are strings with a marker.** `default = { expr = "now()" }` keeps
  the line between "value" and "raw SQL" unambiguous.
- **A raw escape hatch for everything the DSL does not model.** Postgres is enormous;
  a DSL that tries to model every exotic object becomes unusable. `[[raw]]` blocks
  are first-class and dialect-gated.
- **A compiler pipeline:** parse → resolve → validate → normalize → diff → plan →
  emit. The IR is dialect-neutral so MySQL/SQLite/CockroachDB can be added later.

The DSL is *not* an ORM client. It does not generate query APIs, typed row structs, or
runtime navigation. It generates and verifies schema DDL.

---

## 2. Goals and non-goals

### Goals

1. Express the schema of a typical production Postgres application without raw SQL.
2. Be readable in code review: one obvious way to describe each object.
3. Produce deterministic SQL and deterministic migration diffs.
4. Support safe, reviewable migrations, including generated `DOWN` statements.
5. Cover Postgres-specific features used by real apps: extensions, enums, composite
   types, generated columns, partial/expression indexes, RLS policies, triggers,
   functions, roles and grants, partitioning.
6. Keep the door open for MySQL, MariaDB, SQLite and CockroachDB.
7. Allow gradual adoption: introspect an existing database into the DSL (`db pull`),
   or write the DSL first and generate an initial migration.

### Non-goals

1. Runtime query building or an ORM.
2. Modeling every Postgres catalog feature declaratively.
3. Replacing hand-written SQL migrations. Hand-written migrations remain valid.
4. Managing data migrations declaratively. Data changes belong in `[[raw]]` blocks,
   `[[migration]]` files, or seeds.
5. Cross-database logical replication as a first-class feature in v1.
6. Automatic destructive changes without an explicit flag.

---

## 3. Why TOML (and what we give up)

### Chosen: TOML

The roadmap already specifies TOML. TOML gives us:

- a mature, fast parser (`toml` crate);
- exact line/column diagnostics;
- no custom lexer/parser to maintain;
- editor support and schema tooling;
- easy serialization for introspection and state snapshots.

### Trade-off

A Prisma-style line DSL is more readable for relations and attributes:

```prisma
model User {
  id    BigInt @id @default(autoincrement())
  email String @unique
  org   Org    @relation(fields: [orgId], references: [id])
  orgId BigInt
}
```

TOML cannot express that without a custom mini-language inside strings. We accept
slightly more structure in exchange for a real parser and predictable merging.

### Important TOML constraint

A table and its nested objects must be **contiguous**:

```toml
[[table]]
name = "app.users"

[[table.column]]
name = "id"
type = "bigint"

[[table.column]]
name = "email"
type = "text"

[[table.index]]
name = "users_email_idx"
columns = ["email"]
```

You cannot start `[[table]]` for `jobs`, then go back and add a column to `users`.
For large schemas, split tables across files in a schema directory (Section 4.2).

### Possible future addition

A `.midas`/`.prisma`-like front-end could compile to the same IR later. The IR should
not assume TOML, so this remains possible. Do not build two authoring syntaxes in v1.

---

## 4. Project layout

### 4.1 Configuration file

Add `midas.toml` at the project root:

```toml
# midas.toml
[project]
dialect = "postgres"
default_schema = "public"
schema = "schema.toml"              # single-file mode
# schema_dir = "schema"             # directory mode (mutually exclusive with schema)

migrations_dir = "migrations"
shadow_database_url_env = "SHADOW_DATABASE_URL"

[managed]
schemas = ["public", "app"]
ignore_objects = ["public._legacy_*"]
```

Existing CLI flags `--database` / `DATABASE_URL` and `--source` / `MIGRATIONS_DIR`
continue to work and override the file.

### 4.2 Schema files

Two modes:

1. **Single file:** `schema.toml`.
2. **Directory:** `schema/*.toml`, merged in filename order.

Each file may only contribute to a top-level object kind. Duplicate object names are
an error, unless the same file is included twice (which is also an error). A good
convention:

```text
schema/
  00_project.toml       # version, project, schemas, extensions
  10_enums.toml
  20_tables/
    users.toml
    organizations.toml
  30_views.toml
  40_functions.sql.toml # or functions.toml
  50_roles_grants.toml
  90_raw.toml
```

### 4.3 Migrations

Generated migrations use the existing format:

```sql
-- midas:generated
-- midas:schema-hash 4f2c...
-- !UP

create table "app"."users" ( ... );

-- !DOWN

drop table "app"."users";
```

The extra comments are optional metadata; the existing parser ignores everything
before `-- !UP`.

---

## 5. Core concepts

### 5.1 Desired state

The DSL describes what the schema **should be**, not the steps to get there.
`migrate dev` computes the steps.

### 5.2 Managed schemas

Only objects in `[managed].schemas` are considered for diffing. System schemas,
extension-owned objects, and `midas`'s own bookkeeping schema are ignored.
This prevents "adopting" unrelated objects from a shared database.

### 5.3 Object identity

Every object has a fully qualified name:

- schema-scoped: `schema.name` (`app.users`, `app.job_status`);
- database-scoped: plain name (`app_reader`, `pgcrypto`).

In TOML, prefer explicit fields:

```toml
[[table]]
schema = "app"
name = "users"
```

A dotted `name` (`name = "app.users"`) is also accepted when `schema` is omitted.
Identifiers that actually contain a dot should use the explicit `schema` field and
will be quoted.

### 5.4 Naming

The DSL never pluralizes or camel-cases names. If `name` is omitted for a derived
object (constraint, index), Midas generates a deterministic Postgres-style name:

| Object | Default |
| --- | --- |
| primary key | `<table>_pkey` |
| unique constraint | `<table>_<columns>_key` |
| foreign key | `<table>_<columns>_fkey` |
| check constraint | `<table>_<column>_check` |
| index | `<table>_<columns>_idx` |
| sequence | `<table>_<column>_seq` |

Names longer than 63 bytes are truncated and suffixed with an 8-character hash of the
full name, so they stay deterministic and collision-resistant.

### 5.5 Quoting

Generated SQL always double-quotes identifiers. This preserves case, avoids reserved
words, and makes output deterministic. Strings are emitted as escaped single-quoted
literals, never interpolated blindly.

---

## 6. Syntax reference

### 6.1 Top-level file

```toml
version = 1

[project]
dialect = "postgres"
default_schema = "public"

[[schema]]
name = "app"

[[extension]]
name = "pgcrypto"
schema = "public"
version = "1.3"

# ... object arrays ...
```

Supported top-level object arrays in v1:

| Key | Object |
| --- | --- |
| `[[schema]]` | schema |
| `[[extension]]` | extension |
| `[[enum]]` | enum type |
| `[[domain]]` | domain type |
| `[[composite]]` | composite type |
| `[[range]]` | range type |
| `[[collation]]` | collation |
| `[[sequence]]` | sequence |
| `[[table]]` | table |
| `[[view]]` | view / materialized view |
| `[[function]]` | function |
| `[[procedure]]` | procedure |
| `[[trigger]]` | trigger |
| `[[policy]]` | row-level security policy |
| `[[role]]` | role / group |
| `[[grant]]` | privilege grant |
| `[[default_privilege]]` | default privilege |
| `[[publication]]` | logical replication publication |
| `[[subscription]]` | logical replication subscription |
| `[[statistics]]` | extended statistics |
| `[[raw]]` | raw SQL escape hatch |
| `[[ignore]]` | managed-object exclusion |

Unknown fields are errors (`deny_unknown_fields`) so typos surface immediately.
Unknown top-level keys are also errors.

### 6.2 Types

`type` is a string in a small, unambiguous grammar. Each line below is an
independent example, not one TOML document:

```text
type           := base_type array_suffix?
array_suffix   := "[]"
base_type      := builtin | qualified_name
builtin        := numeric_type | string_type | datetime_type | ...
qualified_name := ident ( "." ident )?
```

Examples (independent lines):

```toml
type = "bigint"
type = "numeric(12, 2)"
type = "varchar(300)"
type = "text"
type = "boolean"
type = "timestamp(3) with time zone"
type = "timestamptz(3)"                 # alias, normalized
type = "date"
type = "time without time zone"
type = "interval day to second"
type = "uuid"
type = "jsonb"
type = "bytea"
type = "inet"
type = "cidr"
type = "macaddr8"
type = "tsvector"
type = "int8range"
type = "app.job_status"                 # enum, domain, composite or range
type = "app.job_status[]"               # array
type = "public.citext"                  # extension type
```

Normalization rules:

- aliases map to canonical names for diffing (`int` → `integer`, `bool` → `boolean`,
  `timestamptz` → `timestamp with time zone`, `serial`/`bigserial` → identity
  recommendation, but accepted for compatibility);
- numeric precision/scale is preserved;
- array dimensions are dropped (Postgres ignores declared lengths);
- unknown qualified types are allowed if they exist in the database or in the DSL;
- `pg_catalog` is implied for unqualified built-ins.

### 6.3 Defaults and expressions

A default is either a typed literal or an explicit SQL expression. Each line
below is an independent example:

```toml
default = "draft"                                  # -> 'draft'
default = 1                                        # -> 1
default = 1.5                                      # -> 1.5
default = true                                     # -> true
default = ["a", "b"]                               # -> array['a', 'b']
default = { expr = "now()" }                       # -> now()
default = { expr = "gen_random_uuid()" }           # -> gen_random_uuid()
default = { expr = "row('5','10','USD','hourly')::app.salary_detail" }
default = { expr = "null" }                        # explicit NULL
```

Rules:

- `expr` fields are trusted developer input, not user input.
- Expressions in `default`, `check`, `using`, `with_check`, `where`, and `generated`
  may not contain a semicolon. Function bodies and `[[raw]]` blocks are exempt.
- A literal table or array is serialized safely; it is never spliced verbatim.
- To force a cast, use `expr`.

### 6.4 Schemas

```toml
[[schema]]
name = "app"
authorization = "app_owner"
comment = "Application schema"
```

### 6.5 Extensions

```toml
[[extension]]
name = "pgcrypto"
schema = "public"
version = "1.3"
```

`version` is optional; omitting it means "whatever the server provides". Diffing only
emits `ALTER EXTENSION ... UPDATE` when a version is explicitly requested.

### 6.6 Enums

```toml
[[enum]]
schema = "app"
name = "job_status"
values = ["draft", "published", "closed", "archived"]
```

Postgres can add enum values but cannot remove or reorder them. Diff behavior:

- added value → `alter type ... add value`;
- removed value → not directly expressible; require an explicit
  `on_remove_value = "recreate"` (type swap) or refuse and point to a `[[raw]]` block;
- reorder → no-op for Postgres (ordering is by declaration), warn.

### 6.7 Domains

```toml
[[domain]]
schema = "app"
name = "email"
type = "citext"
not_null = true
default = { expr = "null" }
collation = "app.case_insensitive"

[[domain.constraint]]
name = "app_email_format"
expr = "value ~ '^[^@[:space:]]+@[^@[:space:]]+$'"
```

### 6.8 Composite types

```toml
[[composite]]
schema = "app"
name = "salary_detail"

[[composite.field]]
name = "upper_limit"
type = "text"

[[composite.field]]
name = "lower_limit"
type = "text"

[[composite.field]]
name = "currency"
type = "varchar(10)"

[[composite.field]]
name = "timeframe"
type = "app.salary_timeframe"
```

### 6.9 Range types

```toml
[[range]]
schema = "app"
name = "price_range"
subtype = "numeric"
subtype_opclass = "numeric_ops"
collation = "public.default"
canonical = "app.price_range_canonical"
subtype_diff = "app.numeric_diff"
multirange_type_name = "app.price_multirange"
```

### 6.10 Collations

```toml
[[collation]]
schema = "app"
name = "natural"
locale = "en-US-x-icu"
provider = "icu"          # libc | icu | builtin (PG 17+)
deterministic = true
```

### 6.11 Tables

```toml
[[table]]
schema = "app"
name = "users"
comment = "Application users"
primary_key = ["id"]
unlogged = false
row_level_security = true
force_row_level_security = false
replica_identity = "default"      # default | nothing | full | index
tablespace = "fast"
access_method = "heap"
options = { fillfactor = 90, autovacuum_enabled = true }
inherits = ["app.base_entity"]

[[table.column]]
name = "id"
type = "bigint"
nullable = false
identity = { always = false, start = 10000, increment = 1 }

[[table.column]]
name = "public_id"
type = "varchar(90)"
nullable = false
unique = true

[[table.column]]
name = "zitadel_id"
type = "uuid"
nullable = false
unique = { nulls_not_distinct = true }

[[table.column]]
name = "created_at"
type = "timestamptz"
nullable = false
default = { expr = "current_timestamp" }

[[table.column]]
name = "updated_at"
type = "timestamptz"
nullable = false
default = { expr = "current_timestamp" }

[[table.column]]
name = "fts"
type = "tsvector"
generated = { expr = "to_tsvector('english', title || ' ' || description)", stored = true }

[[table.constraint]]
name = "users_public_id_length"
type = "check"
expr = "char_length(public_id) >= 3"
```

#### Column fields

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `name` | string | required | |
| `type` | string | required | see Section 6.2 |
| `nullable` | bool | `true` | |
| `default` | literal / `{ expr }` | none | |
| `primary_key` | bool | `false` | shorthand; prefer table-level `primary_key` |
| `unique` | bool / table | `false` | shorthand constraint |
| `index` | bool / table | `false` | shorthand index |
| `references` | string / table | none | shorthand foreign key |
| `check` | string / array / table | none | shorthand check |
| `identity` | bool / table | `false` | `{ always, start, increment, min, max, cache, cycle }` |
| `generated` | table | none | `{ expr, stored = true }` |
| `collation` | string | none | qualified collation name |
| `compression` | string | none | `pglz` / `lz4` |
| `storage` | string | none | `plain` / `external` / `extended` / `main` |
| `comment` | string | none | |
| `was` | string | none | previous name for rename detection |

`was` is the explicit rename mechanism. Without it, a rename is treated as
drop + add and is flagged destructive.

#### Identity and sequences

Prefer identity columns (these are two alternatives, not one document):

```toml
identity = true
# or
identity = { always = true, start = 10000 }
```

`serial` and `bigserial` are accepted for compatibility but normalize to an identity
recommendation. A standalone sequence can still be declared with `[[sequence]]`.

### 6.12 Constraints

Table-level constraints are explicit and can express anything:

```toml
[[table.constraint]]
name = "users_org_fk"
type = "foreign_key"
columns = ["org_id"]
references = { schema = "app", table = "orgs", columns = ["id"] }
on_delete = "cascade"          # no action | restrict | cascade | set null | set default
on_update = "no action"
match = "simple"               # simple | full | partial
deferrable = false
initially_deferred = false
not_valid = false

[[table.constraint]]
name = "orgs_slug_key"
type = "unique"
columns = ["slug"]
nulls_not_distinct = false

[[table.constraint]]
name = "bookings_no_overlap"
type = "exclude"
method = "gist"
elements = [
  { column = "room_id", operator = "=" },
  { column = "period", operator = "&&" },
]
where = "status = 'confirmed'"

[[table.constraint]]
name = "users_public_id_length"
type = "check"
expr = "char_length(public_id) >= 3"
no_inherit = false
not_valid = false
```

Shorthand forms on a column:

```toml
[[table.column]]
name = "org_id"
type = "bigint"
references = { schema = "app", table = "orgs", on_delete = "cascade" }

[[table.column]]
name = "email"
type = "text"
unique = true

[[table.column]]
name = "age"
type = "integer"
check = "age >= 0"
```

For composite foreign keys, use the table-level form.

### 6.13 Indexes

Shorthand:

```toml
[[table.column]]
name = "email"
type = "text"
index = true

[[table.column]]
name = "external_id"
type = "uuid"
index = { unique = true, method = "hash", name = "users_external_id_idx" }
```

Full form:

```toml
[[table.index]]
name = "users_lookup_idx"
method = "btree"                 # btree | hash | gist | spgist | gin | brin
unique = false
columns = [
  { column = "org_id" },
  { column = "created_at", order = "desc", nulls = "last" },
  { expr = "lower(email)", opclass = "text_pattern_ops" },
]
include = ["status"]
where = "deleted_at is null"
with = { fillfactor = 90, fastupdate = false }
tablespace = "fast"
concurrent = false
```

Rules:

- A column entry is either a string (`"email"`) or an inline table with exactly one of
  `column` or `expr`.
- `concurrent = true` emits `CREATE INDEX CONCURRENTLY`; such a migration cannot run
  inside a transaction. The migration header gets `-- midas:no-transaction`.
- Indexes on partitioned tables, `ONLY`, and `NULLS NOT DISTINCT` are supported through
  the same fields.
- `[[view.index]]` is valid for materialized views.

### 6.14 Partitioning and inheritance

```toml
[[table]]
schema = "app"
name = "events"
primary_key = ["id", "created_at"]
partition_by = { strategy = "range", columns = ["created_at"] }

[[table.column]]
name = "id"
type = "bigint"
nullable = false

[[table.column]]
name = "created_at"
type = "timestamptz"
nullable = false

[[table.partition]]
name = "events_2025"
bound = { from = "2025-01-01", to = "2026-01-01" }

[[table.partition]]
name = "events_2026"
bound = { from = "2026-01-01", to = "2027-01-01" }

[[table.partition]]
name = "events_default"
bound = { default = true }
```

Supported strategies:

| Strategy | `bound` |
| --- | --- |
| `range` | `{ from = "...", to = "..." }`, `{ min = true }`, `{ max = true }` |
| `list` | `{ values = ["a", "b"] }` |
| `hash` | `{ modulus = 4, remainder = 0 }` |

`partition_by` also accepts multiple columns. Inheritance uses `inherits = [...]`.
Partitions can be detached/attached via diff, but this is treated as destructive by
default.

### 6.15 Views and materialized views

```toml
[[view]]
schema = "app"
name = "active_users"
definition = "select id, public_id from app.users where deleted_at is null"
options = { security_barrier = true, security_invoker = true }
check_option = "cascaded"        # local | cascaded | none

[[view]]
schema = "app"
name = "user_stats"
materialized = true
definition = "select org_id, count(*) as jobs from app.jobs group by org_id"
with_data = true
options = { fillfactor = 90 }

[[view.index]]
name = "user_stats_org_idx"
columns = ["org_id"]
```

Diff behavior:

- compatible definition change → `create or replace view`;
- incompatible change (column set/type change) → drop and recreate;
- materialized views → drop/recreate for definition changes; `refresh materialized view`
  is a separate `[[raw]]` operation.

### 6.16 Functions and procedures

```toml
[[function]]
schema = "app"
name = "is_org_member"
args = [
  { name = "org_id", type = "bigint" },
  { name = "user_id", type = "uuid", default = { expr = "app.current_user_id()" } },
]
returns = "boolean"
language = "sql"                 # sql | plpgsql | ...
body = """
select exists (
  select 1
  from app.organization_members m
  where m.organization_id = org_id
    and m.user_id = user_id
)
"""
volatility = "stable"            # volatile | stable | immutable
security = "definer"             # invoker | definer
strict = false
leakproof = false
parallel = "unsafe"              # unsafe | restricted | safe
cost = 100
rows = 1000
set = ["search_path = app, public"]
comment = "Membership check"
```

Procedures use the same shape with `[[procedure]]`, `language`, `body`, and no
`returns`.

Overloading: functions are identified by `(schema, name, argument types)`. Renaming an
argument does not recreate the function.

### 6.17 Triggers

```toml
[[trigger]]
schema = "app"
name = "users_touch_updated_at"
table = "users"
timing = "before"                # before | after | instead of
events = ["update"]              # insert | delete | truncate | update
for_each = "row"                 # row | statement
when = "old.* is distinct from new.*"
execute = { function = "moddatetime", args = ["updated_at"] }

[[trigger]]
schema = "app"
name = "audit_events"
table = "events"
timing = "after"
events = ["insert", "update"]
for_each = "statement"
referencing = { old_table = "old_rows", new_table = "new_rows" }
execute = { function = "app.audit_changes", args = [] }
```

Constraint triggers:

```toml
[[trigger]]
schema = "app"
name = "bookings_check"
table = "bookings"
timing = "after"
events = ["insert", "update"]
for_each = "row"
constraint = { deferrable = true, initially_deferred = false }
execute = { function = "app.check_booking", args = [] }
```

### 6.18 Row-level security and policies

Enable RLS at the table level:

```toml
[[table]]
schema = "app"
name = "jobs"
row_level_security = true
force_row_level_security = false
```

Policies:

```toml
[[policy]]
schema = "app"
name = "jobs_are_publicly_readable"
table = "jobs"
command = "select"               # all | select | insert | update | delete
permissive = true
roles = ["public"]
using = "true"

[[policy]]
schema = "app"
name = "job_owner_can_write"
table = "jobs"
command = "all"
roles = ["authenticated"]
using = "app.is_org_member(organization_id)"
with_check = "app.is_org_member(organization_id)"
```

`using` and `with_check` are raw SQL expressions. If omitted, they are omitted from
the emitted policy.

### 6.19 Roles, grants, and default privileges

```toml
[[role]]
name = "app_reader"
login = false
inherit = true
superuser = false
createdb = false
createrole = false
replication = false
connection_limit = -1
# password = { env = "APP_READER_PASSWORD" }   # prefer env over literal
member_of = ["app_base"]

[[role]]
name = "app_writer"
login = false
inherit = true
member_of = ["app_reader"]

[[grant]]
on = "schema app"
to = ["app_reader", "app_writer"]
privileges = ["usage"]

[[grant]]
on = "table app.jobs"
to = ["app_reader"]
privileges = ["select"]

[[grant]]
on = "table app.jobs"
to = ["app_writer"]
privileges = ["select", "insert", "update", "delete"]

[[grant]]
on = "table app.users"
to = ["app_reader"]
columns = ["id", "public_id", "created_at"]
privileges = ["select"]

[[default_privilege]]
in_schema = "app"
on_type = "tables"               # tables | sequences | functions | types | schemas
to = ["app_reader"]
privileges = ["select"]
```

`on` accepts the Postgres object grammar:

```text
on = "database mydb"
on = "schema app"
on = "table app.jobs"
on = "sequence app.invoice_seq"
on = "function app.is_org_member(bigint, uuid)"
on = "type app.job_status"
on = "all tables in schema app"
```

Roles are global. If you manage roles in the DSL, be careful: Midas should not drop
roles by default because they may own objects. `[[role]]` creates/alters; dropping
requires an explicit `[[raw]]` or `state = "absent"` with `--accept-data-loss`.

### 6.20 Sequences

```toml
[[sequence]]
schema = "app"
name = "invoice_number_seq"
start = 1000
increment = 1
min = 1
max = 9223372036854775807
cache = 1
cycle = false
owned_by = { schema = "app", table = "invoices", column = "number" }
```

Identity columns are preferred; sequences remain for shared/legacy sequences.

### 6.21 Publications and subscriptions

```toml
[[publication]]
name = "app_publication"
all_tables = false
tables = ["app.jobs", "app.organizations"]
publish = ["insert", "update", "delete"]
publish_via_partition_root = false

[[subscription]]
name = "app_subscription"
connection = { env = "REPLICA_DATABASE_URL" }
publications = ["app_publication"]
enabled = true
```

Subscriptions contain credentials and are environment-specific. Prefer declaring them
in `[[raw]]` or applying them with a dedicated command. For v1, support introspection
and raw DDL, not automatic `db push`.

### 6.22 Extended statistics

```toml
[[statistics]]
schema = "app"
name = "users_dependencies"
table = "users"
kinds = ["dependencies", "ndistinct"]
columns = ["email", "org_id"]
```

### 6.23 Raw SQL escape hatch

Anything not modeled goes here:

```toml
[[raw]]
name = "postgrest_schema_reload"
dialects = ["postgres"]
transaction = false
order = 100
up = """
create event trigger pgrst_ddl_watch
  on ddl_command_end
  execute procedure pgrst_ddl_watch();
"""

down = """
drop event trigger if exists pgrst_ddl_watch;
"""
```

Rules:

- `up` is required; `down` is optional but strongly recommended.
- `dialects` filters emission; omit for all dialects.
- `transaction = false` emits `-- midas:no-transaction` and makes the runner execute
  outside a transaction.
- `order` controls placement among generated changes. Use it sparingly; generated
  changes are already dependency-ordered.
- Raw blocks are not diffed. They are applied in order whenever the containing
  migration is applied, and their `down` runs on revert.
- Prefer raw blocks over trying to make the DSL model something exotic.

### 6.24 Ignore rules

```toml
[[ignore]]
object = "table"
schema = "public"
name = "_legacy_*"
reason = "owned by another tool"
```

Ignored objects are introspected but excluded from diffing. Glob matching is
shell-style. This is the escape hatch for drift and shared databases.

### 6.25 Seeds and faker (separate file)

Seeds are data, not schema; keep them in `seed.toml`:

```toml
# seed.toml
[[seed]]
table = "app.countries"
rows = [
  { id = 1, code = "US", name = "United States" },
  { id = 2, code = "GB", name = "United Kingdom" },
]

[[seed]]
table = "app.users"
count = 100
faker = { public_id = "uuid", email = "safe_email", created_at = "past_datetime" }
```

The `faker` command can reuse this file. This is out of scope for the schema
compiler but fits the existing roadmap.

---

## 7. Complete example

This mirrors the kind of schema already present in `sample_migrations/`.

```toml
version = 1

[project]
dialect = "postgres"
default_schema = "public"

[[schema]]
name = "app"

[[extension]]
name = "pgcrypto"

[[extension]]
name = "moddatetime"

[[enum]]
schema = "app"
name = "job_status"
values = ["draft", "published", "closed", "archived"]

[[enum]]
schema = "app"
name = "salary_timeframe"
values = ["hourly", "daily", "weekly", "semi_monthly", "monthly", "quarterly", "annually"]

[[composite]]
schema = "app"
name = "salary_detail"

[[composite.field]]
name = "upper_limit"
type = "text"

[[composite.field]]
name = "lower_limit"
type = "text"

[[composite.field]]
name = "currency"
type = "varchar(10)"

[[composite.field]]
name = "timeframe"
type = "app.salary_timeframe"

[[table]]
schema = "app"
name = "users"
comment = "Application users"
primary_key = ["id"]
row_level_security = true

[[table.column]]
name = "id"
type = "bigint"
nullable = false
identity = { start = 10000 }

[[table.column]]
name = "public_id"
type = "varchar(90)"
nullable = false
unique = true

[[table.column]]
name = "zitadel_id"
type = "uuid"
nullable = false
unique = true

[[table.column]]
name = "created_at"
type = "timestamptz"
nullable = false
default = { expr = "current_timestamp" }

[[table.column]]
name = "updated_at"
type = "timestamptz"
nullable = false
default = { expr = "current_timestamp" }

[[table.constraint]]
name = "users_public_id_length"
type = "check"
expr = "char_length(public_id) >= 3"

[[trigger]]
schema = "app"
name = "users_touch_updated_at"
table = "users"
timing = "before"
events = ["update"]
for_each = "row"
execute = { function = "moddatetime", args = ["updated_at"] }

[[table]]
schema = "app"
name = "organizations"
primary_key = ["id"]
row_level_security = true

[[table.column]]
name = "id"
type = "bigint"
nullable = false
identity = true

[[table.column]]
name = "owner_id"
type = "bigint"
nullable = false
references = { schema = "app", table = "users", on_delete = "cascade" }

[[table.column]]
name = "name"
type = "text"
nullable = false

[[table]]
schema = "app"
name = "jobs"
primary_key = ["id"]
row_level_security = true

[[table.column]]
name = "id"
type = "varchar(24)"
nullable = false

[[table.column]]
name = "organization_id"
type = "bigint"
references = { schema = "app", table = "organizations", on_delete = "cascade" }

[[table.column]]
name = "title"
type = "varchar(300)"
nullable = false

[[table.column]]
name = "description"
type = "text"
nullable = false

[[table.column]]
name = "status"
type = "app.job_status"
nullable = false
default = "draft"

[[table.column]]
name = "salary"
type = "app.salary_detail"
default = { expr = "row('5', '10', 'USD', 'hourly')::app.salary_detail" }

[[table.column]]
name = "fts"
type = "tsvector"
generated = { expr = "to_tsvector('english', description || ' ' || title)", stored = true }

[[table.index]]
name = "jobs_fts_idx"
method = "gin"
columns = ["fts"]

[[table.index]]
name = "jobs_active_idx"
columns = [
  { column = "organization_id" },
  { column = "created_at", order = "desc" },
]
where = "status = 'published'"

[[policy]]
schema = "app"
name = "jobs_are_publicly_readable"
table = "jobs"
command = "select"
roles = ["public"]
using = "true"

[[policy]]
schema = "app"
name = "job_owner_can_write"
table = "jobs"
command = "all"
roles = ["authenticated"]
using = "app.is_org_member(organization_id)"
with_check = "app.is_org_member(organization_id)"

[[function]]
schema = "app"
name = "is_org_member"
args = [{ name = "org_id", type = "bigint" }]
returns = "boolean"
language = "sql"
volatility = "stable"
security = "definer"
body = """
select exists (
  select 1
  from app.organization_members m
  where m.organization_id = org_id
    and m.user_id = app.current_user_id()
)
"""

[[role]]
name = "app_reader"
login = false
inherit = true

[[role]]
name = "app_writer"
login = false
inherit = true
member_of = ["app_reader"]

[[grant]]
on = "schema app"
to = ["app_reader", "app_writer"]
privileges = ["usage"]

[[grant]]
on = "table app.jobs"
to = ["app_reader"]
privileges = ["select"]

[[grant]]
on = "table app.jobs"
to = ["app_writer"]
privileges = ["select", "insert", "update", "delete"]

[[default_privilege]]
in_schema = "app"
on_type = "tables"
to = ["app_reader"]
privileges = ["select"]

[[raw]]
name = "pgrest_reload"
dialects = ["postgres"]
transaction = false
up = """
create event trigger pgrst_ddl_watch
  on ddl_command_end
  execute procedure pgrst_ddl_watch();
"""
down = """
drop event trigger if exists pgrst_ddl_watch;
"""
```

---

## 8. Postgres coverage matrix

| Feature | v1 | Form |
| --- | --- | --- |
| Schemas | ✅ | `[[schema]]` |
| Extensions | ✅ | `[[extension]]` |
| Roles, membership | ✅ | `[[role]]` |
| Grants / default privileges | ✅ | `[[grant]]`, `[[default_privilege]]` |
| Tables, columns, nullability | ✅ | `[[table]]`, `[[table.column]]` |
| PK / unique / check / FK | ✅ | table-level and column shorthand |
| Exclusion constraints | ✅ | `[[table.constraint]] type = "exclude"` |
| Indexes (btree/hash/gin/gist/spgist/brin) | ✅ | `[[table.index]]` |
| Partial / expression / covering indexes | ✅ | `where`, `expr`, `include` |
| Identity columns | ✅ | `identity` |
| Generated columns | ✅ | `generated` |
| Sequences | ✅ | `[[sequence]]` |
| Enums | ✅ | `[[enum]]` |
| Domains | ✅ | `[[domain]]` |
| Composite types | ✅ | `[[composite]]` |
| Range types | ✅ | `[[range]]` |
| Collations | ✅ | `[[collation]]` |
| Views | ✅ | `[[view]]` |
| Materialized views | ✅ | `[[view]] materialized = true` |
| Functions / procedures | ✅ | `[[function]]`, `[[procedure]]` |
| Triggers | ✅ | `[[trigger]]` |
| RLS + policies | ✅ | table flag + `[[policy]]` |
| Partitioning | ✅ | `partition_by`, `[[table.partition]]` |
| Inheritance | ✅ | `inherits` |
| Storage parameters | ✅ | `options` |
| Tablespaces / access methods | ✅ | `tablespace`, `access_method` |
| Full-text search | ✅ | native `tsvector` + GIN index |
| Extended statistics | ✅ | `[[statistics]]` |
| Publications | ✅ | `[[publication]]` |
| Subscriptions | ⚠️ | introspection + raw preferred |
| FDW, foreign tables, servers | ⚠️ | `[[raw]]` |
| Event triggers | ⚠️ | `[[raw]]` |
| Rules | ⚠️ | `[[raw]]` |
| Operators, casts, aggregates | ⚠️ | `[[raw]]` |
| Text search configs/dictionaries | ⚠️ | `[[raw]]` |
| Security labels | ⚠️ | `[[raw]]` |
| Large objects | ❌ | out of scope |

Legend: ✅ declarative; ⚠️ raw escape hatch; ❌ not planned.

---

## 9. Compiler design (Rust)

### 9.1 Modules

```text
src/schema/
  mod.rs              # public API: load, validate, diff, emit
  ast.rs              # serde structs mirroring TOML
  ir.rs               # resolved, normalized, dialect-neutral IR
  types.rs            # type parser + canonicalization
  expr.rs             # SQL expression wrapper + safety checks
  parse.rs            # TOML -> AST, multi-file merge
  validate.rs         # semantic validation
  naming.rs           # deterministic names and truncation
  normalize.rs        # expression canonicalization for drift comparison
  diff.rs             # Schema x Schema -> Vec<Change>
  plan.rs             # order changes, group ALTERs, mark destructive
  emit/
    mod.rs            # Dialect trait
    postgres.rs       # Postgres DDL
    mysql.rs          # later
    sqlite.rs         # later
  introspect/
    mod.rs            # Database -> Schema IR
    postgres.rs       # pg_catalog queries
```

### 9.2 Pipeline

```text
midas.toml + schema/*.toml
        │
        ▼
   parse (serde + toml)  ──►  AST
        │
        ▼
   resolve (FQNs, references, type names)
        │
        ▼
   validate (duplicates, FK targets, partitions, lengths)
        │
        ▼
   normalize (types, expressions, defaults)
        │
        ▼
   IR (desired)
        │
        ├──────────────► emit create-from-empty SQL
        │
        ▼
   diff(current IR, desired IR)  ──►  Vec<Change>
        │
        ▼
   plan (dependency order, destructive flags)
        │
        ▼
   emit up/down SQL  ──►  migrations/<ts>_<name>.sql
```

### 9.3 IR principles

- Fully resolved: every name is a `QualifiedName { schema: Option<String>, name: String }`.
- No TOML types leak into the IR. `toml::Value` is not used for semantics.
- Ordered collections where order matters (columns, enum values, index columns).
- Sorted collections where order does not matter (tables for diffing, grants).
- Every object carries an optional `comment` and a `dialect_options` map for future
  dialect-specific fields.
- Expressions are stored as `SqlExpr(String)` with a validation flag; never parsed
  into an expression tree in v1.

### 9.4 Parsing details

- Add `toml = "0.8"` and `indexmap` if needed.
- Use `#[serde(deny_unknown_fields)]` on all AST structs.
- Accept both `string` and `table` for fields with shorthand forms via untagged enums.
- Accept `schema + name` and dotted `name`.
- Reject duplicate object IDs after resolution.
- Preserve source spans for error messages where possible (`toml::de::Error` gives
  spans; wrap semantic errors with the object path).

### 9.5 Validation rules

1. Names are non-empty and valid UTF-8; warn above 63 bytes.
2. No duplicate schemas, tables, types, functions with the same signature, etc.
3. Every referenced schema/type/table exists in the DSL or is marked external.
4. Foreign-key target columns exist and form a PK or unique key.
5. FK column count and types match.
6. Enum values are non-empty and unique.
7. Partition bounds are valid for the strategy and do not overlap (best effort).
8. Index column references exist; expression indexes are non-empty.
9. `generated` columns use a valid `stored` flag.
10. No semicolons in expression fields.
11. Raw blocks have balanced dollar-quotes (best effort).
12. Grants reference valid privilege names for the object type.
13. `default_schema` exists or is implicitly created if `create_default_schema = true`.

### 9.6 Diff model

```rust
enum Change {
    CreateSchema(SchemaDef),
    DropSchema(SchemaDef),
    CreateExtension(ExtensionDef),
    AlterExtension(ExtensionDef, ExtensionDef),
    CreateEnum(EnumDef),
    AddEnumValue(EnumDef, String),
    RecreateEnum(EnumDef, EnumDef),
    CreateTable(TableDef),
    DropTable(TableDef),
    RenameTable(TableDef, String),
    AddColumn(TableDef, ColumnDef),
    DropColumn(TableDef, ColumnDef),
    RenameColumn(TableDef, ColumnDef, String),
    AlterColumnType(TableDef, ColumnDef, ColumnDef),
    SetNotNull(TableDef, ColumnDef),
    DropNotNull(TableDef, ColumnDef),
    SetDefault(TableDef, ColumnDef),
    DropDefault(TableDef, ColumnDef),
    AddConstraint(TableDef, ConstraintDef),
    DropConstraint(TableDef, ConstraintDef),
    CreateIndex(TableDef, IndexDef),
    DropIndex(TableDef, IndexDef),
    CreateView(ViewDef),
    DropView(ViewDef),
    ReplaceView(ViewDef, ViewDef),
    CreateFunction(FunctionDef),
    DropFunction(FunctionDef),
    ReplaceFunction(FunctionDef, FunctionDef),
    CreateTrigger(TriggerDef),
    DropTrigger(TriggerDef),
    CreatePolicy(PolicyDef),
    DropPolicy(PolicyDef),
    AlterPolicy(PolicyDef, PolicyDef),
    Grant(GrantDef),
    Revoke(GrantDef),
    Raw(RawDef),
}
```

Each change carries:

- `phase`: an ordering enum (schemas → extensions → types → tables → columns →
  constraints → indexes → views → functions → triggers → policies → grants → raw);
- `destructive`: bool;
- `irreversible`: bool;
- `lossy_down`: bool;
- a human-readable summary.

### 9.7 Ordering

Apply order:

1. schemas
2. extensions
3. collations, enums, domains, composites, ranges
4. sequences
5. tables (without FKs)
6. columns
7. PK / unique / check constraints
8. foreign keys
9. indexes
10. views
11. functions
12. triggers
13. policies / RLS
14. grants / default privileges
15. raw blocks
16. comments (can be folded into create/alter)

Drop order is the reverse, with foreign keys and dependents first.

Group `ALTER TABLE` changes per table where possible to reduce lock churn. Keep one
statement per logical change for readable SQL; add `-- midas:group` support later if
needed.

### 9.8 Rename detection

Explicit only:

```toml
[[table]]
schema = "app"
name = "people"
was = "users"

[[table.column]]
name = "email_address"
was = "email"
```

Without `was`, the diff is drop + create and is marked destructive. Do not guess
renames by similarity; false positives are dangerous in migrations.

### 9.9 Expression normalization

Introspected expressions are not textually identical to authored expressions
(`now()` vs `CURRENT_TIMESTAMP`, `::text` casts, schema qualification, whitespace).
Normalize both sides before comparing:

- trim and collapse whitespace;
- lowercase keywords;
- strip redundant `public.` qualification in `default_schema`;
- normalize `CURRENT_TIMESTAMP` / `now()` only when semantically safe (optional,
  behind a config flag);
- use `pg_get_expr` for defaults and checks;
- if normalization cannot prove equality, report drift and allow `[[ignore]]` or an
  explicit `normalized = "..."` field on the object.

This is the hardest part of database diffing. Ship conservative behavior first:
false drift warnings are better than silent destructive changes.

### 9.10 Introspection

`db pull` reads:

- `pg_namespace`, `pg_class`, `pg_attribute`, `pg_attrdef`, `pg_type`, `pg_enum`,
  `pg_range`, `pg_constraint`, `pg_index`, `pg_proc`, `pg_trigger`, `pg_policy`,
  `pg_roles`, `pg_auth_members`, `pg_extension`, `pg_collation`, `pg_publication`,
  `pg_statistic_ext`, `information_schema.role_table_grants`,
  `information_schema.role_column_grants`, `pg_default_acl`, `pg_views`,
  `pg_matviews`;
- definitions via `pg_get_viewdef`, `pg_get_functiondef`, `pg_get_constraintdef`,
  `pg_get_indexdef`, `pg_get_triggerdef`, `pg_get_expr`.

Rules:

- Skip `pg_catalog`, `information_schema`, `pg_toast`, `pg_temp_*`, and
  `midas`.
- Skip extension-owned objects by default (`pg_depend.deptype = 'e'`), but include
  the extension itself.
- Emit supported objects; report unsupported ones with a warning and, optionally,
  `--include-raw` using `pg_dump --schema-only` fragments.
- Never overwrite an existing schema file without `--force`; print a unified diff.

### 9.11 Generated migration output

```sql
-- midas:generated
-- midas:schema-hash 2f1a9c...
-- midas:no-transaction        # only when needed
-- !UP

create schema if not exists "app";

alter table only "app"."users"
  add column "email" "public"."citext" not null;

create unique index "users_email_idx"
  on "app"."users" ("email");

-- !DOWN

drop index if exists "app"."users_email_idx";
alter table only "app"."users" drop column if exists "email";
```

- Use `if not exists` / `if exists` where Postgres allows it.
- Do not use `cascade` by default.
- Emit comments (`comment on ...`) only when changed.
- Include a header hash to support `migrate resolve` later.

---

## 10. Migration workflow

### 10.1 Recommended developer loop

```bash
midas init                          # midas.toml, schema.toml, migrations/
# edit schema.toml
midas schema validate
midas migrate dev --name add_jobs   # diff + write migration + apply
midas migrate status
```

### 10.2 Generating a diff

`migrate dev` should use a **shadow database**:

1. create an empty shadow database;
2. replay all existing migrations;
3. introspect the shadow schema → *current managed state*;
4. diff against `schema.toml` → *desired state*;
5. generate UP/DOWN;
6. verify by applying UP to a fresh shadow database;
7. apply to the development database (unless `--create-only`).

Why shadow replay instead of diffing the live database:

- unmanaged objects in a shared dev database do not get dropped;
- it matches what production will look like after migrations;
- it makes drift detectable separately.

If no shadow database is available, fall back to a committed state snapshot:

```toml
# .midas/state.json (generated, committed)
```

or a `midas.__schema_state` table. Prefer the shadow database.

### 10.3 Drift

Before generating a migration, compare the live database against the shadow/expected
state. If objects differ:

- default: abort with a clear report;
- `--force`: ignore drift and generate from the expected state;
- `--accept-drift`: adopt the live state as the baseline (dangerous).

### 10.4 Applying migrations

```bash
midas migrate deploy      # alias for existing `up`
midas migrate status      # existing `status`
midas migrate revert      # existing `revert`
midas migrate reset       # drop + replay all migrations
midas migrate baseline    # mark existing migrations as applied
midas migrate resolve     # mark a failed migration as applied/rolled back
```

`migrate deploy` applies pending files in order. It must not generate diffs and must
not require DSL parsing, so it remains usable in production with only the migration
files.

### 10.5 Direct push

```bash
midas db push --accept-data-loss
```

`db push` introspects the live database, diffs against the DSL, and applies directly
without writing a migration. Useful for prototypes. Never the default.

### 10.6 Pull

```bash
midas db pull --schema app --include-raw
```

Introspect and write `schema.toml`. Print warnings for unsupported objects.

### 10.7 SQL preview

```bash
midas schema sql                    # full create-from-empty SQL
midas migrate diff --from schema --to db --script
midas migrate dev --create-only --name preview
```

All support `--dry-run` (no writes) and `--output <path>`.

---

## 11. Safety and operations

### 11.1 Destructive change policy

Classify changes:

| Class | Examples | Flag |
| --- | --- | --- |
| safe | create table, add nullable column, add index | none |
| caution | drop index, drop constraint, set not null | `--accept-data-loss` optional |
| destructive | drop table, drop column, type narrowing, rename without `was` | `--accept-data-loss` required |
| irreversible | remove enum value, drop function without previous definition | `--accept-data-loss` + warning |

`migrate dev` prints a summary and asks for confirmation when interactive. CI uses
`--accept-data-loss` explicitly.

### 11.2 Locking

- Prefer `ALTER TABLE ... ADD COLUMN` without a default for large tables; backfill in
  a `[[raw]]` block.
- `CREATE INDEX CONCURRENTLY` for large tables; mark the migration
  `-- midas:no-transaction`.
- `ADD CONSTRAINT ... NOT VALID` followed by `VALIDATE CONSTRAINT` for large tables;
  expose `not_valid = true` and a `validate` step.
- Set `lock_timeout` and `statement_timeout` for migration sessions via project config.

### 11.3 Transactions

- Default: wrap a migration in a transaction.
- `[[raw]] transaction = false` or any `concurrent = true` index disables it for that
  migration.
- Document that partial failure is possible in non-transactional migrations.

### 11.4 Secrets

- Never put passwords or connection strings directly in schema files.
- Use `connection = { env = "VAR" }` or the existing `DATABASE_URL`.
- Redact secrets in logs and error messages.

---

## 12. Portability

The IR must not assume Postgres. The `Dialect` trait:

```rust
trait Dialect {
    fn emit_create_table(&self, table: &TableDef) -> Result<String>;
    fn emit_create_index(&self, table: &TableDef, index: &IndexDef) -> Result<String>;
    fn supports(&self, feature: Feature) -> bool;
    fn quote_ident(&self, ident: &str) -> String;
    fn emit_literal(&self, value: &Literal) -> String;
}
```

Portability rules:

- Features not supported by a dialect produce a compile error with a clear message,
  not silently ignored.
- `[[raw]]` blocks can be gated by `dialects`.
- Store dialect-specific knobs in `dialect_options` rather than polluting the core IR.
- v1 ships Postgres only; MySQL/SQLite emitters can follow.

Known differences to design around:

| Feature | Postgres | MySQL | SQLite |
| --- | --- | --- | --- |
| Schemas | yes | database ≈ schema | attached DB |
| Enums | native | inline enum | check constraint |
| Composite types | native | no | no |
| Arrays | native | no | no |
| Partial indexes | yes | no | yes |
| Expression indexes | yes | generated columns | yes |
| RLS | yes | no | no |
| Identity | `GENERATED ... AS IDENTITY` | `AUTO_INCREMENT` | `AUTOINCREMENT` |
| `ALTER COLUMN` | rich | limited | limited; often table rebuild |

---

## 13. Implementation roadmap

### Milestone 0 — decisions and fixtures

- Confirm TOML, object model, and migration workflow.
- Add `docs/dsl.md` (this file).
- Build a fixture corpus from `sample_migrations/` by writing the equivalent DSL by
  hand.

### Milestone 1 — parser, IR, validator, create emitter

- `src/schema/ast.rs`, `ir.rs`, `types.rs`, `parse.rs`, `validate.rs`, `naming.rs`.
- Postgres emitter for schemas, extensions, tables, columns, PK/FK/unique/check,
  indexes, comments.
- CLI: `midas schema validate`, `midas schema sql`.
- Snapshot tests for TOML → SQL.

### Milestone 2 — diff and migration generation

- `diff.rs`, `plan.rs`, `emit/postgres.rs` for ALTER.
- Generate UP/DOWN; write into the existing migration format.
- CLI: `midas migrate diff`, `midas migrate dev --create-only`.
- Idempotence test: diff(schema, schema) is empty.

### Milestone 3 — introspection and drift

- `introspect/postgres.rs` for the v1 object set.
- `midas db pull`.
- Drift detection and `--force`.
- Shadow database replay.
- Round-trip test: DSL → SQL → apply → introspect → diff is empty.

### Milestone 4 — advanced Postgres objects

- Enums, domains, composites, ranges, collations.
- Sequences, generated columns, identity options.
- Views, materialized views.
- Functions, procedures, triggers.
- RLS policies.
- Roles, grants, default privileges.
- Partitioning and inheritance.
- Publications, extended statistics.

### Milestone 5 — safety and operations

- Rename detection via `was`.
- Destructive/irreversible classification.
- Concurrent indexes and non-transactional migrations.
- `NOT VALID` constraints and validation.
- `migrate baseline`, `migrate resolve`, `migrate reset`.
- State snapshot fallback.

### Milestone 6 — polish and portability

- `schema fmt` (deterministic TOML formatting).
- `db push`.
- Seeds/faker integration.
- MySQL and SQLite emitters for the core subset.
- Documentation, examples, error-message review.

---

## 14. Testing strategy

1. **Unit tests**
   - type parser and canonicalization;
   - naming/truncation;
   - expression safety checks;
   - expression normalization.

2. **Snapshot tests** (`snapbox` is already a dev dependency)
   - TOML fixtures → generated SQL;
   - diff fixtures → UP/DOWN SQL;
   - error messages.

3. **Idempotence tests**
   - `diff(schema, schema) == []`;
   - `apply(plan)` then `diff(introspected, schema) == []`.

4. **Integration tests**
   - spin up Postgres in CI (container), apply generated SQL, introspect, compare;
   - cover the sample schema: extensions, enums, composites, RLS, triggers,
     functions, grants, FTS, generated columns.

5. **CLI tests** (`trycmd` / `assert_cmd`)
   - `init`, `schema validate`, `schema sql`, `migrate dev --create-only`,
     `db pull`.

6. **Fuzz/property tests** (later)
   - random valid IR → emit → parse → compare;
   - random diffs → apply → converge.

---

## 15. Open decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | TOML vs a dedicated `.midas` syntax | TOML for v1; keep IR format-neutral; consider a Prisma-like front-end later |
| 2 | Single schema file vs directory | Support both; directory for large schemas |
| 3 | Diff source: live DB vs shadow replay | Shadow replay; state snapshot fallback |
| 4 | Explicit renames vs heuristic detection | Explicit `was`; never guess |
| 5 | Raw expression normalization | Conservative; report drift rather than silently accept |
| 6 | Auto-drop unmanaged objects | Never; only objects in managed schemas and not ignored |
| 7 | Role management | Create/alter only; no automatic drop |
| 8 | Secrets in DSL | Env references only |
| 9 | Postgres-only v1 | Yes; design the IR for portability but ship Postgres first |
| 10 | `serial` support | Accept, normalize, steer users to identity |

---

## 16. Appendix

### 16.1 Common Postgres type spellings

| Category | Canonical spellings |
| --- | --- |
| Integer | `smallint`, `integer`, `bigint` |
| Arbitrary precision | `numeric(p,s)`, `decimal(p,s)`, `numeric` |
| Floating point | `real`, `double precision` |
| Money | `money` |
| Character | `char(n)`, `varchar(n)`, `text` |
| Binary | `bytea` |
| Date/time | `date`, `time(p) [without time zone]`, `time(p) with time zone`, `timestamp(p) [without time zone]`, `timestamp(p) with time zone`, `interval [fields]` |
| Boolean | `boolean` |
| UUID | `uuid` |
| JSON | `json`, `jsonb` |
| XML | `xml` |
| Network | `inet`, `cidr`, `macaddr`, `macaddr8` |
| Bit string | `bit(n)`, `bit varying(n)` |
| Text search | `tsvector`, `tsquery` |
| Geometric | `point`, `line`, `lseg`, `box`, `path`, `polygon`, `circle` |
| Range/multirange | `int4range`, `int8range`, `numrange`, `tsrange`, `tstzrange`, `daterange`, `int4multirange`, `int8multirange`, `nummultirange`, `tsmultirange`, `tstzmultirange`, `datemultirange` |
| Object identifier | `oid`, `regclass`, `regtype`, `regproc`, ... |

### 16.2 Type grammar

```text
type        = base array? ;
array       = "[]" ;
base        = builtin | qualified_name ;
qualified   = ident ( "." ident )? ;
builtin     = "smallint" | "integer" | "bigint" | "boolean" | "text" | "bytea"
            | "uuid" | "json" | "jsonb" | "xml" | "inet" | "cidr" | "macaddr"
            | "macaddr8" | "money" | "tsvector" | "tsquery"
            | "real" | "double precision"
            | "numeric" [ "(" integer [ "," integer ] ")" ]
            | "decimal" [ "(" integer [ "," integer ] ")" ]
            | "varchar" [ "(" integer ")" ]
            | "char" [ "(" integer ")" ]
            | "bit" [ "(" integer ")" ]
            | "bit varying" [ "(" integer ")" ]
            | "date"
            | ( "time" | "timestamp" ) [ "(" integer ")" ]
              [ "with" | "without" ] "time" "zone"
            | "interval" [ interval_field ] ;
```

### 16.3 Identifier rules

- Prefer lowercase snake_case.
- Names are quoted in emitted SQL, so mixed case is preserved.
- Limit names to 63 bytes; longer names are hashed deterministically.
- Avoid dots in names; use the `schema` field instead.
- Avoid `;`, newlines, and control characters.

### 16.4 Expression rules

- Allowed in `default`, `check`, `generated`, `using`, `with_check`, `where`,
  `definition`, and `body`.
- No semicolons in scalar expressions.
- Dollar-quoted bodies are allowed for functions, procedures, and triggers.
- Expressions are trusted code, like migration SQL. Do not feed user input into them.
- Prefer `{ expr = "..." }` over trying to make the DSL understand SQL semantics.
