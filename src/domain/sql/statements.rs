use crate::diff::{
    ColumnChange, EnumValuePosition, IdentityChange, SequenceChange, TableOptionChange,
};
use crate::models::iden::Iden;
use crate::schema::*;
use std::collections::HashMap;
use std::fmt::Write as _;

use super::generator::{SqlOutput, SqlStmt, SqlStmtPart};

pub fn quote_identifier(dialect: &SqlDialect, name: impl Into<String>) -> String {
    let name: String = name.into();
    match dialect {
        SqlDialect::Postgres | SqlDialect::Sqlite => {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
        SqlDialect::Mysql => {
            format!("`{}`", name.replace('`', "``"))
        }
    }
}

pub fn qualified_table_name(dialect: &SqlDialect, id: &Iden) -> String {
    match id.schema() {
        Some(s) => format!(
            "{}.{}",
            quote_identifier(dialect, s),
            quote_identifier(dialect, id.name.clone())
        ),
        None => quote_identifier(dialect, id.name.clone()),
    }
}

pub fn qualified_name(
    dialect: &SqlDialect,
    name: impl Into<String>,
    schema: &Option<String>,
) -> String {
    match schema {
        Some(s) => format!(
            "{}.{}",
            quote_identifier(dialect, s),
            quote_identifier(dialect, name)
        ),
        None => quote_identifier(dialect, name),
    }
}

pub fn create_schema(dialect: &SqlDialect, name: &str) -> SqlStmt {
    format!("CREATE SCHEMA {}", quote_identifier(dialect, name)).into()
}

pub fn drop_schema(dialect: &SqlDialect, name: &str, cascade: bool) -> SqlStmt {
    let cascade = if cascade { " CASCADE" } else { "" };
    format!("DROP SCHEMA {}{}", quote_identifier(dialect, name), cascade).into()
}

pub fn rename_schema(dialect: &SqlDialect, from: &str, to: &str) -> SqlStmt {
    format!(
        "ALTER SCHEMA {} RENAME TO {}",
        quote_identifier(dialect, from),
        quote_identifier(dialect, to)
    )
    .into()
}

// Enum operations (PostgreSQL)

pub fn create_enum(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    values: &[String],
    description: &Option<String>,
) -> SqlOutput {
    let qualified = qualified_name(dialect, name, schema);
    let values_str: Vec<String> = values.iter().map(|v| format!("'{}'", v)).collect();
    let mut result = vec![SqlStmt::from(format!(
        "CREATE TYPE {} AS ENUM ({})",
        qualified,
        values_str.join(", ")
    ))];

    // Add COMMENT ON TYPE if description is present
    if let Some(desc) = description {
        let escaped = desc.replace('\'', "''");
        result.push(SqlStmt::from(format!(
            "COMMENT ON TYPE {} IS '{}'",
            qualified, escaped
        )));
    }

    result.into()
}

pub fn drop_enum(dialect: &SqlDialect, name: &str, schema: &Option<String>) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);
    format!("DROP TYPE {}", qualified).into()
}

pub fn rename_enum(dialect: &SqlDialect, from: &str, to: &str, schema: &Option<String>) -> SqlStmt {
    let qualified = qualified_name(dialect, from, schema);
    format!(
        "ALTER TYPE {} RENAME TO {}",
        qualified,
        quote_identifier(dialect, to)
    )
    .into()
}

pub fn add_enum_value(
    dialect: &SqlDialect,
    enum_name: &str,
    schema: &Option<String>,
    value: &str,
    position: &EnumValuePosition,
) -> SqlStmt {
    let qualified = qualified_name(dialect, enum_name, schema);
    let position_str = match position {
        EnumValuePosition::End => String::new(),
        EnumValuePosition::Before(v) => format!(" BEFORE '{}'", v),
        EnumValuePosition::After(v) => format!(" AFTER '{}'", v),
    };
    format!(
        "ALTER TYPE {} ADD VALUE '{}'{}",
        qualified, value, position_str
    )
    .into()
}

pub fn rename_enum_value(
    dialect: &SqlDialect,
    enum_name: &str,
    schema: &Option<String>,
    from: &str,
    to: &str,
) -> SqlStmt {
    let qualified = qualified_name(dialect, enum_name, schema);
    format!(
        "ALTER TYPE {} RENAME VALUE '{}' TO '{}'",
        qualified,
        from.replace('\'', "''"),
        to.replace('\'', "''")
    )
    .into()
}

pub fn drop_enum_value(
    dialect: &SqlDialect,
    enum_name: &str,
    schema: &Option<String>,
    value: &str,
) -> SqlOutput {
    rebuild_enum(
        dialect,
        enum_name,
        schema,
        format!(
            "    SELECT string_agg(quote_literal(e.enumlabel), ', ' ORDER BY e.enumsortorder)\n    INTO enum_values_sql\n    FROM pg_enum e\n    WHERE e.enumtypid = old_type_oid\n      AND e.enumlabel <> '{}';",
            value.replace('\'', "''")
        ),
        true,
    )
    .into()
}

pub fn reorder_enum_values(
    dialect: &SqlDialect,
    enum_name: &str,
    schema: &Option<String>,
    values: &[String],
) -> SqlOutput {
    rebuild_enum(
        dialect,
        enum_name,
        schema,
        format!(
            "    enum_values_sql := '{}';",
            render_enum_values(values).replace('\'', "''")
        ),
        true,
    )
    .into()
}

pub fn recreate_enum(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    values: &[String],
    description: &Option<String>,
) -> SqlOutput {
    SqlOutput::many(vec![
        rebuild_enum(
            dialect,
            name,
            schema,
            format!(
                "    enum_values_sql := '{}';",
                render_enum_values(values).replace('\'', "''")
            ),
            false,
        ),
        alter_enum_description(dialect, name, schema, description),
    ])
}

pub fn alter_enum_description(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    description: &Option<String>,
) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);
    match description {
        Some(desc) => {
            let escaped = desc.replace('\'', "''");
            format!("COMMENT ON TYPE {} IS '{}'", qualified, escaped).into()
        }
        None => format!("COMMENT ON TYPE {} IS NULL", qualified).into(),
    }
}

// Sequence operations

pub fn create_sequence(dialect: &SqlDialect, sequence: &Sequence) -> SqlStmt {
    let name = qualified_name(dialect, &sequence.name, &sequence.schema);
    let mut parts = vec![format!("CREATE SEQUENCE {}", name)];

    parts.push(format!("INCREMENT BY {}", sequence.increment));
    parts.push(format!("MINVALUE {}", sequence.min_value));

    if let Some(max) = sequence.max_value {
        parts.push(format!("MAXVALUE {}", max));
    } else {
        parts.push("NO MAXVALUE".to_string());
    }

    parts.push(format!("START WITH {}", sequence.start));
    parts.push(format!("CACHE {}", sequence.cache));

    if sequence.cycle {
        parts.push("CYCLE".to_string());
    } else {
        parts.push("NO CYCLE".to_string());
    }

    parts.join(" ").into()
}

pub fn drop_sequence(dialect: &SqlDialect, name: &str, schema: &Option<String>) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);
    format!("DROP SEQUENCE {}", qualified).into()
}

pub fn alter_sequence(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    changes: &[SequenceChange],
) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);

    let mut line = String::new();
    let _ = write!(line, "ALTER SEQUENCE {}", qualified);

    for change in changes {
        let _ = match change {
            SequenceChange::Increment(v) => write!(line, " INCREMENT BY {}", v),
            SequenceChange::MinValue(v) => write!(line, " MINVALUE {}", v),
            SequenceChange::MaxValue(Some(v)) => write!(line, " MAXVALUE {}", v),
            SequenceChange::MaxValue(None) => write!(line, " NO MAXVALUE"),
            SequenceChange::Start(v) => write!(line, " START WITH {}", v),
            SequenceChange::Cache(v) => write!(line, " CACHE {}", v),
            SequenceChange::Cycle(true) => write!(line, " CYCLE"),
            SequenceChange::Cycle(false) => write!(line, " NO CYCLE"),
        };
    }

    line.into()
}

// Table operations

pub fn create_table(dialect: &SqlDialect, table: &Table) -> SqlOutput {
    let name = qualified_name(dialect, &table.name, &table.schema);

    let mut table_pk_cols: HashMap<String, Option<String>> = HashMap::new();
    let mut table_unique_cols: HashMap<String, Option<String>> = HashMap::new();
    for constraint_type in &table.constraints {
        match constraint_type {
            Constraint::PrimaryKey(constraint) if constraint.columns.len() == 1 => {
                table_pk_cols.insert(constraint.columns[0].clone(), constraint.name.clone());
            }
            Constraint::Unique(constraint) if constraint.columns.len() == 1 => {
                table_unique_cols.insert(constraint.columns[0].clone(), constraint.name.clone());
            }
            _ => {}
        }
    }

    let mut column_defs: Vec<String> = table
        .columns
        .values()
        .map(|c| {
            column_definition_with_suppression(
                dialect,
                c,
                table_pk_cols.get(&c.name).map(Option::as_deref),
                table_unique_cols.get(&c.name).map(Option::as_deref),
            )
            .to_string()
        })
        .collect();

    // Add table-level constraints
    for constraint in &table.constraints {
        if is_inlined_single_column_constraint(constraint, table) {
            continue;
        }

        column_defs.push(constraint_definition(dialect, constraint).to_string());
    }

    let mut result = vec![SqlStmt::from(format!(
        "CREATE TABLE {} (\n  {}\n)",
        name,
        column_defs.join(",\n  ")
    ))];

    // Add COMMENT ON TABLE if comment is present
    if let Some(comment) = &table.comment {
        let escaped = comment.replace('\'', "''");
        result.push(SqlStmt::from(format!(
            "COMMENT ON TABLE {} IS '{}'",
            name, escaped
        )));
    }

    // Add COMMENT ON COLUMN for columns with comments
    for col in table.columns.values() {
        if let Some(comment) = &col.comment {
            let escaped = comment.replace('\'', "''");
            result.push(SqlStmt::from(format!(
                "COMMENT ON COLUMN {}.{} IS '{}'",
                name,
                quote_identifier(dialect, &col.name),
                escaped
            )));
        }
    }

    result.into()
}

pub fn column_definition(dialect: &SqlDialect, col: &Column) -> SqlStmtPart {
    column_definition_with_suppression(dialect, col, None, None)
}

fn column_definition_with_suppression(
    dialect: &SqlDialect,
    col: &Column,
    inline_primary_key: Option<Option<&str>>,
    inline_unique: Option<Option<&str>>,
) -> SqlStmtPart {
    let mut parts = vec![
        quote_identifier(dialect, &col.name),
        col.data_type.clone().to_string(dialect),
    ];

    if let Some(name) = inline_primary_key {
        if let Some(name) = name {
            parts.push(format!("CONSTRAINT {}", quote_identifier(dialect, name)));
        }
        parts.push("PRIMARY KEY".to_string());
    } else if col.primary_key {
        parts.push("PRIMARY KEY".to_string());
    }

    if !col.nullable {
        parts.push("NOT NULL".to_string());
    }

    if let Some(name) = inline_unique {
        if let Some(name) = name {
            parts.push(format!("CONSTRAINT {}", quote_identifier(dialect, name)));
        }
        parts.push("UNIQUE".to_string());
    } else if col.unique && !col.primary_key {
        parts.push("UNIQUE".to_string());
    }

    if let Some(default) = &col.default {
        parts.push(format!("DEFAULT {}", default));
    }

    if let Some(generated) = &col.generated {
        parts.push(format!("{}", generated));
    }

    if let Some(collation) = &col.collation {
        parts.push(format!("COLLATE {}", quote_identifier(dialect, collation)));
    }

    parts.join(" ").into()
}

fn is_inlined_single_column_constraint(constraint: &Constraint, table: &Table) -> bool {
    match constraint {
        Constraint::PrimaryKey(c) if c.columns.len() == 1 => {
            table.columns.contains_key(&c.columns[0])
        }
        Constraint::Unique(c) if c.columns.len() == 1 => table.columns.contains_key(&c.columns[0]),
        _ => false,
    }
}

pub fn constraint_definition(dialect: &SqlDialect, constraint: &Constraint) -> SqlStmtPart {
    let mut sql = String::new();

    if let Some(name) = &constraint.name() {
        write!(&mut sql, "CONSTRAINT {} ", quote_identifier(dialect, *name))
            .expect("writing to String cannot fail");
    }

    match (dialect, constraint) {
        (_, Constraint::PrimaryKey(c)) => {
            sql.push_str("PRIMARY KEY (");
            for (idx, col) in c.columns.iter().enumerate() {
                if idx > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&quote_identifier(dialect, col));
            }
            sql.push(')');
        }
        (_, Constraint::Unique(c)) => {
            sql.push_str("UNIQUE (");
            for (idx, col) in c.columns.iter().enumerate() {
                if idx > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&quote_identifier(dialect, col));
            }
            sql.push(')');
        }
        (_, Constraint::ForeignKey(c)) => {
            let ref_table = qualified_name(dialect, &c.references.name, &c.references.schema);

            sql.push_str("FOREIGN KEY (");
            for (idx, col) in c.columns.iter().enumerate() {
                if idx > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&quote_identifier(dialect, col));
            }
            write!(&mut sql, ") REFERENCES {} (", ref_table)
                .expect("writing to String cannot fail");
            for (idx, col) in c.references_columns.iter().enumerate() {
                if idx > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&quote_identifier(dialect, col));
            }
            sql.push(')');

            if c.on_delete != ReferenceAction::NoAction {
                write!(&mut sql, " ON DELETE {}", c.on_delete)
                    .expect("writing to String cannot fail");
            }
            if c.on_update != ReferenceAction::NoAction {
                write!(&mut sql, " ON UPDATE {}", c.on_update)
                    .expect("writing to String cannot fail");
            }
        }
        (_, Constraint::Check(c)) => {
            write!(&mut sql, "CHECK ({})", c.expression).expect("writing to String cannot fail");
        }
        (SqlDialect::Postgres, Constraint::Exclusion(c)) => {
            write!(&mut sql, "EXCLUDE USING {} (", c.using).expect("writing to String cannot fail");

            for (idx, (element, operator)) in c.elements.iter().enumerate() {
                if idx > 0 {
                    sql.push_str(", ");
                }
                write!(&mut sql, "{} WITH {}", element, operator)
                    .expect("writing to String cannot fail");
            }

            sql.push(')');

            if let Some(where_clause) = &c.where_clause {
                write!(&mut sql, " WHERE ({})", where_clause)
                    .expect("writing to String cannot fail");
            }
        }
        (_, Constraint::Exclusion(_)) => {
            // no-op as exclusions are only pg
        }
    }

    sql.into()
}

pub fn drop_table(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    cascade: bool,
) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);
    let cascade_str = if cascade { " CASCADE" } else { "" };
    format!("DROP TABLE {}{}", qualified, cascade_str).into()
}

pub fn rename_table(
    dialect: &SqlDialect,
    from: &str,
    to: &str,
    schema: &Option<String>,
) -> SqlStmt {
    let qualified = qualified_name(dialect, from, schema);
    format!(
        "ALTER TABLE {} RENAME TO {}",
        qualified,
        quote_identifier(dialect, to)
    )
    .into()
}

pub fn alter_table_comment(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    comment: &Option<String>,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    let stmt = match comment {
        Some(c) => {
            let escaped = c.replace('\'', "''");
            format!("COMMENT ON TABLE {} IS '{}'", qualified, escaped)
        }
        None => format!("COMMENT ON TABLE {} IS NULL", qualified),
    };
    stmt.into()
}

pub fn alter_table_options(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    changes: &[TableOptionChange],
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    let rendered = changes
        .iter()
        .map(|change| match change {
            TableOptionChange::Set { key, value, .. } => format!("{}={}", key, value),
            TableOptionChange::Drop { key, .. } => format!("{}=DEFAULT", key),
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("ALTER TABLE {} SET ({})", qualified, rendered).into()
}

pub fn alter_table_tablespace(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    tablespace: &Option<String>,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);

    match tablespace {
        Some(tablespace) => format!(
            "ALTER TABLE {} SET TABLESPACE {}",
            qualified,
            quote_identifier(dialect, tablespace)
        )
        .into(),
        None => format!(
            "ALTER TABLE {} SET TABLESPACE {}",
            qualified,
            quote_identifier(dialect, "pg_default")
        )
        .into(),
    }
}

pub fn alter_table_partition(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    partition: &Option<PartitionSpec>,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);

    match partition {
        Some(partition) => format!(
            "ALTER TABLE {} PARTITION BY {} ({})",
            qualified,
            render_partition_method(partition.method),
            partition.columns.join(", ")
        )
        .into(),
        None => format!("ALTER TABLE {} NOT PARTITIONED", qualified).into(),
    }
}

// Column operations

pub fn add_column(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    column: &Column,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    format!(
        "ALTER TABLE {} ADD COLUMN {}",
        qualified,
        column_definition(dialect, column)
    )
    .into()
}

pub fn drop_column(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    column: &str,
    cascade: bool,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    let cascade_str = if cascade { " CASCADE" } else { "" };
    format!(
        "ALTER TABLE {} DROP COLUMN {}{}",
        qualified,
        quote_identifier(dialect, column),
        cascade_str
    )
    .into()
}

pub fn rename_column(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    from: &str,
    to: &str,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {}",
        qualified,
        quote_identifier(dialect, from),
        quote_identifier(dialect, to)
    )
    .into()
}

pub fn alter_column(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    column: &str,
    changes: &[ColumnChange],
) -> SqlOutput {
    let qualified = qualified_name(dialect, table, schema);
    let column_quoted = quote_identifier(dialect, column);

    changes
        .iter()
        .map(|change| match change {
            ColumnChange::SetType(t) => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                    qualified, column_quoted, t
                )
            }
            ColumnChange::SetNotNull => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL",
                    qualified, column_quoted
                )
            }
            ColumnChange::DropNotNull => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                    qualified, column_quoted
                )
            }
            ColumnChange::SetDefault(d) => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                    qualified, column_quoted, d
                )
            }
            ColumnChange::DropDefault => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
                    qualified, column_quoted
                )
            }
            ColumnChange::SetGenerated(g) => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} SET {}",
                    qualified, column_quoted, g
                )
            }
            ColumnChange::DropGenerated => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP EXPRESSION",
                    qualified, column_quoted
                )
            }
            ColumnChange::SetCollation(collation) => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} SET COLLATION {}",
                    qualified,
                    column_quoted,
                    quote_identifier(dialect, collation)
                )
            }
            ColumnChange::DropCollation => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP COLLATION",
                    qualified, column_quoted
                )
            }
            ColumnChange::SetIdentity(identity) => {
                let mut stmt = format!(
                    "ALTER TABLE {} ALTER COLUMN {} ADD GENERATED {} AS IDENTITY",
                    qualified,
                    column_quoted,
                    if identity.always {
                        "ALWAYS"
                    } else {
                        "BY DEFAULT"
                    }
                );

                if let Some(options) = &identity.sequence_options {
                    let options = format_sequence_options(options);
                    if !options.is_empty() {
                        write!(&mut stmt, " ({})", options).expect("writing to String cannot fail");
                    }
                }

                stmt
            }
            ColumnChange::DropIdentity => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP IDENTITY",
                    qualified, column_quoted
                )
            }
            ColumnChange::AlterIdentity(identity_changes) => {
                let rendered = identity_changes
                    .iter()
                    .map(format_identity_change)
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} {}",
                    qualified, column_quoted, rendered
                )
            }
        })
        .map(SqlStmt::from)
        .collect::<Vec<_>>()
        .into()
}

pub fn alter_column_comment(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    column: &str,
    comment: &Option<String>,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    let column_quoted = quote_identifier(dialect, column);
    let stmt = match comment {
        Some(c) => {
            let escaped = c.replace('\'', "''");
            format!(
                "COMMENT ON COLUMN {}.{} IS '{}'",
                qualified, column_quoted, escaped
            )
        }
        None => format!("COMMENT ON COLUMN {}.{} IS NULL", qualified, column_quoted),
    };
    stmt.into()
}

pub fn create_index(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    index: &Index,
    concurrently: bool,
    if_not_exists: bool,
) -> SqlStmt {
    let mut sql = String::from("CREATE ");

    if index.unique {
        sql.push_str("UNIQUE ");
    }

    sql.push_str("INDEX ");

    if concurrently {
        sql.push_str("CONCURRENTLY ");
    }

    if if_not_exists {
        sql.push_str("IF NOT EXISTS ");
    }

    sql.push_str(&quote_identifier(dialect, &index.name));
    sql.push_str(" ON ");
    sql.push_str(&qualified_name(dialect, table, schema));

    if index.method != IndexMethod::BTree {
        write!(&mut sql, " USING {}", index.method).expect("writing to String cannot fail");
    }

    sql.push_str(" (");
    for (idx, col) in index.columns.iter().enumerate() {
        if idx > 0 {
            sql.push_str(", ");
        }

        sql.push_str(&render_index_column(dialect, col));
    }
    sql.push(')');

    if !index.include.is_empty() {
        sql.push_str(" INCLUDE (");
        for (idx, col) in index.include.iter().enumerate() {
            if idx > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&quote_identifier(dialect, col));
        }
        sql.push(')');
    }

    if let Some(where_clause) = &index.where_clause {
        write!(&mut sql, " WHERE {}", where_clause).expect("writing to String cannot fail");
    }

    if !index.options.is_empty() {
        let options = index
            .options
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<_>>()
            .join(", ");
        write!(&mut sql, " WITH ({})", options).expect("writing to String cannot fail");
    }

    if let Some(tablespace) = &index.tablespace {
        write!(
            &mut sql,
            " TABLESPACE {}",
            quote_identifier(dialect, tablespace)
        )
        .expect("writing to String cannot fail");
    }

    sql.into()
}

pub fn drop_index(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    concurrently: bool,
    if_exists: bool,
) -> SqlStmt {
    let mut sql = String::from("DROP INDEX ");

    if concurrently {
        sql.push_str("CONCURRENTLY ");
    }

    if if_exists {
        sql.push_str("IF EXISTS ");
    }

    sql.push_str(&qualified_name(dialect, name, schema));
    sql.into()
}

pub fn rename_index(
    dialect: &SqlDialect,
    from: &str,
    schema: &Option<String>,
    to: &str,
) -> SqlStmt {
    let qualified = qualified_name(dialect, from, schema);
    format!(
        "ALTER INDEX {} RENAME TO {}",
        qualified,
        quote_identifier(dialect, to)
    )
    .into()
}

// Constraint operations

pub fn add_constraint(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    constraint: &Constraint,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    format!(
        "ALTER TABLE {} ADD {}",
        qualified,
        constraint_definition(dialect, constraint)
    )
    .into()
}

pub fn drop_constraint(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    name: &str,
    cascade: bool,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    let cascade_str = if cascade { " CASCADE" } else { "" };
    format!(
        "ALTER TABLE {} DROP CONSTRAINT {}{}",
        qualified,
        quote_identifier(dialect, name),
        cascade_str
    )
    .into()
}

pub fn rename_constraint(
    dialect: &SqlDialect,
    table: &str,
    schema: &Option<String>,
    from: &str,
    to: &str,
) -> SqlStmt {
    let qualified = qualified_name(dialect, table, schema);
    format!(
        "ALTER TABLE {} RENAME CONSTRAINT {} TO {}",
        qualified,
        quote_identifier(dialect, from),
        quote_identifier(dialect, to)
    )
    .into()
}

// View operations

pub fn create_view(dialect: &SqlDialect, view: &View, or_replace: bool) -> SqlStmt {
    let name = qualified_name(dialect, &view.name, &view.schema);

    let mut sql = String::from("CREATE ");

    if or_replace {
        sql.push_str("OR REPLACE ");
    }

    if view.materialized {
        sql.push_str("MATERIALIZED ");
    }

    write!(&mut sql, "VIEW {} AS {}", name, view.definition)
        .expect("writing to String cannot fail");
    sql.into()
}

pub fn drop_view(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    materialized: bool,
    cascade: bool,
) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);
    let materialized_str = if materialized { "MATERIALIZED " } else { "" };
    let cascade_str = if cascade { " CASCADE" } else { "" };
    format!("DROP {}VIEW {}{}", materialized_str, qualified, cascade_str).into()
}

pub fn alter_view(
    dialect: &SqlDialect,
    name: &str,
    schema: &Option<String>,
    new_definition: &str,
) -> SqlStmt {
    let qualified = qualified_name(dialect, name, schema);
    format!("CREATE OR REPLACE VIEW {} AS {}", qualified, new_definition).into()
}

// Extension operations (PostgreSQL)

pub fn create_extension(dialect: &SqlDialect, name: &str) -> SqlStmt {
    format!(
        "CREATE EXTENSION IF NOT EXISTS {}",
        quote_identifier(dialect, name)
    )
    .into()
}

pub fn drop_extension(dialect: &SqlDialect, name: &str) -> SqlStmt {
    format!(
        "DROP EXTENSION IF EXISTS {}",
        quote_identifier(dialect, name)
    )
    .into()
}

fn format_sequence_options(options: &SequenceOptions) -> String {
    let mut parts = Vec::new();

    if let Some(start) = options.start {
        parts.push(format!("START WITH {}", start));
    }
    if let Some(increment) = options.increment {
        parts.push(format!("INCREMENT BY {}", increment));
    }
    if let Some(min_value) = options.min_value {
        parts.push(format!("MINVALUE {}", min_value));
    }
    if let Some(max_value) = options.max_value {
        parts.push(format!("MAXVALUE {}", max_value));
    }
    if let Some(cache) = options.cache {
        parts.push(format!("CACHE {}", cache));
    }
    if options.cycle {
        parts.push("CYCLE".to_string());
    }

    parts.join(" ")
}

fn format_identity_change(change: &IdentityChange) -> String {
    match change {
        IdentityChange::SetGeneratedAlways => "SET GENERATED ALWAYS".to_string(),
        IdentityChange::SetGeneratedByDefault => "SET GENERATED BY DEFAULT".to_string(),
        IdentityChange::SetSequenceOptions(options) => {
            let rendered = format_sequence_options(options);
            if rendered.is_empty() {
                "SET ( )".to_string()
            } else {
                format!("SET ({})", rendered)
            }
        }
        IdentityChange::DropSequenceOptions => "SET ()".to_string(),
    }
}

fn render_index_column(dialect: &SqlDialect, column: &IndexColumn) -> String {
    let mut rendered = String::new();

    match column {
        IndexColumn::Column {
            name,
            order,
            nulls,
            opclass,
        } => {
            rendered.push_str(&quote_identifier(dialect, name));
            if let Some(opclass) = opclass {
                write!(&mut rendered, " {}", opclass).expect("writing to String cannot fail");
            }
            if let Some(order) = order {
                write!(&mut rendered, " {}", order.to_sql())
                    .expect("writing to String cannot fail");
            }
            if let Some(nulls) = nulls {
                write!(&mut rendered, " {}", nulls.to_sql())
                    .expect("writing to String cannot fail");
            }
        }
        IndexColumn::Expression {
            expression,
            order,
            nulls,
        } => {
            rendered.push_str(expression);
            if let Some(order) = order {
                write!(&mut rendered, " {}", order.to_sql())
                    .expect("writing to String cannot fail");
            }
            if let Some(nulls) = nulls {
                write!(&mut rendered, " {}", nulls.to_sql())
                    .expect("writing to String cannot fail");
            }
        }
    }

    rendered
}

fn render_partition_method(method: PartitionMethod) -> &'static str {
    match method {
        PartitionMethod::Range => "RANGE",
        PartitionMethod::List => "LIST",
        PartitionMethod::Hash => "HASH",
    }
}

fn render_enum_values(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn rebuild_enum(
    dialect: &SqlDialect,
    enum_name: &str,
    schema: &Option<String>,
    values_assignment_sql: String,
    preserve_comment: bool,
) -> SqlStmt {
    let qualified = qualified_name(dialect, enum_name, schema);
    let temp_name = format!("{}__old", enum_name);
    let temp_qualified = qualified_name(dialect, &temp_name, schema);
    let schema_expr = match schema {
        Some(schema) => format!("'{}'", schema.replace('\'', "''")),
        None => "current_schema()".to_string(),
    };

    let mut block = vec![
        "DO $$".to_string(),
        "DECLARE".to_string(),
        "    old_type_oid oid;".to_string(),
        "    enum_values_sql text;".to_string(),
        "    column_record record;".to_string(),
    ];

    if preserve_comment {
        block.push("    enum_comment text;".to_string());
    }

    block.push("BEGIN".to_string());

    if preserve_comment {
        block.push(format!(
            "    SELECT obj_description(t.oid, 'pg_type') INTO enum_comment FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE t.typname = '{}' AND n.nspname = {};",
            enum_name.replace('\'', "''"),
            schema_expr
        ));
    }

    block.push(format!(
        "    EXECUTE 'ALTER TYPE {} RENAME TO {}';",
        qualified,
        quote_identifier(dialect, &temp_name)
    ));
    block.push(format!(
        "    SELECT t.oid INTO old_type_oid FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE t.typname = '{}' AND n.nspname = {};",
        temp_name.replace('\'', "''"),
        schema_expr
    ));
    block.push(values_assignment_sql);
    block.push(
        "    IF enum_values_sql IS NULL OR enum_values_sql = '' THEN RAISE EXCEPTION 'enum must contain at least one value'; END IF;"
            .to_string(),
    );
    block.push(format!(
        "    EXECUTE 'CREATE TYPE {} AS ENUM (' || enum_values_sql || ')';",
        qualified
    ));
    block.push(
        "    FOR column_record IN SELECT ns.nspname AS schema_name, cls.relname AS table_name, att.attname AS column_name, pg_get_expr(def.adbin, def.adrelid) AS default_expr FROM pg_attribute att JOIN pg_class cls ON cls.oid = att.attrelid JOIN pg_namespace ns ON ns.oid = cls.relnamespace LEFT JOIN pg_attrdef def ON def.adrelid = att.attrelid AND def.adnum = att.attnum WHERE att.atttypid = old_type_oid AND att.attnum > 0 AND NOT att.attisdropped LOOP".to_string(),
    );
    block.push(
        "        IF column_record.default_expr IS NOT NULL THEN EXECUTE format('ALTER TABLE %I.%I ALTER COLUMN %I DROP DEFAULT', column_record.schema_name, column_record.table_name, column_record.column_name); END IF;"
            .to_string(),
    );
    block.push(format!(
        "        EXECUTE format('ALTER TABLE %I.%I ALTER COLUMN %I TYPE {} USING %I::text::{}', column_record.schema_name, column_record.table_name, column_record.column_name, column_record.column_name);",
        qualified, qualified
    ));
    block.push(format!(
        "        IF column_record.default_expr IS NOT NULL THEN EXECUTE format('ALTER TABLE %I.%I ALTER COLUMN %I SET DEFAULT ((%s)::text::{})', column_record.schema_name, column_record.table_name, column_record.column_name, column_record.default_expr); END IF;",
        qualified
    ));
    block.push("    END LOOP;".to_string());
    block.push(format!("    EXECUTE 'DROP TYPE {}';", temp_qualified));

    if preserve_comment {
        block.push(format!(
            "    IF enum_comment IS NOT NULL THEN EXECUTE format('COMMENT ON TYPE {} IS %L', enum_comment); END IF;",
            qualified
        ));
    }

    block.push("END $$;".to_string());

    block.join("\n").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_postgres_exclusion_constraint() {
        let constraint = Constraint::Exclusion(
            ExclusionConstraint::new(vec![("room", "="), ("during", "&&")])
                .named("room_booking_excl")
                .using("gist")
                .where_clause("cancelled_at IS NULL"),
        );

        assert_eq!(
            constraint_definition(&SqlDialect::Postgres, &constraint).to_string(),
            "CONSTRAINT \"room_booking_excl\" EXCLUDE USING gist (room WITH =, during WITH &&) WHERE (cancelled_at IS NULL)"
        );
    }

    #[test]
    fn skips_exclusion_constraint_for_non_postgres() {
        let constraint = Constraint::Exclusion(ExclusionConstraint::new(vec![("room", "=")]));

        assert_eq!(
            constraint_definition(&SqlDialect::Mysql, &constraint).to_string(),
            ""
        );
    }

    #[test]
    fn renders_foreign_key_referenced_columns() {
        let constraint = Constraint::ForeignKey(
            ForeignKeyConstraint::new(
                vec!["product_id"],
                ("product".to_string(), Some("public".to_string())),
                vec!["id"],
            )
            .named("user_product_product_id_fkey"),
        );

        assert_eq!(
            add_constraint(
                &SqlDialect::Postgres,
                "user_product",
                &Some("public".to_string()),
                &constraint,
            )
            .to_string(),
            "ALTER TABLE \"public\".\"user_product\" ADD CONSTRAINT \"user_product_product_id_fkey\" FOREIGN KEY (\"product_id\") REFERENCES \"public\".\"product\" (\"id\");"
        );
    }

    #[test]
    fn rebuilds_enum_when_dropping_a_value() {
        let sql = drop_enum_value(
            &SqlDialect::Postgres,
            "status",
            &Some("public".to_string()),
            "archived",
        )
        .to_string(None);

        assert!(sql.contains("ALTER TYPE \"public\".\"status\" RENAME TO \"status__old\""));
        assert!(sql.contains("e.enumlabel <> 'archived'"));
        assert!(sql.contains("CREATE TYPE \"public\".\"status\" AS ENUM ("));
        assert!(sql.contains("DROP TYPE \"public\".\"status__old\""));
    }

    #[test]
    fn recreates_enum_with_comment_statement() {
        let sql = recreate_enum(
            &SqlDialect::Postgres,
            "status",
            &Some("public".to_string()),
            &["draft".to_string(), "published".to_string()],
            &Some("workflow state".to_string()),
        );

        let parts = sql.parts();
        assert_eq!(parts.len(), 2);
        assert!(
            parts[0]
                .as_sql()
                .contains("CREATE TYPE \"public\".\"status\" AS ENUM (")
        );
        assert_eq!(
            parts[1],
            "COMMENT ON TYPE \"public\".\"status\" IS 'workflow state'".into()
        );
    }
}
