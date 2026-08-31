use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::models::iden::Iden;
use crate::schema::{Constraint, Table};
use crate::sql::planner::order_statements;
use crate::sql::render::{SqlObjectType, SqlOperation, SqlStmt};
use crate::{Result, ShkiError};

use super::sql::parse::{
    create_statement_object_type, create_statement_operation, create_table_info,
    is_alter_table_add_foreign_key, join_sql_statements, parse_include_directive,
    rewrite_create_index_concurrently, rewrite_create_table_foreign_keys, split_sql_statements,
};

pub const DIRECTORY_SCHEMA_ENTRYPOINT: &str = "main.sql";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeSchema {
    pub entrypoint: PathBuf,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeApplySql {
    pub setup_sql: String,
    pub deferred_sql: String,
    /// Names of indexes declared `CREATE INDEX CONCURRENTLY`. The keyword is
    /// stripped from the apply SQL (it can't run in the Shadow Database's
    /// transaction and isn't recorded in catalogs), so the declared intent is
    /// carried here instead.
    pub concurrent_indexes: Vec<String>,
}

pub fn normalize_declarative_apply_sql(sql: &str) -> Result<String> {
    let plan = plan_declarative_apply_sql(sql)?;
    Ok([plan.setup_sql, plan.deferred_sql]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn plan_declarative_apply_sql(sql: &str) -> Result<DeclarativeApplySql> {
    let mut setup = Vec::new();
    let mut deferred = Vec::new();
    let mut concurrent_indexes = Vec::new();

    for (idx, statement) in split_sql_statements(sql)?.into_iter().enumerate() {
        let statement = match rewrite_create_index_concurrently(&statement)? {
            Some(rewritten) => {
                concurrent_indexes.push(rewritten.index_name);
                rewritten.sql
            }
            None => statement,
        };
        if let Some(rewritten) = rewrite_create_table_foreign_keys(&statement)? {
            setup.push(plan_setup_statement(idx, rewritten.create_table_sql)?);
            deferred.extend(
                rewritten
                    .deferred_foreign_keys
                    .into_iter()
                    .map(SqlStmt::from),
            );
        } else if is_alter_table_add_foreign_key(&statement) {
            deferred.push(SqlStmt::from(statement));
        } else {
            setup.push(plan_setup_statement(idx, statement)?);
        }
    }

    let setup = order_statements(setup);

    Ok(DeclarativeApplySql {
        setup_sql: join_sql_statements(&setup),
        deferred_sql: join_sql_statements(&deferred),
        concurrent_indexes,
    })
}

fn plan_setup_statement(idx: usize, statement: String) -> Result<SqlStmt> {
    if let Some(table) = create_table_info(&statement)? {
        return Ok(SqlStmt::from(statement)
            .with_planning(SqlObjectType::Table, SqlOperation::Create, idx)
            .with_identity(table.id())
            .with_dependencies(table_dependencies(&table)));
    }

    let object_type = create_statement_object_type(&statement);
    let operation = create_statement_operation(&statement);
    Ok(SqlStmt::from(statement).with_planning(object_type, operation, idx))
}

fn table_dependencies(table: &Table) -> Vec<Iden> {
    table
        .constraints
        .iter()
        .filter_map(|constraint| match constraint {
            Constraint::ForeignKey(fk) => Some(fk.references.clone()),
            _ => None,
        })
        .collect()
}

pub fn load_declarative_schema(path: impl AsRef<Path>) -> Result<DeclarativeSchema> {
    let path = path.as_ref();
    let entrypoint = if path.is_dir() {
        path.join(DIRECTORY_SCHEMA_ENTRYPOINT)
    } else {
        path.to_path_buf()
    };

    if !entrypoint.exists() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema entrypoint does not exist: {}",
            entrypoint.display()
        )));
    }

    if !entrypoint.is_file() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema entrypoint is not a file: {}",
            entrypoint.display()
        )));
    }

    let mut loading = Vec::new();
    let mut loaded = HashSet::new();
    let sql = load_sql_file(&entrypoint, &mut loading, &mut loaded)?;

    Ok(DeclarativeSchema { entrypoint, sql })
}

fn load_sql_file(
    path: &Path,
    loading: &mut Vec<PathBuf>,
    loaded: &mut HashSet<PathBuf>,
) -> Result<String> {
    let canonical = canonicalize_existing_file(path)?;

    if let Some(index) = loading.iter().position(|active| active == &canonical) {
        let mut cycle = loading[index..]
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        cycle.push(canonical.display().to_string());
        return Err(ShkiError::schema(format!(
            "Cyclic Declarative Schema include detected: {}",
            cycle.join(" -> ")
        )));
    }

    if loaded.contains(&canonical) {
        return Ok(String::new());
    }

    loading.push(canonical.clone());
    let content = std::fs::read_to_string(&canonical)?;
    let mut expanded = String::new();

    for line in content.lines() {
        if let Some(include_path) = parse_include_directive(line)? {
            let include_path = canonical
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(include_path);
            expanded.push_str(&load_sql_file(&include_path, loading, loaded)?);
            if !expanded.ends_with('\n') {
                expanded.push('\n');
            }
        } else {
            expanded.push_str(line);
            expanded.push('\n');
        }
    }

    loading.pop();
    loaded.insert(canonical);
    Ok(expanded)
}

fn canonicalize_existing_file(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|err| {
        ShkiError::schema(format!(
            "Failed to read Declarative Schema file {}: {}",
            path.display(),
            err
        ))
    })?;

    if !canonical.is_file() {
        return Err(ShkiError::schema(format!(
            "Declarative Schema include is not a file: {}",
            canonical.display()
        )));
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn strips_concurrently_from_apply_sql_and_records_index_names() {
        let sql = r#"
CREATE TABLE users (id int PRIMARY KEY, email text);
CREATE INDEX CONCURRENTLY users_email_idx ON users (email);
CREATE INDEX users_id_idx ON users (id);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.concurrent_indexes, vec!["users_email_idx".to_string()]);
        assert!(!plan.setup_sql.contains("CONCURRENTLY"));
        assert!(plan.setup_sql.contains("CREATE INDEX users_email_idx"));
        assert!(plan.setup_sql.contains("CREATE INDEX users_id_idx"));
    }

    #[test]
    fn loads_single_sql_file() {
        let temp = TempDir::new().expect("temp dir");
        let schema = temp.path().join("schema.sql");
        std::fs::write(&schema, "CREATE TABLE users (id int);\n").expect("write schema");

        let loaded = load_declarative_schema(&schema).expect("load schema");

        assert_eq!(loaded.entrypoint, schema);
        assert_eq!(loaded.sql, "CREATE TABLE users (id int);\n");
    }

    #[test]
    fn loads_directory_schema_from_main_sql() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "SELECT 1;\n").expect("write main");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(loaded.entrypoint, temp.path().join("main.sql"));
        assert_eq!(loaded.sql, "SELECT 1;\n");
    }

    #[test]
    fn expands_relative_i_includes() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::create_dir(temp.path().join("tables")).expect("create tables dir");
        std::fs::write(
            temp.path().join("main.sql"),
            "CREATE SCHEMA app;\n\\i tables/users.sql\n",
        )
        .expect("write main");
        std::fs::write(
            temp.path().join("tables/users.sql"),
            "CREATE TABLE app.users (id int);\n",
        )
        .expect("write users");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(
            loaded.sql,
            "CREATE SCHEMA app;\nCREATE TABLE app.users (id int);\n"
        );
    }

    #[test]
    fn supports_quoted_include_paths() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "\\i 'user table.sql'\n").expect("write main");
        std::fs::write(temp.path().join("user table.sql"), "SELECT 1;\n").expect("write file");

        let loaded = load_declarative_schema(temp.path()).expect("load schema");

        assert_eq!(loaded.sql, "SELECT 1;\n");
    }

    #[test]
    fn rejects_include_cycles() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("main.sql"), "\\i a.sql\n").expect("write main");
        std::fs::write(temp.path().join("a.sql"), "\\i b.sql\n").expect("write a");
        std::fs::write(temp.path().join("b.sql"), "\\i a.sql\n").expect("write b");

        let error = load_declarative_schema(temp.path()).expect_err("cycle should fail");

        assert!(
            error
                .to_string()
                .contains("Cyclic Declarative Schema include")
        );
    }

    #[test]
    fn rejects_unsupported_backslash_commands() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::write(temp.path().join("schema.sql"), "\\ir other.sql\n").expect("write schema");

        let error = load_declarative_schema(temp.path().join("schema.sql"))
            .expect_err("unsupported command should fail");

        assert!(
            error
                .to_string()
                .contains("Only `\\i` includes are supported")
        );
    }

    #[test]
    fn normalizes_inline_table_foreign_keys_to_deferred_alter_table() {
        let sql = r#"
CREATE TABLE "public"."enrollment_session_to_fingerprint" (
  "enrollment_session_id" UUID NOT NULL,
  "fingerprint_id" UUID NOT NULL,
  CONSTRAINT "fk_session" FOREIGN KEY ("enrollment_session_id") REFERENCES "public"."enrollment_session" ("id"),
  CONSTRAINT "fk_fingerprint" FOREIGN KEY ("fingerprint_id") REFERENCES "public"."fingerprint" ("id"),
  CONSTRAINT "pk_join" PRIMARY KEY ("enrollment_session_id", "fingerprint_id")
);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert!(
            plan.setup_sql
                .contains("CONSTRAINT \"pk_join\" PRIMARY KEY")
        );
        assert!(
            !plan
                .setup_sql
                .contains("CONSTRAINT \"fk_session\" FOREIGN KEY")
        );
        assert!(plan.deferred_sql.contains(
            "ALTER TABLE \"public\".\"enrollment_session_to_fingerprint\" ADD CONSTRAINT \"fk_session\" FOREIGN KEY"
        ));
        assert!(plan.deferred_sql.contains(
            "ALTER TABLE \"public\".\"enrollment_session_to_fingerprint\" ADD CONSTRAINT \"fk_fingerprint\" FOREIGN KEY"
        ));
    }

    #[test]
    fn normalizes_standalone_alter_table_foreign_keys_to_deferred_sql() {
        let sql = r#"
ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (parent_id) REFERENCES parent(id);
CREATE TABLE child (parent_id int);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.setup_sql, "CREATE TABLE child (parent_id int);");
        assert_eq!(
            plan.deferred_sql,
            "ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (parent_id) REFERENCES parent(id);"
        );
    }

    #[test]
    fn normalizes_create_tables_in_dependency_order() {
        let sql = r#"
CREATE SCHEMA app;
CREATE TABLE app.child (
  id int,
  parent_id int REFERENCES app.parent(id)
);
CREATE TABLE app.parent (id int PRIMARY KEY);
CREATE INDEX child_parent_id_idx ON app.child (parent_id);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        let schema_pos = plan.setup_sql.find("CREATE SCHEMA app").unwrap();
        let parent_pos = plan.setup_sql.find("CREATE TABLE app.parent").unwrap();
        let child_pos = plan.setup_sql.find("CREATE TABLE app.child").unwrap();
        let index_pos = plan
            .setup_sql
            .find("CREATE INDEX child_parent_id_idx")
            .unwrap();

        assert!(schema_pos < parent_pos);
        assert!(parent_pos < child_pos);
        assert!(child_pos < index_pos);
    }

    #[test]
    fn normalizes_create_types_before_dependent_tables() {
        let sql = r#"
CREATE TABLE "public"."events" (
  "id" int PRIMARY KEY,
  "status" "public"."event_status" NOT NULL
);
CREATE TYPE "public"."event_status" AS ENUM ('UNPUBLISHED', 'PUBLISHED', 'FAILED');
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        let type_pos = plan
            .setup_sql
            .find("CREATE TYPE \"public\".\"event_status\"")
            .unwrap();
        let table_pos = plan
            .setup_sql
            .find("CREATE TABLE \"public\".\"events\"")
            .unwrap();

        assert!(type_pos < table_pos);
    }

    #[test]
    fn normalizes_composite_types_before_dependent_tables() {
        let sql = r#"
CREATE TABLE "public"."events" (
  "id" int PRIMARY KEY,
  "location" "public"."point2d" NOT NULL
);
CREATE TYPE "public"."point2d" AS (
  "x" double precision,
  "y" double precision
);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        let type_pos = plan
            .setup_sql
            .find("CREATE TYPE \"public\".\"point2d\"")
            .unwrap();
        let table_pos = plan
            .setup_sql
            .find("CREATE TABLE \"public\".\"events\"")
            .unwrap();

        assert!(type_pos < table_pos);
    }

    #[test]
    fn normalizes_setup_statements_by_object_dependency_order() {
        let sql = r#"
CREATE INDEX child_parent_id_idx ON app.child (parent_id);
CREATE VIEW app.child_names AS SELECT id FROM app.child;
CREATE TABLE app.child (id int PRIMARY KEY, parent_id int REFERENCES app.parent(id));
CREATE SEQUENCE app.child_id_seq;
CREATE FUNCTION app.one() RETURNS int LANGUAGE SQL AS 'SELECT 1';
CREATE TYPE app.status AS ENUM ('active');
CREATE TABLE app.parent (id int PRIMARY KEY);
CREATE SCHEMA app;
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        let schema_pos = plan.setup_sql.find("CREATE SCHEMA app").unwrap();
        let type_pos = plan.setup_sql.find("CREATE TYPE app.status").unwrap();
        let function_pos = plan.setup_sql.find("CREATE FUNCTION app.one").unwrap();
        let sequence_pos = plan
            .setup_sql
            .find("CREATE SEQUENCE app.child_id_seq")
            .unwrap();
        let parent_pos = plan.setup_sql.find("CREATE TABLE app.parent").unwrap();
        let child_pos = plan.setup_sql.find("CREATE TABLE app.child").unwrap();
        let view_pos = plan.setup_sql.find("CREATE VIEW app.child_names").unwrap();
        let index_pos = plan
            .setup_sql
            .find("CREATE INDEX child_parent_id_idx")
            .unwrap();

        assert!(schema_pos < type_pos);
        assert!(type_pos < function_pos);
        assert!(function_pos < sequence_pos);
        assert!(sequence_pos < parent_pos);
        assert!(parent_pos < child_pos);
        assert!(child_pos < view_pos);
        assert!(view_pos < index_pos);
    }

    #[test]
    fn normalizes_quoted_foreign_key_constraint_names() {
        let sql = r#"
CREATE TABLE child (
  parent_id int,
  CONSTRAINT "child_parent_fkey" FOREIGN KEY (parent_id) REFERENCES parent(id)
);
"#;

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.setup_sql, "CREATE TABLE child (\nparent_id int\n);");
        assert_eq!(
            plan.deferred_sql,
            "ALTER TABLE child ADD CONSTRAINT \"child_parent_fkey\" FOREIGN KEY (parent_id) REFERENCES parent(id);"
        );
    }

    #[test]
    fn alter_table_literals_do_not_trigger_foreign_key_deferral() {
        let sql = "ALTER TABLE child ADD COLUMN note text DEFAULT 'FOREIGN KEY';";

        let plan = plan_declarative_apply_sql(sql).expect("sql should normalize");

        assert_eq!(plan.setup_sql, sql);
        assert!(plan.deferred_sql.is_empty());
    }

    #[test]
    fn normalizer_preserves_commas_and_semicolons_inside_sql_literals() {
        let sql = r#"
CREATE TABLE child (
  id int,
  note text DEFAULT 'a,b;c',
  CONSTRAINT child_parent_fkey FOREIGN KEY (id) REFERENCES parent(id)
);
CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  RAISE NOTICE 'not a statement; still function body';
END;
$$;
"#;

        let normalized = normalize_declarative_apply_sql(sql).expect("sql should normalize");

        assert!(normalized.contains("note text DEFAULT 'a,b;c'"));
        assert!(normalized.contains("RAISE NOTICE 'not a statement; still function body';"));
        assert!(normalized.contains(
            "ALTER TABLE child ADD CONSTRAINT child_parent_fkey FOREIGN KEY (id) REFERENCES parent(id);"
        ));
    }
}
