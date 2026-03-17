use super::introspect::introspect_db;
use crate::config::Config;
use crate::create_any_pool_opts;
use crate::schema::SchemaDialect;
use crate::snapshot::{ColumnSnapshot, ConstraintType};
use crate::{MigrationManager, Result, ShkiError, Snapshot, SqlGenerator, diff_snapshots};
use colored::Colorize;
use std::fmt::Write as _;
use std::path::PathBuf;

#[allow(clippy::too_many_arguments)]
pub async fn cmd_bootstrap(
    config: &Config,
    name: Option<String>,
    legacy_tables: Vec<String>,
    drop_legacy_tables: bool,
    write_lua: bool,
    lua_output: Option<PathBuf>,
    mark_applied: bool,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    if drop_legacy_tables && legacy_tables.is_empty() {
        return Err(ShkiError::config(
            "--drop-legacy-tables requires at least one --legacy-table",
        ));
    }

    let db_url = config
        .database_url
        .as_ref()
        .ok_or_else(|| ShkiError::config("DATABASE_URL is required"))?;

    let manager = build_migration_manager(config);

    if !force {
        let has_migrations = !manager.list_migrations()?.is_empty();
        let has_snapshots = !Snapshot::load_all(&config.out_dir())?.is_empty();

        if has_migrations || has_snapshots {
            return Err(ShkiError::config(
                "Local migrations/snapshots already exist. Use --force to bootstrap anyway.",
            ));
        }
    }

    println!("{}", "Introspecting database...".cyan());

    let mut snapshot = introspect_db(config).await?;

    // Never include the configured shki migrations table in a baseline migration.
    snapshot.tables.shift_remove(&config.migrations.table);

    // Optionally remove legacy migration metadata tables from baseline snapshot.
    for table in &legacy_tables {
        snapshot.tables.shift_remove(&table_basename(table));
    }

    let base = Snapshot::new(config.dialect);
    let diff = diff_snapshots(&base, &snapshot)?;
    let sql = SqlGenerator::new(config.dialect)
        .with_breakpoints(config.breakpoints)
        .generate_sql(&diff)?;

    let migration_name = name.or_else(|| Some("bootstrap".to_string()));
    let lua_path = lua_output
        .as_ref()
        .map(|p| config.resolve_path(p))
        .unwrap_or_else(|| config.schema_path());

    if dry_run {
        println!("\n{}", "Bootstrap migration SQL (dry run):".cyan());
        println!("{}", sql);

        if write_lua {
            println!(
                "\n{} {}",
                "Lua schema would be written to:".cyan(),
                lua_path.display()
            );
        }

        if mark_applied {
            println!("{}", "Migration would be marked as applied.".yellow());
        }

        if drop_legacy_tables {
            println!("{}", "Legacy migration table(s) would be dropped:".yellow());
            for table in &legacy_tables {
                println!("  - {}", table);
            }
        }

        return Ok(());
    }

    let (up_path, _down_path) =
        manager.create_migration_with_down(migration_name, &sql, None, None, &snapshot)?;

    println!("\n{}", "Bootstrap migration created:".green());
    println!("  Up: {}", up_path.display());

    if write_lua {
        if lua_path.exists() && !force {
            return Err(ShkiError::config(format!(
                "Lua schema path '{}' already exists. Use --force to overwrite.",
                lua_path.display()
            )));
        }

        if let Some(parent) = lua_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let lua = render_lua_schema(&snapshot, config.dialect);
        std::fs::write(&lua_path, lua)?;
        println!("  Lua: {}", lua_path.display());
    }

    if mark_applied || drop_legacy_tables {
        let pool = create_any_pool_opts()
            .max_connections(2)
            .connect(db_url)
            .await?;

        if mark_applied {
            manager.mark_migration_applied(&pool, &up_path).await?;
            println!("{}", "Marked bootstrap migration as applied.".green());
        }

        if drop_legacy_tables {
            for legacy in &legacy_tables {
                let qualified = parse_qualified_table(legacy);
                let sql = drop_table_if_exists_sql(
                    config.dialect,
                    qualified.schema.as_deref(),
                    &qualified.table,
                );
                sqlx::query(&sql).execute(&pool).await?;
                println!("{} {}", "Dropped legacy table:".yellow(), legacy);
            }
        }
    }

    Ok(())
}

fn build_migration_manager(config: &Config) -> MigrationManager {
    let manager = MigrationManager::new(config.out_dir(), config.dialect)
        .with_table_name(&config.migrations.table)
        .with_prefix(config.migrations.prefix);

    if let Some(schema) = &config.migrations.schema {
        manager.with_table_schema(schema)
    } else {
        manager
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QualifiedTable {
    schema: Option<String>,
    table: String,
}

fn parse_qualified_table(input: &str) -> QualifiedTable {
    let trimmed = input.trim();
    if let Some((schema, table)) = trimmed.split_once('.') {
        QualifiedTable {
            schema: Some(unquote_identifier(schema).to_string()),
            table: unquote_identifier(table).to_string(),
        }
    } else {
        QualifiedTable {
            schema: None,
            table: unquote_identifier(trimmed).to_string(),
        }
    }
}

fn table_basename(input: &str) -> String {
    parse_qualified_table(input).table
}

fn unquote_identifier(s: &str) -> &str {
    s.trim().trim_matches('"').trim_matches('`')
}

fn quote_identifier(dialect: SchemaDialect, ident: &str) -> String {
    match dialect {
        SchemaDialect::Mysql => format!("`{}`", ident.replace('`', "``")),
        SchemaDialect::Postgres | SchemaDialect::Sqlite => {
            format!("\"{}\"", ident.replace('"', "\"\""))
        }
    }
}

fn drop_table_if_exists_sql(dialect: SchemaDialect, schema: Option<&str>, table: &str) -> String {
    let qualified = match (dialect, schema) {
        (SchemaDialect::Postgres, Some(s)) | (SchemaDialect::Mysql, Some(s)) => {
            format!(
                "{}.{}",
                quote_identifier(dialect, s),
                quote_identifier(dialect, table)
            )
        }
        _ => quote_identifier(dialect, table),
    };

    format!("DROP TABLE IF EXISTS {}", qualified)
}

fn render_lua_schema(snapshot: &Snapshot, dialect: SchemaDialect) -> String {
    let mut out = String::new();
    writeln!(
        &mut out,
        "-- Generated by shki bootstrap from live database"
    )
    .expect("writing to String cannot fail");
    writeln!(
        &mut out,
        "-- Review and adjust this file before committing."
    )
    .expect("writing to String cannot fail");
    out.push('\n');

    let (dialect_mod, root_schema): (&str, Option<String>) = match dialect {
        SchemaDialect::Postgres => (
            "pg",
            Some(
                snapshot
                    .schemas
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "public".to_string()),
            ),
        ),
        SchemaDialect::Mysql => (
            "mysql",
            Some(
                snapshot
                    .schemas
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "mydb".to_string()),
            ),
        ),
        SchemaDialect::Sqlite => ("sqlite", None),
    };

    if let Some(schema) = &root_schema {
        writeln!(
            &mut out,
            "local schema = {}.schema({})",
            dialect_mod,
            lua_string(schema)
        )
        .expect("writing to String cannot fail");
    } else {
        writeln!(&mut out, "local schema = {}.schema()", dialect_mod)
            .expect("writing to String cannot fail");
    }

    out.push_str("local Table = TableBuilder\n");
    out.push_str("local Col = ColumnBuilder\n");
    out.push('\n');

    if snapshot.schemas.len() > 1 {
        out.push_str(
            "-- NOTE: multiple schemas detected; table-level :schema(...) is used when needed.\n\n",
        );
    }

    for enum_snapshot in snapshot.enums.values() {
        out.push_str("schema:enum(\n");
        writeln!(
            &mut out,
            "    EnumBuilder.new({})",
            lua_string(&enum_snapshot.name)
        )
        .expect("writing to String cannot fail");
        for value in &enum_snapshot.values {
            writeln!(&mut out, "        :value({})", lua_string(value))
                .expect("writing to String cannot fail");
        }
        out.push_str(")\n\n");
    }

    for table in snapshot.tables.values() {
        out.push_str("schema:table(\n");
        writeln!(&mut out, "    Table.new({})", lua_string(&table.name))
            .expect("writing to String cannot fail");

        if let Some(schema) = &table.schema
            && root_schema.as_ref() != Some(schema)
        {
            writeln!(&mut out, "        :schema({})", lua_string(schema))
                .expect("writing to String cannot fail");
        }

        if let Some(comment) = &table.comment {
            writeln!(&mut out, "        :comment({})", lua_string(comment))
                .expect("writing to String cannot fail");
        }

        for column in table.columns.values() {
            let mut column_expr = lua_column_builder_expr(column, snapshot);

            if !column.nullable {
                column_expr.push_str(":not_null()");
            }
            if column.primary_key {
                column_expr.push_str(":primary_key()");
            } else if column.unique {
                column_expr.push_str(":unique()");
            }
            if let Some(default) = &column.default {
                column_expr.push_str(":default_sql(");
                column_expr.push_str(&lua_string(default));
                column_expr.push(')');
            }
            if let Some(comment) = &column.comment {
                column_expr.push_str(":comment(");
                column_expr.push_str(&lua_string(comment));
                column_expr.push(')');
            }
            if let Some(collation) = &column.collation {
                column_expr.push_str(":collate(");
                column_expr.push_str(&lua_string(collation));
                column_expr.push(')');
            }

            writeln!(&mut out, "        :column({})", column_expr)
                .expect("writing to String cannot fail");

            if column.generated.is_some() || column.identity.is_some() {
                out.push_str("        -- NOTE: generated/identity details require manual review\n");
            }
        }

        for constraint in &table.constraints {
            match constraint.constraint_type {
                ConstraintType::PrimaryKey => {
                    if !constraint.columns.is_empty() {
                        writeln!(
                            &mut out,
                            "        :primary_key({})",
                            lua_string_vec(&constraint.columns)
                        )
                        .expect("writing to String cannot fail");
                    }
                }
                ConstraintType::Unique => {
                    if !constraint.columns.is_empty() {
                        writeln!(
                            &mut out,
                            "        :unique_constraint({})",
                            lua_string_vec(&constraint.columns)
                        )
                        .expect("writing to String cannot fail");
                    }
                }
                ConstraintType::ForeignKey => {
                    if let Some(reference) = &constraint.references {
                        writeln!(
                            &mut out,
                            "        :foreign_key_with_actions({}, {}, {}, {}, {})",
                            lua_string_vec(&constraint.columns),
                            lua_string(&reference.table),
                            lua_string_vec(&reference.columns),
                            lua_string(&normalize_ref_action(&reference.on_delete)),
                            lua_string(&normalize_ref_action(&reference.on_update)),
                        )
                        .expect("writing to String cannot fail");
                    }
                }
                ConstraintType::Check => {
                    if let Some(expr) = &constraint.expression {
                        writeln!(&mut out, "        :check({})", lua_string(expr))
                            .expect("writing to String cannot fail");
                    }
                }
                ConstraintType::Exclusion => {
                    out.push_str(
                        "        -- NOTE: exclusion constraints require manual Lua mapping\n",
                    );
                }
            }
        }

        for index in table.indexes.values() {
            if index.where_clause.is_none() && index.include.is_empty() && index.method == "btree" {
                if index.unique {
                    writeln!(
                        &mut out,
                        "        :unique_index({}, {})",
                        lua_string(&index.name),
                        lua_string_vec(&index.columns)
                    )
                    .expect("writing to String cannot fail");
                } else {
                    writeln!(
                        &mut out,
                        "        :index({}, {})",
                        lua_string(&index.name),
                        lua_string_vec(&index.columns)
                    )
                    .expect("writing to String cannot fail");
                }
            } else {
                out.push_str("        -- NOTE: complex index (method/where/include) requires manual Lua mapping\n");
            }
        }

        out.push_str(")\n\n");
    }

    out.push_str("return schema\n");
    out
}

fn lua_column_builder_expr(column: &ColumnSnapshot, snapshot: &Snapshot) -> String {
    let name = lua_string(&column.name);
    let ty = column.data_type.trim();
    let upper = ty.to_ascii_uppercase();

    if upper.ends_with("[]") {
        let element = upper.trim_end_matches("[]").to_ascii_lowercase();
        return format!("Col.array({}, {})", name, lua_string(&element));
    }

    if snapshot.enums.contains_key(ty) {
        return format!("Col.enum({}, {})", name, lua_string(ty));
    }

    if let Some((len_open, len_close)) = upper
        .strip_prefix("VARCHAR(")
        .and_then(|rest| rest.find(')').map(|idx| (0usize, idx)))
    {
        let len = upper[8 + len_open..8 + len_close].trim();
        if let Ok(parsed) = len.parse::<u32>() {
            return format!("Col.varchar({}, {})", name, parsed);
        }
    }

    if let Some((len_open, len_close)) = upper
        .strip_prefix("CHAR(")
        .and_then(|rest| rest.find(')').map(|idx| (0usize, idx)))
    {
        let len = upper[5 + len_open..5 + len_close].trim();
        if let Ok(parsed) = len.parse::<u32>() {
            return format!("Col.char({}, {})", name, parsed);
        }
    }

    match upper.as_str() {
        "SMALLINT" => format!("Col.smallint({})", name),
        "INTEGER" => format!("Col.integer({})", name),
        "BIGINT" => format!("Col.bigint({})", name),
        "TEXT" => format!("Col.text({})", name),
        "BOOLEAN" => format!("Col.boolean({})", name),
        "UUID" => format!("Col.uuid({})", name),
        "JSON" => format!("Col.json({})", name),
        "JSONB" => format!("Col.jsonb({})", name),
        "DATE" => format!("Col.date({})", name),
        "TIMESTAMP" => format!("Col.timestamp({})", name),
        "TIMESTAMPTZ" => format!("Col.timestamptz({})", name),
        "TIME" => format!("Col.time({})", name),
        "BYTEA" => format!("Col.bytea({})", name),
        "INET" => format!("Col.inet({})", name),
        "CIDR" => format!("Col.cidr({})", name),
        "REAL" => format!("Col.real({})", name),
        "DOUBLE PRECISION" => format!("Col.double_precision({})", name),
        _ => format!(
            "Col.new({}, {})",
            name,
            lua_string(&upper.to_ascii_lowercase())
        ),
    }
}

fn lua_string(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

fn lua_string_vec(values: &[String]) -> String {
    let mut out = String::from("{");
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&lua_string(value));
    }
    out.push('}');
    out
}

fn normalize_ref_action(action: &str) -> String {
    action.trim().to_ascii_lowercase().replace(' ', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{EnumSnapshot, TableSnapshot};
    use indexmap::IndexMap;

    #[test]
    fn test_parse_qualified_table() {
        let qualified = parse_qualified_table("public.schema_migrations");
        assert_eq!(qualified.schema.as_deref(), Some("public"));
        assert_eq!(qualified.table, "schema_migrations");

        let qualified = parse_qualified_table("\"public\".\"schema_migrations\"");
        assert_eq!(qualified.schema.as_deref(), Some("public"));
        assert_eq!(qualified.table, "schema_migrations");

        let qualified = parse_qualified_table("`schema_migrations`");
        assert_eq!(qualified.schema, None);
        assert_eq!(qualified.table, "schema_migrations");
    }

    #[test]
    fn test_drop_table_if_exists_sql_qualified() {
        let pg = drop_table_if_exists_sql(SchemaDialect::Postgres, Some("public"), "legacy");
        assert_eq!(pg, "DROP TABLE IF EXISTS \"public\".\"legacy\"");

        let mysql = drop_table_if_exists_sql(SchemaDialect::Mysql, Some("mydb"), "legacy");
        assert_eq!(mysql, "DROP TABLE IF EXISTS `mydb`.`legacy`");
    }

    #[test]
    fn test_render_lua_schema_basic() {
        let mut snapshot = Snapshot::new(SchemaDialect::Postgres);
        snapshot.schemas.push("public".to_string());
        snapshot.enums.insert(
            "status".to_string(),
            EnumSnapshot {
                name: "status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["active".to_string(), "inactive".to_string()],
                description: None,
            },
        );

        let mut columns = IndexMap::new();
        columns.insert(
            "id".to_string(),
            ColumnSnapshot {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                default: None,
                primary_key: true,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );
        columns.insert(
            "state".to_string(),
            ColumnSnapshot {
                name: "state".to_string(),
                data_type: "status".to_string(),
                nullable: true,
                default: None,
                primary_key: false,
                unique: false,
                generated: None,
                identity: None,
                comment: None,
                collation: None,
            },
        );

        snapshot.tables.insert(
            "users".to_string(),
            TableSnapshot {
                name: "users".to_string(),
                schema: Some("public".to_string()),
                columns,
                constraints: Vec::new(),
                indexes: IndexMap::new(),
                comment: None,
            },
        );

        let lua = render_lua_schema(&snapshot, SchemaDialect::Postgres);
        assert!(lua.contains("local schema = pg.schema(\"public\")"));
        assert!(lua.contains("EnumBuilder.new(\"status\")"));
        assert!(lua.contains(":column(Col.integer(\"id\"):not_null():primary_key())"));
        assert!(lua.contains(":column(Col.enum(\"state\", \"status\"))"));
    }
}
