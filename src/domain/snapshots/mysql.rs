use crate::engines::mysql::Mysql;
use crate::models::iden::Iden;
use crate::schema::{
    CheckConstraint, Column, Constraint, DataType, DbEnum, ForeignKeyConstraint, GeneratedColumn,
    Index, IndexColumn, PrimaryKeyConstraint, Sequence, SqlDialect, Table, UniqueConstraint, View,
    ViewColumn,
};
use crate::snapshots::SnapshotProvider;
use crate::{Result, ShkiError};
use indexmap::IndexMap;

use super::utils::{
    non_empty, parse_default_value, parse_index_method, parse_reference_action, parse_sort_order,
    push_unique,
};

#[derive(Clone, sqlx::FromRow)]
struct MysqlTableRow {
    table_schema: String,
    table_name: String,
    table_comment: String,
    engine: Option<String>,
    table_collation: Option<String>,
    create_options: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct MysqlViewRow {
    table_schema: String,
    table_name: String,
    view_definition: String,
    column_name: String,
    column_type: String,
}

#[derive(Clone, sqlx::FromRow)]
struct MysqlColumnRow {
    table_schema: String,
    table_name: String,
    column_name: String,
    column_type: String,
    is_nullable: String,
    column_default: Option<String>,
    extra: String,
    generation_expression: Option<String>,
    collation_name: Option<String>,
    column_comment: String,
    column_key: String,
}

#[derive(Clone, sqlx::FromRow)]
struct MysqlConstraintRow {
    table_schema: String,
    table_name: String,
    constraint_name: String,
    constraint_type: String,
    column_name: Option<String>,
    referenced_table_schema: Option<String>,
    referenced_table_name: Option<String>,
    referenced_column_name: Option<String>,
    update_rule: Option<String>,
    delete_rule: Option<String>,
    check_clause: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
struct MysqlIndexRow {
    table_schema: String,
    table_name: String,
    index_name: String,
    non_unique: i64,
    seq_in_index: u64,
    column_name: Option<String>,
    expression: Option<String>,
    collation: Option<String>,
    index_type: String,
}

#[async_trait::async_trait]
impl SnapshotProvider for Mysql {
    async fn get_schemas(&self, schema: &Option<String>) -> Result<Vec<String>> {
        let schema = target_schema(&self.pool, schema).await?;
        Ok(schema.into_iter().collect())
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

    async fn get_tables(&self, schema: &Option<String>) -> Result<IndexMap<Iden, Table>> {
        let rows = sqlx::query_as::<_, MysqlTableRow>(
            r#"
            SELECT table_schema, table_name, table_comment, engine, table_collation, create_options
            FROM information_schema.tables
            WHERE table_schema = ?
                AND table_type = 'BASE TABLE'
            ORDER BY table_schema, table_name
            "#,
        )
        .bind(required_schema(&self.pool, schema).await?)
        .fetch_all(&self.pool)
        .await?;

        let mut tables = IndexMap::new();
        for row in rows {
            let mut options = IndexMap::new();
            if let Some(engine) = row.engine {
                options.insert("engine".to_string(), engine);
            }
            if let Some(collation) = row.table_collation {
                options.insert("collation".to_string(), collation);
            }
            if let Some(create_options) = row.create_options
                && !create_options.is_empty()
            {
                options.insert("create_options".to_string(), create_options);
            }

            tables.insert(
                Iden::new(row.table_name.clone(), Some(row.table_schema.clone())),
                Table {
                    name: row.table_name,
                    schema: Some(row.table_schema),
                    comment: non_empty(row.table_comment),
                    options,
                    ..Default::default()
                },
            );
        }

        Ok(tables)
    }

    async fn get_views(&self, schema: &Option<String>) -> Result<IndexMap<Iden, View>> {
        let rows = sqlx::query_as::<_, MysqlViewRow>(
            r#"
            SELECT v.table_schema, v.table_name, v.view_definition, c.column_name, c.column_type
            FROM information_schema.views v
            JOIN information_schema.columns c
                ON c.table_schema = v.table_schema
                AND c.table_name = v.table_name
            WHERE v.table_schema = ?
            ORDER BY v.table_schema, v.table_name, c.ordinal_position
            "#,
        )
        .bind(required_schema(&self.pool, schema).await?)
        .fetch_all(&self.pool)
        .await?;

        let mut views = IndexMap::new();
        for row in rows {
            let key = Iden::new(row.table_name.clone(), Some(row.table_schema.clone()));
            let view = views.entry(key).or_insert_with(|| View {
                name: row.table_name.clone(),
                schema: Some(row.table_schema.clone()),
                definition: row.view_definition.clone(),
                materialized: false,
                columns: Vec::new(),
            });
            view.columns.push(ViewColumn {
                name: row.column_name,
                data_type: DataType::parse(row.column_type, &SqlDialect::Mysql)
                    .to_string(&SqlDialect::Mysql),
            });
        }

        Ok(views)
    }

    async fn get_columns(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, Column>>> {
        let rows = sqlx::query_as::<_, MysqlColumnRow>(
            r#"
            SELECT
                table_schema,
                table_name,
                column_name,
                column_type,
                is_nullable,
                column_default,
                extra,
                generation_expression,
                collation_name,
                column_comment,
                column_key
            FROM information_schema.columns
            WHERE table_schema = ?
            ORDER BY table_schema, table_name, ordinal_position
            "#,
        )
        .bind(required_schema(&self.pool, schema).await?)
        .fetch_all(&self.pool)
        .await?;

        let mut columns_by_table = IndexMap::new();
        for row in rows {
            let table_id = Iden::new(row.table_name.clone(), Some(row.table_schema.clone()));
            let column = Column::from(row);
            columns_by_table
                .entry(table_id)
                .or_insert_with(IndexMap::new)
                .insert(column.name.clone(), column);
        }

        Ok(columns_by_table)
    }
    async fn get_constraints(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, Vec<Constraint>>> {
        let rows = sqlx::query_as::<_, MysqlConstraintRow>(
            r#"
            SELECT
                tc.table_schema,
                tc.table_name,
                tc.constraint_name,
                tc.constraint_type,
                kcu.column_name,
                kcu.referenced_table_schema,
                kcu.referenced_table_name,
                kcu.referenced_column_name,
                rc.update_rule,
                rc.delete_rule,
                cc.check_clause
            FROM information_schema.table_constraints tc
            LEFT JOIN information_schema.key_column_usage kcu
                ON kcu.constraint_schema = tc.constraint_schema
                AND kcu.constraint_name = tc.constraint_name
                AND kcu.table_schema = tc.table_schema
                AND kcu.table_name = tc.table_name
            LEFT JOIN information_schema.referential_constraints rc
                ON rc.constraint_schema = tc.constraint_schema
                AND rc.constraint_name = tc.constraint_name
                AND rc.table_name = tc.table_name
            LEFT JOIN information_schema.check_constraints cc
                ON cc.constraint_schema = tc.constraint_schema
                AND cc.constraint_name = tc.constraint_name
            WHERE tc.table_schema = ?
                AND tc.constraint_type IN ('PRIMARY KEY', 'UNIQUE', 'FOREIGN KEY', 'CHECK')
            ORDER BY tc.table_schema, tc.table_name, tc.constraint_name, kcu.ordinal_position
            "#,
        )
        .bind(required_schema(&self.pool, schema).await?)
        .fetch_all(&self.pool)
        .await?;

        constraints_from_rows(rows)
    }
    async fn get_indexes(
        &self,
        schema: &Option<String>,
    ) -> Result<IndexMap<Iden, IndexMap<String, crate::schema::Index>>> {
        let rows = sqlx::query_as::<_, MysqlIndexRow>(
            r#"
            SELECT
                s.table_schema,
                s.table_name,
                s.index_name,
                s.non_unique,
                s.seq_in_index,
                s.column_name,
                s.expression,
                s.collation,
                s.index_type
            FROM information_schema.statistics s
            LEFT JOIN information_schema.table_constraints tc
                ON tc.table_schema = s.table_schema
                AND tc.table_name = s.table_name
                AND tc.constraint_name = s.index_name
                AND tc.constraint_type IN ('PRIMARY KEY', 'UNIQUE')
            WHERE s.table_schema = ?
                AND s.index_name != 'PRIMARY'
                AND tc.constraint_name IS NULL
            ORDER BY s.table_schema, s.table_name, s.index_name, s.seq_in_index
            "#,
        )
        .bind(required_schema(&self.pool, schema).await?)
        .fetch_all(&self.pool)
        .await?;

        indexes_from_rows(rows)
    }
}

impl From<MysqlColumnRow> for Column {
    fn from(row: MysqlColumnRow) -> Self {
        let auto_increment = row.extra.to_ascii_lowercase().contains("auto_increment");
        let data_type = if auto_increment {
            DataType::parse(
                format!("{} AUTO_INCREMENT", row.column_type),
                &SqlDialect::Mysql,
            )
        } else {
            DataType::parse(row.column_type, &SqlDialect::Mysql)
        };
        let generated = row
            .generation_expression
            .filter(|expression| !expression.is_empty())
            .map(|expression| GeneratedColumn {
                expression,
                stored: row.extra.to_ascii_lowercase().contains("stored"),
            });

        Column {
            name: row.column_name,
            data_type,
            nullable: row.is_nullable == "YES",
            default: row.column_default.map(parse_default_value),
            primary_key: row.column_key == "PRI",
            unique: row.column_key == "UNI",
            generated,
            comment: non_empty(row.column_comment),
            collation: row.collation_name,
            identity: None,
            references: None,
        }
    }
}

async fn target_schema(
    pool: &sqlx::Pool<sqlx::MySql>,
    schema: &Option<String>,
) -> Result<Option<String>> {
    if let Some(schema) = schema.as_deref()
        && schema != "public"
    {
        return Ok(Some(schema.to_string()));
    }

    sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
        .fetch_one(pool)
        .await
        .map_err(Into::into)
}

async fn required_schema(
    pool: &sqlx::Pool<sqlx::MySql>,
    schema: &Option<String>,
) -> Result<String> {
    target_schema(pool, schema)
        .await?
        .ok_or_else(|| ShkiError::introspection("No MySQL database is selected"))
}

fn constraints_from_rows(rows: Vec<MysqlConstraintRow>) -> Result<IndexMap<Iden, Vec<Constraint>>> {
    let mut constraints_by_table: IndexMap<Iden, IndexMap<String, Constraint>> = IndexMap::new();

    for row in rows {
        let constraint_map = constraints_by_table
            .entry(Iden::new(
                row.table_name.clone(),
                Some(row.table_schema.clone()),
            ))
            .or_default();

        let entry = constraint_map
            .entry(row.constraint_name.clone())
            .or_insert_with(|| match row.constraint_type.as_str() {
                "PRIMARY KEY" => Constraint::PrimaryKey(PrimaryKeyConstraint {
                    name: Some(row.constraint_name.clone()),
                    columns: Vec::new(),
                }),
                "UNIQUE" => Constraint::Unique(UniqueConstraint {
                    name: Some(row.constraint_name.clone()),
                    columns: Vec::new(),
                    nulls_distinct: true,
                }),
                "FOREIGN KEY" => Constraint::ForeignKey(ForeignKeyConstraint {
                    name: Some(row.constraint_name.clone()),
                    columns: Vec::new(),
                    references: Iden::new(
                        row.referenced_table_name.clone().unwrap_or_default(),
                        row.referenced_table_schema.clone(),
                    ),
                    references_columns: Vec::new(),
                    on_delete: parse_reference_action(row.delete_rule.as_deref()),
                    on_update: parse_reference_action(row.update_rule.as_deref()),
                    deferrable: false,
                    initially_deferred: false,
                }),
                _ => Constraint::Check(CheckConstraint {
                    name: Some(row.constraint_name.clone()),
                    expression: row.check_clause.clone().unwrap_or_default(),
                }),
            });

        match entry {
            Constraint::PrimaryKey(constraint) => {
                push_unique(&mut constraint.columns, row.column_name.as_ref())
            }
            Constraint::Unique(constraint) => {
                push_unique(&mut constraint.columns, row.column_name.as_ref())
            }
            Constraint::ForeignKey(constraint) => {
                push_unique(&mut constraint.columns, row.column_name.as_ref());
                push_unique(
                    &mut constraint.references_columns,
                    row.referenced_column_name.as_ref(),
                );
            }
            Constraint::Check(_) | Constraint::Exclusion(_) => {}
        }
    }

    Ok(constraints_by_table
        .into_iter()
        .map(|(table_id, constraints)| (table_id, constraints.into_values().collect()))
        .collect())
}

fn indexes_from_rows(rows: Vec<MysqlIndexRow>) -> Result<IndexMap<Iden, IndexMap<String, Index>>> {
    let mut indexes_by_table: IndexMap<Iden, IndexMap<String, Index>> = IndexMap::new();

    for row in rows {
        let index_map = indexes_by_table
            .entry(Iden::new(
                row.table_name.clone(),
                Some(row.table_schema.clone()),
            ))
            .or_default();
        let index = index_map
            .entry(row.index_name.clone())
            .or_insert_with(|| Index {
                name: row.index_name.clone(),
                columns: Vec::new(),
                unique: row.non_unique == 0,
                method: parse_index_method(&row.index_type),
                where_clause: None,
                options: Vec::new(),
                is_constraint: false,
                concurrently: false,
                include: Vec::new(),
                tablespace: None,
            });

        let column = if let Some(expression) = row.expression.as_ref() {
            IndexColumn::Expression {
                expression: expression.clone(),
                order: parse_sort_order(row.collation.as_deref()),
                nulls: None,
            }
        } else if let Some(column_name) = row.column_name.as_ref() {
            IndexColumn::Column {
                name: column_name.clone(),
                order: parse_sort_order(row.collation.as_deref()),
                nulls: None,
                opclass: None,
            }
        } else {
            return Err(ShkiError::introspection(format!(
                "Index '{}' is missing column or expression metadata",
                row.index_name
            )));
        };

        let position = row.seq_in_index.saturating_sub(1) as usize;
        if position >= index.columns.len() {
            index.columns.push(column);
        } else {
            index.columns.insert(position, column);
        }
    }

    Ok(indexes_by_table)
}
