use crate::Result;
use crate::engines::sqlite::Sqlite;
use crate::models::iden::Iden;
use crate::queries::sqlite::snapshot as sqlite_snapshot_queries;
use crate::schema::{
    CheckConstraint, Column, Constraint, DataType, DbEnum, ForeignKeyConstraint, Index,
    IndexColumn, IndexMethod, PrimaryKeyConstraint, Sequence, SqlDialect, Table, UniqueConstraint,
    View, ViewColumn,
};
use crate::snapshots::SnapshotProvider;
use indexmap::IndexMap;

use super::utils::{parse_default_value, parse_reference_action, take_parenthesized};

#[derive(Clone, sqlx::FromRow)]
struct SqliteSchemaRow {
    name: String,
}

#[derive(Clone, sqlx::FromRow)]
struct SqliteObjectRow {
    name: String,
    #[sqlx(rename = "type")]
    object_type: String,
    sql: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct SqliteTableInfoRow {
    name: String,
    #[sqlx(rename = "type")]
    data_type: String,
    notnull: i64,
    dflt_value: Option<String>,
    pk: i64,
}

#[derive(Clone, sqlx::FromRow)]
struct SqliteForeignKeyRow {
    id: i64,
    seq: i64,
    table: String,
    from: String,
    to: Option<String>,
    on_update: String,
    on_delete: String,
}

#[derive(Clone, sqlx::FromRow)]
struct SqliteIndexListRow {
    name: String,
    unique: i64,
    origin: String,
    partial: i64,
}

#[derive(Clone, sqlx::FromRow)]
struct SqliteIndexXinfoRow {
    seqno: i64,
    cid: i64,
    name: Option<String>,
    desc: i64,
    key: i64,
}

#[async_trait::async_trait]
impl SnapshotProvider for Sqlite {
    async fn get_schemas(&self, _schema: &Option<String>) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, SqliteSchemaRow>(sqlite_snapshot_queries::SCHEMAS)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| row.name).collect())
    }

    async fn get_extensions(&self, _schema: &Option<String>) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn get_enums(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, DbEnum>> {
        Ok(IndexMap::new())
    }

    async fn get_sequences(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Sequence>> {
        Ok(IndexMap::new())
    }

    async fn get_tables(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, Table>> {
        let rows = user_objects(&self.pool, "table").await?;
        let mut tables = IndexMap::new();

        for row in rows {
            let schema = object_schema(row.name.as_str());
            tables.insert(
                Iden::new(row.name.clone(), schema.clone()),
                Table {
                    name: row.name,
                    schema,
                    ..Default::default()
                },
            );
        }

        Ok(tables)
    }

    async fn get_views(&self, _schema: &Option<String>) -> Result<IndexMap<Iden, View>> {
        let rows = user_objects(&self.pool, "view").await?;
        let mut views = IndexMap::new();

        for row in rows {
            let schema = object_schema(row.name.as_str());
            let columns = sqlite_columns(&self.pool, &row.name)
                .await?
                .into_iter()
                .map(|column| ViewColumn {
                    name: column.name,
                    data_type: column.data_type,
                })
                .collect();
            views.insert(
                Iden::new(row.name.clone(), schema.clone()),
                View {
                    name: row.name,
                    schema,
                    definition: row.sql.unwrap_or_default(),
                    materialized: false,
                    columns,
                },
            );
        }

        Ok(views)
    }

    async fn get_columns(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Column>>> {
        let mut columns_by_table = IndexMap::new();

        for table in user_objects(&self.pool, "table").await? {
            let table_id = Iden::new(table.name.clone(), object_schema(&table.name));
            let mut columns = IndexMap::new();
            for column in sqlite_columns(&self.pool, &table.name).await? {
                columns.insert(column.name.clone(), column);
            }
            columns_by_table.insert(table_id, columns);
        }

        Ok(columns_by_table)
    }
    async fn get_constraints(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, Vec<Constraint>>> {
        let mut constraints_by_table = IndexMap::new();

        for table in user_objects(&self.pool, "table").await? {
            let table_id = Iden::new(table.name.clone(), object_schema(&table.name));
            let table_sql = table.sql.unwrap_or_default();
            let mut constraints = Vec::new();

            let columns = sqlite_table_info(&self.pool, &table.name).await?;
            let mut pk_columns: Vec<(i64, String)> = columns
                .iter()
                .filter(|column| column.pk > 0)
                .map(|column| (column.pk, column.name.clone()))
                .collect();
            pk_columns.sort_by_key(|(position, _)| *position);
            let pk_columns: Vec<String> = pk_columns.into_iter().map(|(_, name)| name).collect();
            if !pk_columns.is_empty() {
                constraints.push(Constraint::PrimaryKey(PrimaryKeyConstraint {
                    name: None,
                    columns: pk_columns,
                }));
            }

            let mut unique_indexes = sqlite_index_list(&self.pool, &table.name)
                .await?
                .into_iter()
                .filter(|index| index.unique != 0 && index.origin == "u")
                .collect::<Vec<_>>();
            unique_indexes.sort_by(|a, b| a.name.cmp(&b.name));
            for index in unique_indexes {
                let columns = sqlite_index_columns(&self.pool, &index.name).await?;
                constraints.push(Constraint::Unique(UniqueConstraint {
                    name: Some(index.name),
                    columns,
                    nulls_distinct: true,
                }));
            }

            for foreign_key in sqlite_foreign_keys(&self.pool, &table.name).await? {
                constraints.push(foreign_key);
            }

            for check in parse_sqlite_check_constraints(&table_sql) {
                constraints.push(Constraint::Check(CheckConstraint {
                    name: None,
                    expression: check,
                }));
            }

            constraints_by_table.insert(table_id, constraints);
        }

        Ok(constraints_by_table)
    }
    async fn get_indexes(
        &self,
        _schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, crate::schema::Index>>> {
        let mut indexes_by_table = IndexMap::new();

        for table in user_objects(&self.pool, "table").await? {
            let table_id = Iden::new(table.name.clone(), object_schema(&table.name));
            let mut indexes = IndexMap::new();
            for index_row in sqlite_index_list(&self.pool, &table.name).await? {
                if index_row.origin != "c" {
                    continue;
                }

                let columns = sqlite_index_columns(&self.pool, &index_row.name)
                    .await?
                    .into_iter()
                    .map(IndexColumn::column)
                    .collect();
                indexes.insert(
                    index_row.name.clone(),
                    Index {
                        name: index_row.name,
                        columns,
                        unique: index_row.unique != 0,
                        method: IndexMethod::BTree,
                        where_clause: (index_row.partial != 0).then_some(String::new()),
                        options: Vec::new(),
                        is_constraint: false,
                        concurrently: false,
                        include: Vec::new(),
                        tablespace: None,
                    },
                );
            }
            indexes_by_table.insert(table_id, indexes);
        }

        Ok(indexes_by_table)
    }
}

async fn user_objects(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    object_type: &str,
) -> Result<Vec<SqliteObjectRow>> {
    sqlx::query_as::<_, SqliteObjectRow>(
        r#"
        SELECT name, type, sql
        FROM sqlite_schema
        WHERE type = ?
            AND name NOT LIKE 'sqlite_%'
        ORDER BY name
        "#,
    )
    .bind(object_type)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

async fn sqlite_table_info(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    table: &str,
) -> Result<Vec<SqliteTableInfoRow>> {
    let query = format!("PRAGMA table_xinfo({})", quote_sqlite_string(table));
    sqlx::query_as::<_, SqliteTableInfoRow>(&query)
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}

async fn sqlite_columns(pool: &sqlx::Pool<sqlx::Sqlite>, table: &str) -> Result<Vec<Column>> {
    Ok(sqlite_table_info(pool, table)
        .await?
        .into_iter()
        .map(|row| Column {
            name: row.name,
            data_type: DataType::parse(row.data_type, &SqlDialect::Sqlite),
            nullable: row.notnull == 0 && row.pk == 0,
            default: row.dflt_value.map(parse_default_value),
            primary_key: row.pk > 0,
            unique: false,
            generated: None,
            comment: None,
            collation: None,
            identity: None,
            references: None,
        })
        .collect())
}

async fn sqlite_index_list(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    table: &str,
) -> Result<Vec<SqliteIndexListRow>> {
    let query = format!("PRAGMA index_list({})", quote_sqlite_string(table));
    sqlx::query_as::<_, SqliteIndexListRow>(&query)
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}

async fn sqlite_index_columns(pool: &sqlx::Pool<sqlx::Sqlite>, index: &str) -> Result<Vec<String>> {
    let query = format!("PRAGMA index_xinfo({})", quote_sqlite_string(index));
    let mut rows = sqlx::query_as::<_, SqliteIndexXinfoRow>(&query)
        .fetch_all(pool)
        .await?;
    rows.sort_by_key(|row| row.seqno);

    Ok(rows
        .into_iter()
        .filter(|row| row.key != 0 && row.cid >= 0)
        .filter_map(|row| row.name)
        .collect())
}

async fn sqlite_foreign_keys(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    table: &str,
) -> Result<Vec<Constraint>> {
    let query = format!("PRAGMA foreign_key_list({})", quote_sqlite_string(table));
    let mut rows = sqlx::query_as::<_, SqliteForeignKeyRow>(&query)
        .fetch_all(pool)
        .await?;
    rows.sort_by_key(|row| (row.id, row.seq));

    let mut by_id: IndexMap<i64, ForeignKeyConstraint> = IndexMap::new();
    for row in rows {
        let entry = by_id.entry(row.id).or_insert_with(|| ForeignKeyConstraint {
            name: None,
            columns: Vec::new(),
            references: Iden::new(row.table.clone(), None),
            references_columns: Vec::new(),
            on_delete: parse_reference_action(Some(&row.on_delete)),
            on_update: parse_reference_action(Some(&row.on_update)),
            deferrable: false,
            initially_deferred: false,
        });
        entry.columns.push(row.from);
        entry.references_columns.push(row.to.unwrap_or_default());
    }

    Ok(by_id
        .into_values()
        .map(Constraint::ForeignKey)
        .collect::<Vec<_>>())
}

fn parse_sqlite_check_constraints(sql: &str) -> Vec<String> {
    let mut checks = Vec::new();
    let mut rest = sql;

    while let Some(offset) = rest.to_ascii_lowercase().find("check") {
        rest = &rest[offset + "check".len()..];
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('(') {
            continue;
        }
        if let Some((expression, tail)) = take_parenthesized(trimmed) {
            checks.push(expression.to_string());
            rest = tail;
        } else {
            break;
        }
    }

    checks
}

fn object_schema(name: &str) -> Option<String> {
    let _ = name;
    None
}

fn quote_sqlite_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DefaultValue, IndexColumn};

    async fn test_engine() -> Sqlite {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("sqlite pool should connect");

        sqlx::raw_sql(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                age INTEGER CHECK (age >= 0),
                status TEXT DEFAULT 'active'
            );

            CREATE TABLE posts (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                UNIQUE(user_id, title)
            );

            CREATE INDEX posts_title_idx ON posts(title);
            CREATE VIEW post_titles AS SELECT title FROM posts;
            "#,
        )
        .execute(&pool)
        .await
        .expect("sqlite schema should be created");

        Sqlite::new(pool, Iden::new("__shki_migrations", None))
    }

    #[tokio::test]
    async fn sqlite_provider_introspects_schema_objects() {
        let engine = test_engine().await;

        let schemas = engine.get_schemas(&None).await.expect("schemas");
        assert_eq!(schemas, vec!["main"]);

        let tables = engine.get_tables(&None).await.expect("tables");
        assert!(tables.contains_key(&Iden::new("users", None)));
        assert!(tables.contains_key(&Iden::new("posts", None)));

        let columns = engine.get_columns(&None).await.expect("columns");
        let user_columns = columns
            .get(&Iden::new("users", None))
            .expect("users columns should be introspected");
        assert!(user_columns.get("id").expect("id column").primary_key);
        assert!(!user_columns.get("email").expect("email column").nullable);
        assert!(matches!(
            user_columns.get("status").expect("status column").default,
            Some(DefaultValue::Literal(ref value)) if value == "active"
        ));

        let constraints = engine.get_constraints(&None).await.expect("constraints");
        let user_constraints = constraints
            .get(&Iden::new("users", None))
            .expect("users constraints should be introspected");
        assert!(
            user_constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::PrimaryKey(pk) if pk.columns == vec!["id"]))
        );
        assert!(
            user_constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::Unique(unique) if unique.columns == vec!["email"]))
        );
        assert!(
            user_constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::Check(check) if check.expression == "age >= 0"))
        );

        let post_constraints = constraints
            .get(&Iden::new("posts", None))
            .expect("posts constraints should be introspected");
        assert!(post_constraints.iter().any(|constraint| {
            matches!(
                constraint,
                Constraint::ForeignKey(fk)
                    if fk.columns == vec!["user_id"]
                        && fk.references == Iden::new("users", None)
                        && fk.references_columns == vec!["id"]
                        && fk.on_delete == crate::schema::ReferenceAction::Cascade
            )
        }));

        let indexes = engine.get_indexes(&None).await.expect("indexes");
        let post_indexes = indexes
            .get(&Iden::new("posts", None))
            .expect("posts indexes should be introspected");
        let title_index = post_indexes
            .get("posts_title_idx")
            .expect("explicit index should be introspected");
        assert!(matches!(
            title_index.columns.as_slice(),
            [IndexColumn::Column { name, .. }] if name == "title"
        ));

        let views = engine.get_views(&None).await.expect("views");
        let view = views
            .get(&Iden::new("post_titles", None))
            .expect("view should be introspected");
        assert_eq!(view.columns[0].name, "title");
    }
}
