use crate::Result;
use crate::diff::DiffStatement;
use crate::schema::SqlDialect;
use crate::sql::statements::*;

use super::render::{SqlOutput, ToSql};

impl ToSql for DiffStatement {
    fn to_sql(&self, dialect: &SqlDialect) -> Result<SqlOutput> {
        match self {
            DiffStatement::CreateSchema { name } => Ok(create_schema(dialect, name).into()),
            DiffStatement::DropSchema { name, cascade } => {
                Ok(drop_schema(dialect, name, *cascade).into())
            }
            DiffStatement::RenameSchema { from, to } => Ok(rename_schema(dialect, from, to).into()),
            DiffStatement::CreateEnum {
                name,
                schema,
                values,
                description,
            } => Ok(create_enum(dialect, name, schema, values, description)),
            DiffStatement::DropEnum { name, schema, .. } => {
                Ok(drop_enum(dialect, name, schema).into())
            }
            DiffStatement::RenameEnum { from, to, schema } => {
                Ok(rename_enum(dialect, from, to, schema).into())
            }
            DiffStatement::AddEnumValue {
                enum_name,
                schema,
                value,
                position,
            } => Ok(add_enum_value(dialect, enum_name, schema, value, position).into()),
            DiffStatement::RenameEnumValue {
                enum_name,
                schema,
                from,
                to,
            } => Ok(rename_enum_value(dialect, enum_name, schema, from, to).into()),
            DiffStatement::DropEnumValue {
                enum_name,
                schema,
                value,
            } => Ok(drop_enum_value(dialect, enum_name, schema, value)),
            DiffStatement::ReorderEnumValues {
                enum_name,
                schema,
                values,
                ..
            } => Ok(reorder_enum_values(dialect, enum_name, schema, values)),
            DiffStatement::RecreateEnum {
                name,
                schema,
                values,
                description,
                ..
            } => Ok(recreate_enum(dialect, name, schema, values, description)),
            DiffStatement::AlterEnumDescription {
                name,
                schema,
                description,
                ..
            } => Ok(alter_enum_description(dialect, name, schema, description).into()),
            DiffStatement::CreateSequence { sequence } => {
                Ok(create_sequence(dialect, sequence).into())
            }
            DiffStatement::DropSequence { name, schema, .. } => {
                Ok(drop_sequence(dialect, name, schema).into())
            }
            DiffStatement::AlterSequence {
                name,
                schema,
                changes,
            } => Ok(alter_sequence(dialect, name, schema, changes).into()),
            DiffStatement::CreateTable { table } => Ok(create_table(dialect, table)),
            DiffStatement::DropTable {
                name,
                schema,
                cascade,
                ..
            } => Ok(drop_table(dialect, name, schema, *cascade).into()),
            DiffStatement::RenameTable { from, to, schema } => {
                Ok(rename_table(dialect, from, to, schema).into())
            }
            DiffStatement::AlterTableComment {
                table,
                schema,
                comment,
                // don't need prev to set the comment -
                // only used to build down migration
                prev: _,
            } => Ok(alter_table_comment(dialect, table, schema, comment).into()),
            DiffStatement::AlterTableOptions {
                table,
                schema,
                changes,
            } => Ok(alter_table_options(dialect, table, schema, changes).into()),
            DiffStatement::AlterTableTablespace {
                table,
                schema,
                tablespace,
                ..
            } => Ok(alter_table_tablespace(dialect, table, schema, tablespace).into()),
            DiffStatement::AlterTablePartition {
                table,
                schema,
                partition,
                ..
            } => Ok(alter_table_partition(dialect, table, schema, partition).into()),
            DiffStatement::AddColumn {
                table,
                schema,
                column,
            } => Ok(add_column(dialect, table, schema, column).into()),
            DiffStatement::DropColumn {
                table,
                schema,
                column,
                cascade,
                ..
            } => Ok(drop_column(dialect, table, schema, column, *cascade).into()),
            DiffStatement::RenameColumn {
                table,
                schema,
                from,
                to,
            } => Ok(rename_column(dialect, table, schema, from, to).into()),
            DiffStatement::AlterColumn {
                table,
                schema,
                column,
                changes,
            } => Ok(alter_column(dialect, table, schema, column, changes)),
            DiffStatement::AlterColumnComment {
                table,
                schema,
                column,
                comment,
                ..
            } => Ok(alter_column_comment(dialect, table, schema, column, comment).into()),
            DiffStatement::CreateIndex {
                table,
                schema,
                index,
                concurrently,
                if_not_exists,
            } => Ok(
                create_index(dialect, table, schema, index, *concurrently, *if_not_exists).into(),
            ),
            DiffStatement::DropIndex {
                name,
                schema,
                concurrently,
                if_exists,
                ..
            } => Ok(drop_index(dialect, name, schema, *concurrently, *if_exists).into()),
            DiffStatement::RenameIndex {
                schema, from, to, ..
            } => Ok(rename_index(dialect, from, schema, to).into()),
            DiffStatement::AddConstraint {
                table,
                schema,
                constraint,
            } => Ok(add_constraint(dialect, table, schema, constraint).into()),
            DiffStatement::DropConstraint {
                table,
                schema,
                name,
                cascade,
                ..
            } => Ok(drop_constraint(dialect, table, schema, name, *cascade).into()),
            DiffStatement::RenameConstraint {
                table,
                schema,
                from,
                to,
            } => Ok(rename_constraint(dialect, table, schema, from, to).into()),
            DiffStatement::CreateView { view, or_replace } => {
                Ok(create_view(dialect, view, *or_replace).into())
            }
            DiffStatement::DropView {
                name,
                schema,
                materialized,
                cascade,
                ..
            } => Ok(drop_view(dialect, name, schema, *materialized, *cascade).into()),
            DiffStatement::AlterView {
                name,
                schema,
                new_definition,
                ..
            } => Ok(alter_view(dialect, name, schema, new_definition).into()),
            DiffStatement::CreateExtension(name) => Ok(create_extension(dialect, name).into()),
            DiffStatement::DropExtension(name) => Ok(drop_extension(dialect, name).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ColumnChange, EnumValuePosition, TableOptionChange};
    use crate::schema::{
        Column, Constraint, DataType, Index, IndexColumn, IndexMethod, PartitionMethod,
        PartitionSpec, PrimaryKeyConstraint, Sequence, SequenceOptions, UniqueConstraint, View,
    };

    #[test]
    fn renders_enum_and_table_diff_statements_to_sql() {
        let add_enum_value = DiffStatement::AddEnumValue {
            enum_name: "status".to_string(),
            schema: Some("public".to_string()),
            value: "archived".to_string(),
            position: EnumValuePosition::After("published".to_string()),
        };
        assert_eq!(
            add_enum_value
                .to_sql(&SqlDialect::Postgres)
                .expect("enum sql should render")
                .to_string(None),
            "ALTER TYPE \"public\".\"status\" ADD VALUE 'archived' AFTER 'published';"
        );

        let alter_comment = DiffStatement::AlterTableComment {
            table: "users".to_string(),
            schema: Some("app".to_string()),
            prev: None,
            comment: Some("owner's records".to_string()),
        };
        assert_eq!(
            alter_comment
                .to_sql(&SqlDialect::Postgres)
                .expect("table comment sql should render")
                .to_string(None),
            "COMMENT ON TABLE \"app\".\"users\" IS 'owner''s records';"
        );

        let alter_options = DiffStatement::AlterTableOptions {
            table: "users".to_string(),
            schema: Some("app".to_string()),
            changes: vec![
                TableOptionChange::Set {
                    key: "fillfactor".to_string(),
                    value: "80".to_string(),
                    prev: None,
                },
                TableOptionChange::Drop {
                    key: "autovacuum_enabled".to_string(),
                    prev: "true".to_string(),
                },
            ],
        };
        assert_eq!(
            alter_options
                .to_sql(&SqlDialect::Postgres)
                .expect("table options sql should render")
                .to_string(None),
            "ALTER TABLE \"app\".\"users\" SET (fillfactor=80, autovacuum_enabled=DEFAULT);"
        );

        let alter_partition = DiffStatement::AlterTablePartition {
            table: "events".to_string(),
            schema: Some("app".to_string()),
            prev_partition: None,
            partition: Some(PartitionSpec {
                method: PartitionMethod::Hash,
                columns: vec!["tenant_id".to_string(), "region_id".to_string()],
            }),
        };
        assert_eq!(
            alter_partition
                .to_sql(&SqlDialect::Postgres)
                .expect("partition sql should render")
                .to_string(None),
            "ALTER TABLE \"app\".\"events\" PARTITION BY HASH (tenant_id, region_id);"
        );
    }

    #[test]
    fn renders_column_and_index_diff_statements_to_sql() {
        let alter_column = DiffStatement::AlterColumn {
            table: "users".to_string(),
            schema: Some("app".to_string()),
            column: "id".to_string(),
            changes: vec![
                ColumnChange::SetNotNull,
                ColumnChange::SetIdentity(crate::schema::IdentitySpec {
                    always: false,
                    sequence_options: Some(SequenceOptions {
                        start: Some(10),
                        increment: Some(5),
                        ..Default::default()
                    }),
                }),
            ],
        };
        assert_eq!(
            alter_column
                .to_sql(&SqlDialect::Postgres)
                .expect("column sql should render")
                .to_string(None),
            "ALTER TABLE \"app\".\"users\" ALTER COLUMN \"id\" SET NOT NULL;\nALTER TABLE \"app\".\"users\" ALTER COLUMN \"id\" ADD GENERATED BY DEFAULT AS IDENTITY (START WITH 10 INCREMENT BY 5);"
        );

        let index = Index::with_columns(
            "users_email_idx",
            vec![
                IndexColumn::column("email")
                    .desc()
                    .nulls_last()
                    .opclass("text_pattern_ops"),
                IndexColumn::expression("lower(name)").asc(),
            ],
        )
        .unique()
        .using(IndexMethod::Gin)
        .where_clause("email IS NOT NULL")
        .include(vec!["id", "tenant_id"])
        .tablespace("fastspace")
        .option("fillfactor", "80");

        let create_index = DiffStatement::CreateIndex {
            table: "users".to_string(),
            schema: Some("app".to_string()),
            index,
            concurrently: true,
            if_not_exists: true,
        };
        assert_eq!(
            create_index
                .to_sql(&SqlDialect::Postgres)
                .expect("index sql should render")
                .to_string(None),
            "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS \"users_email_idx\" ON \"app\".\"users\" USING gin (\"email\" text_pattern_ops DESC NULLS LAST, lower(name) ASC) INCLUDE (\"id\", \"tenant_id\") WHERE email IS NOT NULL WITH (fillfactor=80) TABLESPACE \"fastspace\";"
        );

        let drop_index = DiffStatement::DropIndex {
            table: "users".to_string(),
            name: "users_email_idx".to_string(),
            schema: Some("app".to_string()),
            concurrently: true,
            if_exists: true,
            prev: Index::new("users_email_idx", vec!["email"]),
        };
        assert_eq!(
            drop_index
                .to_sql(&SqlDialect::Postgres)
                .expect("drop index sql should render")
                .to_string(None),
            "DROP INDEX CONCURRENTLY IF EXISTS \"app\".\"users_email_idx\";"
        );
    }

    #[test]
    fn renders_view_and_recreate_enum_diff_statements_to_sql() {
        let create_view = DiffStatement::CreateView {
            view: View {
                name: "active_users".to_string(),
                schema: Some("app".to_string()),
                definition: "SELECT id, email FROM users WHERE active".to_string(),
                materialized: true,
                columns: vec![],
            },
            or_replace: true,
        };
        assert_eq!(
            create_view
                .to_sql(&SqlDialect::Postgres)
                .expect("view sql should render")
                .to_string(None),
            "CREATE OR REPLACE MATERIALIZED VIEW \"app\".\"active_users\" AS SELECT id, email FROM users WHERE active;"
        );

        let alter_view = DiffStatement::AlterView {
            name: "active_users".to_string(),
            schema: Some("app".to_string()),
            new_definition: "SELECT id FROM users WHERE active".to_string(),
            prev_definition: "SELECT id FROM users".to_string(),
        };
        assert_eq!(
            alter_view
                .to_sql(&SqlDialect::Postgres)
                .expect("alter view sql should render")
                .to_string(None),
            "CREATE OR REPLACE VIEW \"app\".\"active_users\" AS SELECT id FROM users WHERE active;"
        );

        let recreate_enum = DiffStatement::RecreateEnum {
            name: "status".to_string(),
            schema: Some("public".to_string()),
            values: vec!["draft".to_string(), "published".to_string()],
            prev: crate::schema::DbEnum {
                name: "status".to_string(),
                schema: Some("public".to_string()),
                values: vec!["draft".to_string()],
                description: Some("old".to_string()),
            },
            description: Some("workflow states".to_string()),
        };
        let sql = recreate_enum
            .to_sql(&SqlDialect::Postgres)
            .expect("recreate enum sql should render")
            .to_string(None);

        assert!(sql.contains("ALTER TYPE \"public\".\"status\" RENAME TO \"status__old\""));
        assert!(sql.contains("CREATE TYPE \"public\".\"status\" AS ENUM ("));
        assert!(sql.contains("DROP TYPE \"public\".\"status__old\""));
        assert!(sql.contains("COMMENT ON TYPE \"public\".\"status\" IS 'workflow states';"));
    }

    #[test]
    fn renders_add_column_diff_statement_to_sql() {
        let add_column = DiffStatement::AddColumn {
            table: "users".to_string(),
            schema: Some("app".to_string()),
            column: Column::new("email", DataType::VarChar { length: Some(255) }).not_null(),
        };

        assert_eq!(
            add_column
                .to_sql(&SqlDialect::Postgres)
                .expect("add column sql should render")
                .to_string(None),
            "ALTER TABLE \"app\".\"users\" ADD COLUMN \"email\" VARCHAR(255) NOT NULL;"
        );
    }

    #[test]
    fn renders_sequence_diff_statements_to_sql() {
        let create_sequence = DiffStatement::CreateSequence {
            sequence: Sequence {
                name: "users_id_seq".to_string(),
                schema: Some("public".to_string()),
                increment: 1,
                min_value: 1,
                max_value: Some(2147483647),
                start: 1,
                cache: 1,
                cycle: false,
            },
        };

        assert_eq!(
            create_sequence
                .to_sql(&SqlDialect::Postgres)
                .expect("create sequence sql should render")
                .to_string(None),
            "CREATE SEQUENCE \"public\".\"users_id_seq\" INCREMENT BY 1 MINVALUE 1 MAXVALUE 2147483647 START WITH 1 CACHE 1 NO CYCLE;"
        );
    }

    #[test]
    fn create_table_diff_does_not_render_redundant_single_column_constraints() {
        let mut table = crate::schema::Table::in_schema("users", "public");
        table.column(Column::new("id", DataType::Serial).not_null());
        table.column(Column::new("email", DataType::Text).not_null());
        table.constraint(Constraint::PrimaryKey(
            PrimaryKeyConstraint::new(vec!["id"]).named("users_pkey"),
        ));
        table.constraint(Constraint::Unique(
            UniqueConstraint::new(vec!["email"]).named("users_email_key"),
        ));

        let sql = DiffStatement::CreateTable { table }
            .to_sql(&SqlDialect::Postgres)
            .expect("create table sql should render")
            .to_string(None);

        assert!(sql.contains("\"id\" SERIAL CONSTRAINT \"users_pkey\" PRIMARY KEY NOT NULL"));
        assert!(sql.contains("\"email\" TEXT NOT NULL CONSTRAINT \"users_email_key\" UNIQUE"));
        assert!(!sql.contains("CONSTRAINT \"users_pkey\" PRIMARY KEY (\"id\")"));
        assert!(!sql.contains("CONSTRAINT \"users_email_key\" UNIQUE (\"email\")"));
    }
}
