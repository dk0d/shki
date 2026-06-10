# Pgschema-Inspired Diff Refactor Plan

## Goal

Replace shki's flat `Vec<DiffStatement>` diff pipeline with a cleaner, granular diff pipeline inspired by pgschema:

```text
Snapshot -> typed object diffs -> ordered SQL plan -> rendered migration
```

The refactor should improve locality for object-specific behavior, make dependency ordering explicit, and support PostgreSQL patterns such as creating tables before adding foreign key constraints that reference other tables.

## Current Shape

The current path is:

```text
Snapshot A + Snapshot B
-> diff_snapshots
-> helpers.rs appends DiffStatement values directly
-> SqlRenderer renders DiffStatement values
```

This works for simple diffs, but it has architectural friction:

- Table behavior is spread across `diff/helpers.rs`, `diff/statements.rs`, `sql/diffs.rs`, and `sql/statements.rs`.
- Dependency ordering is implicit in append order.
- A `CreateTable` statement currently carries the whole table, including constraints that may need to be applied later.
- Statement metadata such as object type, operation, path, and transaction safety is not represented.
- The renderer sees flattened statements, so it cannot reason about a table create as a group of related sub-resource changes.

## Target Shape

Introduce a deeper diff module with object-specific diffs and an explicit SQL plan:

```text
src/domain/diff/
  mod.rs
  model.rs          # Diff, DiffType, DiffOperation, SqlStep, SqlPlan
  collector.rs      # Collects SQL steps with object metadata
  table.rs          # TableDiff, ColumnDiff, ConstraintDiff, IndexDiff
  topological.rs    # Dependency ordering helpers
  render.rs         # Object diffs -> SQL plan
```

Initial target types:

```rust
pub struct SqlPlan {
    pub steps: Vec<SqlStep>,
}

pub struct SqlStep {
    pub sql: String,
    pub object_type: DiffType,
    pub operation: DiffOperation,
    pub path: String,
    pub can_run_in_transaction: bool,
}

pub enum DiffType {
    Schema,
    Type,
    Sequence,
    Table,
    TableColumn,
    TableConstraint,
    TableIndex,
    View,
    Extension,
}

pub enum DiffOperation {
    Create,
    Alter,
    Drop,
}
```

The existing `SchemaDiff` can remain during migration, but its implementation should move toward storing structured object diffs and/or plan steps rather than only `DiffStatement`.

## Table-Diff Module

Tables should become the first deep module because they contain the most ordering-sensitive behavior.

Target table module:

```rust
pub struct TableDiff {
    pub table: Table,
    pub added_columns: Vec<Column>,
    pub dropped_columns: Vec<Column>,
    pub modified_columns: Vec<ColumnDiff>,
    pub added_constraints: Vec<Constraint>,
    pub dropped_constraints: Vec<Constraint>,
    pub modified_constraints: Vec<ConstraintDiff>,
    pub added_indexes: Vec<Index>,
    pub dropped_indexes: Vec<Index>,
    pub modified_indexes: Vec<IndexDiff>,
    pub comment_changed: Option<CommentChange>,
    pub option_changes: Vec<TableOptionChange>,
}
```

The module owns:

- comparing table sub-resources
- deciding whether a change is create/drop/alter
- topological ordering for table creates and drops
- foreign key deferral for create-table operations
- lowering to the current `DiffStatement` adapter while the rest of the codebase migrates

## Foreign Key Deferral

For PostgreSQL-compatible migration generation, table creation should follow PostgreSQL's `pg_dump` pattern:

```text
CREATE TABLE parent (...);
CREATE TABLE child (... without FK ...);
ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (...) REFERENCES parent (...);
```

This avoids failures when declarative schemas contain tables with inline foreign keys referencing tables created later or cycles.

In the structured diff pipeline, this is not a parser concern. It is table rendering behavior:

- Keep primary keys, unique constraints, checks, and exclusions inline.
- Defer foreign keys from newly-created tables to later `AddConstraint` steps.
- Topologically sort table creates where possible.
- Break cycles deterministically because deferred FKs make cycles safe.

## Migration Strategy

### Phase 1: Transitional Table Diff

Introduce table-specific diff and topological modules, but lower back to existing `DiffStatement` values.

Scope:

- Add `diff/table.rs` and `diff/topological.rs`.
- Route table diffs through the new table module.
- Topologically sort created tables.
- Split foreign keys out of newly-created tables and emit them later as `AddConstraint` statements.
- Keep `SchemaDiff { statements, rename_scenarios }` unchanged.

This phase gives immediate behavior improvement without breaking command surfaces.

### Phase 2: SQL Plan Collector

Add plan/step metadata while still supporting the existing renderer.

Scope:

- Add `SqlPlan`, `SqlStep`, `DiffType`, and `DiffOperation`.
- Add a collector module.
- Teach rendering to produce plan steps.
- Keep `generate_string` as a compatibility layer over the plan.

### Phase 3: Object-Specific Diffs

Move non-table logic into object modules:

- `schema.rs`
- `enum.rs`
- `sequence.rs`
- `view.rs`
- `extension.rs`

Each module should expose a small interface that returns typed diffs or plan steps.

### Phase 4: Remove Flat DiffStatement Dependency

Once all object modules generate plan steps directly:

- Replace `SchemaDiff.statements` with structured diffs or a plan.
- Move down-migration reversal behavior onto typed diff objects.
- Delete the large `DiffStatement` renderer match.

### Phase 5: Declarative SQL Normalization

The pgschema-style diff refactor improves generated migrations from structured snapshots. Raw SQL declarative compilation still needs a separate normalization seam if imported SQL is applied directly.

After the table-diff refactor lands, add a declarative SQL normalizer that mirrors the table renderer behavior for raw SQL:

- split top-level statements
- rewrite `CREATE TABLE` table-level FKs into deferred `ALTER TABLE ADD CONSTRAINT`
- apply normal statements first, deferred FKs second

## First Slice Implemented Here

The first slice is intentionally transitional:

- `diff/table.rs` owns table create/drop/modify lowering.
- `diff/topological.rs` owns deterministic table create ordering.
- Newly-created table foreign keys are deferred to `AddConstraint` statements.
- Existing public API and rename flow remain unchanged.

This creates the seam for the full replacement while reducing risk.
