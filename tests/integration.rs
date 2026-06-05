mod engines;
use engines::*;
mod common;

use shki::codegen::OutputMode;
use shki::codegen::lang::typescript::TypescriptFlavor;
use shki::compiler::{ExternalShadowDBCompiler, SchemaCompiler};
use shki::config::Config;
use shki::migrate::journal::{Journal, MigrationKind};
use shki::models::iden::Iden;
use shki::run;
use shki::schema::{Column, DataType, DbEnum, Table};
use shki::snapshots::Snapshot;
use shki::{Cli, CodegenLanguage, Commands, CommonArgs, PullFormat};

use self::common::*;

async fn scenario_apply_simple<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let table_name = ctx.unique_name("users");
    let migration_path = ctx.write_migration(
        "0001_create_users.sql",
        &ctx.create_table_sql(&table_name, &[format!("name {} NOT NULL", ctx.text_type())]),
    );

    manager
        .apply_migration(&migration_path)
        .await
        .expect("failed to apply migration");

    assert!(ctx.table_exists(&table_name).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_users"]);

    ctx.cleanup().await;
}

async fn scenario_apply_all_and_pending_detection<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("users");
    let posts = ctx.unique_name("posts");
    let logs = ctx.unique_name("logs");

    ctx.write_migrations(&[
        (
            "0001_create_users.sql",
            ctx.create_table_sql(&users, &[format!("name {} NOT NULL", ctx.text_type())]),
        ),
        (
            "0002_create_posts.sql",
            ctx.create_table_sql(&posts, &[format!("title {} NOT NULL", ctx.text_type())]),
        ),
        (
            "0003_create_logs.sql",
            ctx.create_table_sql(&logs, &[format!("body {} NOT NULL", ctx.text_type())]),
        ),
    ]);

    assert_eq!(
        migration_names(
            manager
                .get_pending_migrations()
                .await
                .expect("failed to load pending migrations")
        ),
        vec![
            "0001_create_users".to_string(),
            "0002_create_posts".to_string(),
            "0003_create_logs".to_string(),
        ]
    );

    manager
        .apply_migration(&ctx.migrations_dir().join("0001_create_users.sql"))
        .await
        .expect("failed to apply first migration");

    assert_eq!(
        migration_names(
            manager
                .get_pending_migrations()
                .await
                .expect("failed to reload pending migrations")
        ),
        vec![
            "0002_create_posts".to_string(),
            "0003_create_logs".to_string()
        ]
    );

    let applied = manager
        .apply_all()
        .await
        .expect("failed to apply all pending migrations");

    assert_eq!(applied, vec!["0002_create_posts", "0003_create_logs"]);
    assert!(ctx.table_exists(&users).await);
    assert!(ctx.table_exists(&posts).await);
    assert!(ctx.table_exists(&logs).await);
    assert!(
        manager
            .get_pending_migrations()
            .await
            .expect("failed to read final pending migrations")
            .is_empty()
    );

    ctx.cleanup().await;
}

async fn scenario_rollback_single<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let table_name = ctx.unique_name("widgets");
    let up_path = ctx.write_migration(
        "0001_create_widgets.sql",
        &ctx.create_table_sql(&table_name, &[format!("name {} NOT NULL", ctx.text_type())]),
    );
    let down_path = ctx.write_migration(
        "0001_create_widgets.down.sql",
        &ctx.drop_table_sql(&table_name),
    );

    manager
        .apply_migration(&up_path)
        .await
        .expect("failed to apply migration");
    manager
        .rollback_migration(&down_path)
        .await
        .expect("failed to rollback migration");

    assert!(!ctx.table_exists(&table_name).await);
    assert!(ctx.applied_names(&manager).await.is_empty());

    ctx.cleanup().await;
}

async fn scenario_rollback_all<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("users");
    let posts = ctx.unique_name("posts");

    ctx.write_migrations(&[
        (
            "0001_create_users.sql",
            ctx.create_table_sql(&users, &[format!("name {} NOT NULL", ctx.text_type())]),
        ),
        ("0001_create_users.down.sql", ctx.drop_table_sql(&users)),
        (
            "0002_create_posts.sql",
            ctx.create_table_sql(&posts, &[format!("title {} NOT NULL", ctx.text_type())]),
        ),
        ("0002_create_posts.down.sql", ctx.drop_table_sql(&posts)),
    ]);

    manager
        .apply_all()
        .await
        .expect("failed to apply all migrations");

    let rolled_back = manager
        .rollback_all()
        .await
        .expect("failed to rollback all migrations");

    assert_eq!(rolled_back, vec!["0002_create_posts", "0001_create_users"]);
    assert!(!ctx.table_exists(&users).await);
    assert!(!ctx.table_exists(&posts).await);

    ctx.cleanup().await;
}

async fn scenario_rollback_count<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();

    for i in 1..=3 {
        let table = ctx.unique_name(&format!("tbl{i}"));
        ctx.write_migration(
            &format!("000{i}_create_tbl{i}.sql"),
            &ctx.create_table_sql(&table, &[]),
        );
        ctx.write_migration(
            &format!("000{i}_create_tbl{i}.down.sql"),
            &ctx.drop_table_sql(&table),
        );
    }

    manager
        .apply_all()
        .await
        .expect("failed to apply all migrations");

    let rolled_back = manager
        .rollback_count(2)
        .await
        .expect("failed to rollback migrations");

    assert_eq!(rolled_back, vec!["0003_create_tbl3", "0002_create_tbl2"]);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_tbl1"]);

    ctx.cleanup().await;
}

async fn scenario_transaction_rollback_on_error<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let table_name = ctx.unique_name("broken");
    let migration_path = ctx.write_migration(
        "0001_bad.sql",
        &format!(
            "{}\n{}",
            ctx.create_table_sql(&table_name, &[]),
            ctx.create_table_sql(&table_name, &[])
        ),
    );

    let result = manager.apply_migration(&migration_path).await;

    assert!(result.is_err());

    if ctx.dialect() == shki::schema::SqlDialect::Mysql {
        // MySQL autocommits DDL, so the failed second statement still leaves the first table behind.
        assert!(ctx.table_exists(&table_name).await);
    } else {
        assert!(!ctx.table_exists(&table_name).await);
    }

    assert!(ctx.applied_names(&manager).await.is_empty());

    ctx.cleanup().await;
}

async fn scenario_checksum_validation_blocks_new_migrations<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let first_table = ctx.unique_name("users");
    let second_table = ctx.unique_name("posts");
    let config_path = ctx.write_config();
    let first = ctx.write_migration(
        "0001_create_users.sql",
        &ctx.create_table_sql(
            &first_table,
            &[format!("name {} NOT NULL", ctx.text_type())],
        ),
    );

    manager
        .apply_migration(&first)
        .await
        .expect("failed to apply initial migration");

    std::fs::write(
        &first,
        ctx.create_table_sql(
            &first_table,
            &[
                format!("name {} NOT NULL", ctx.text_type()),
                format!("email {}", ctx.text_type()),
            ],
        ),
    )
    .expect("failed to modify migration file");
    ctx.write_migration(
        "0002_create_posts.sql",
        &ctx.create_table_sql(
            &second_table,
            &[format!("title {} NOT NULL", ctx.text_type())],
        ),
    );

    let error = run(ctx.migrate_cli(config_path))
        .await
        .expect_err("migrate should fail on checksum mismatch");

    assert!(error.to_string().contains("checksum mismatch"));
    assert!(!ctx.table_exists(&second_table).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_users"]);

    ctx.cleanup().await;
}

async fn scenario_custom_migration_table<T: TestBackend>(ctx: T) {
    let manager = ctx.manager_with_table("custom_migrations");

    manager
        .ensure_migrations_table()
        .await
        .expect("failed to ensure custom migrations table");

    assert!(ctx.migration_table_exists(&manager).await);

    ctx.cleanup().await;
}

async fn scenario_cli_migrate_applies_pending<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let table_name = ctx.unique_name("posts");
    ctx.write_migration(
        "0001_create_posts.sql",
        &ctx.create_table_sql(
            &table_name,
            &[format!("title {} NOT NULL", ctx.text_type())],
        ),
    );

    run(ctx.migrate_cli(ctx.write_config()))
        .await
        .expect("cli migrate should succeed");

    assert!(ctx.table_exists(&table_name).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_posts"]);

    ctx.cleanup().await;
}

async fn scenario_cli_create_records_custom_migration_in_journal<T: TestBackend>(ctx: T) {
    let config_path = ctx.write_config();

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(ctx.dialect()),
            database_url: Some(ctx.database_url()),
            ..CommonArgs::default()
        },
        command: Commands::Create {
            name: "Add audit table".to_string(),
            sql: Some(ctx.create_table_sql(&ctx.unique_name("audit"), &[])),
            sql_file: None,
            with_down: false,
            edit: false,
        },
    })
    .await
    .expect("cli create should succeed");

    let journal_path = ctx.migrations_dir().join("_meta/_journal.json");
    let journal_json = std::fs::read_to_string(&journal_path).expect("journal should be written");
    let journal: Journal = serde_json::from_str(&journal_json).expect("journal should parse");

    assert_eq!(journal.entries.len(), 1);
    assert_eq!(journal.entries[0].migration, "0000_add-audit-table");
    assert_eq!(journal.entries[0].kind, MigrationKind::Custom);
    assert!(journal.entries[0].snapshot_id.is_none());

    let migration_path = ctx.migrations_dir().join("0000_add-audit-table.sql");
    assert!(migration_path.exists());

    ctx.cleanup().await;
}

async fn scenario_cli_down_dry_run_does_not_modify_database<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("logs");
    let up_path = ctx.write_migration(
        "0001_create_logs.sql",
        &ctx.create_table_sql(&table_name, &[format!("body {} NOT NULL", ctx.text_type())]),
    );
    ctx.write_migration(
        "0001_create_logs.down.sql",
        &ctx.drop_table_sql(&table_name),
    );

    manager
        .apply_migration(&up_path)
        .await
        .expect("failed to apply migration");

    run(ctx.down_cli(config_path, Some(1), true))
        .await
        .expect("dry-run down should succeed");

    assert!(ctx.table_exists(&table_name).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_logs"]);

    ctx.cleanup().await;
}

async fn scenario_cli_pull_json_introspects_schema<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("introspected_users");
    let output_path = ctx.root_dir().join("snapshot.json");
    let migration_path = ctx.write_migration(
        "0001_create_introspected_users.sql",
        &ctx.create_table_sql(&table_name, &[format!("name {} NOT NULL", ctx.text_type())]),
    );

    manager
        .apply_migration(&migration_path)
        .await
        .expect("failed to apply migration before introspection");

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(ctx.dialect()),
            database_url: Some(ctx.database_url()),
            ..CommonArgs::default()
        },
        command: Commands::Pull {
            format: PullFormat::Json,
            output: Some(output_path.clone()),
            schema: ctx.migration_schema().map(str::to_string),
        },
    })
    .await
    .expect("pull json should introspect database");

    let snapshot_json = std::fs::read_to_string(&output_path).expect("snapshot should be written");
    let snapshot: Snapshot = serde_json::from_str(&snapshot_json).expect("snapshot should parse");
    assert_eq!(snapshot.dialect, ctx.dialect());

    let tables = snapshot.tables();
    let table_id = tables
        .keys()
        .find(|id| id.name == table_name)
        .cloned()
        .expect("created table should be introspected");
    let table = tables
        .get(&table_id)
        .expect("created table should be readable");

    assert!(table.columns.contains_key("id"));
    assert!(table.columns.contains_key("name"));
    assert!(!table.columns.get("name").expect("name column").nullable);
    assert!(table.constraints.iter().any(
        |constraint| matches!(constraint, shki::schema::Constraint::PrimaryKey(pk) if pk.columns == vec!["id"])
    ));

    if let Some(schema) = ctx.migration_schema() {
        assert_eq!(table_id, Iden::new(table_name, Some(schema.to_string())));
    }

    ctx.cleanup().await;
}

macro_rules! backend_suite {
    ($module:ident, $backend:ty) => {
        mod $module {
            use super::*;

            #[tokio::test]
            async fn apply_simple() {
                scenario_apply_simple(<$backend as TestBackend>::setup("apply_simple").await).await;
            }

            #[tokio::test]
            async fn apply_all_and_pending_detection() {
                scenario_apply_all_and_pending_detection(
                    <$backend as TestBackend>::setup("apply_all").await,
                )
                .await;
            }

            #[tokio::test]
            async fn rollback_single() {
                scenario_rollback_single(<$backend as TestBackend>::setup("rollback_single").await)
                    .await;
            }

            #[tokio::test]
            async fn rollback_all() {
                scenario_rollback_all(<$backend as TestBackend>::setup("rollback_all").await).await;
            }

            #[tokio::test]
            async fn rollback_count() {
                scenario_rollback_count(<$backend as TestBackend>::setup("rollback_count").await)
                    .await;
            }

            #[tokio::test]
            async fn transaction_rollback_on_error() {
                scenario_transaction_rollback_on_error(
                    <$backend as TestBackend>::setup("tx_rollback").await,
                )
                .await;
            }

            #[tokio::test]
            async fn checksum_validation_blocks_new_migrations() {
                scenario_checksum_validation_blocks_new_migrations(
                    <$backend as TestBackend>::setup("checksum").await,
                )
                .await;
            }

            #[tokio::test]
            async fn custom_migration_table() {
                scenario_custom_migration_table(
                    <$backend as TestBackend>::setup("migration_table").await,
                )
                .await;
            }

            #[tokio::test]
            async fn cli_migrate_applies_pending() {
                scenario_cli_migrate_applies_pending(
                    <$backend as TestBackend>::setup("cli_migrate").await,
                )
                .await;
            }

            #[tokio::test]
            async fn cli_create_records_custom_migration_in_journal() {
                scenario_cli_create_records_custom_migration_in_journal(
                    <$backend as TestBackend>::setup("cli_create_journal").await,
                )
                .await;
            }

            #[tokio::test]
            async fn cli_down_dry_run_does_not_modify_database() {
                scenario_cli_down_dry_run_does_not_modify_database(
                    <$backend as TestBackend>::setup("cli_down").await,
                )
                .await;
            }

            #[tokio::test]
            async fn cli_pull_json_introspects_schema() {
                scenario_cli_pull_json_introspects_schema(
                    <$backend as TestBackend>::setup("cli_pull_json").await,
                )
                .await;
            }
        }
    };
}

backend_suite!(sqlite, SqliteTestContext);
backend_suite!(postgres, PgTestContext);
backend_suite!(mysql, MysqlTestContext);

fn write_codegen_fixture_snapshot(root: &std::path::Path) -> std::path::PathBuf {
    let mut snapshot = Snapshot::new(shki::schema::SqlDialect::Postgres);
    let mut enums = indexmap::IndexMap::new();
    enums.insert(
        Iden::new("user_status", Some("public".to_string())),
        DbEnum::with_values("user_status", vec!["active", "inactive"]),
    );
    snapshot.set_enums(enums);

    let mut table = Table::in_schema("users", "public");
    table.column(Column::new("id", DataType::Integer).primary_key());
    table.column(Column::new("email", DataType::Text).not_null());
    table.column(Column::new(
        "status",
        DataType::Enum {
            name: "user_status".to_string(),
            schema: Some("public".to_string()),
        },
    ));
    snapshot.insert_table(Iden::new("users", Some("public".to_string())), table);

    let path = root.join("snapshot.json");
    std::fs::write(
        &path,
        snapshot.to_json().expect("snapshot should serialize"),
    )
    .expect("failed to write snapshot fixture");
    path
}

fn write_codegen_config(
    root: &std::path::Path,
    snapshot_path: &std::path::Path,
) -> std::path::PathBuf {
    let config_path = root.join("shki.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
root = "{}"
dialect = "postgres"
schema = "{}"
"#,
            root.display(),
            snapshot_path.display(),
        ),
    )
    .expect("failed to write codegen config");
    config_path
}

#[tokio::test]
async fn codegen_writes_typescript_module_from_snapshot() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let snapshot_path = write_codegen_fixture_snapshot(temp_dir.path());
    let config_path = write_codegen_config(temp_dir.path(), &snapshot_path);
    let output_dir = temp_dir.path().join("generated-ts");

    run(Cli {
        config: config_path,
        common: CommonArgs::default(),
        command: Commands::Codegen {
            language: CodegenLanguage::Typescript {
                flavor: TypescriptFlavor::Interface,
            },
            out: Some(output_dir.clone()),
            schema: None,
            mode: Some(OutputMode::SingleModule),
            verbose: false,
        },
    })
    .await
    .expect("typescript codegen should succeed");

    let user = std::fs::read_to_string(output_dir.join("user.ts"))
        .expect("user interface should be written");
    let status = std::fs::read_to_string(output_dir.join("user_status.ts"))
        .expect("status enum should be written");
    let index =
        std::fs::read_to_string(output_dir.join("index.ts")).expect("index should be written");

    assert!(user.contains("interface User"));
    assert!(user.contains("import { UserStatus } from './user_status';"));
    assert!(status.contains("enum UserStatus"));
    assert!(index.contains("export { User } from './user';"));
}

#[tokio::test]
async fn codegen_writes_rust_nested_modules_from_snapshot() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let snapshot_path = write_codegen_fixture_snapshot(temp_dir.path());
    let config_path = write_codegen_config(temp_dir.path(), &snapshot_path);
    let output_dir = temp_dir.path().join("generated-rs");

    run(Cli {
        config: config_path,
        common: CommonArgs::default(),
        command: Commands::Codegen {
            language: CodegenLanguage::Rust,
            out: Some(output_dir.clone()),
            schema: None,
            mode: Some(OutputMode::Modules),
            verbose: false,
        },
    })
    .await
    .expect("rust codegen should succeed");

    let user = std::fs::read_to_string(output_dir.join("user/user.rs"))
        .expect("user struct module should be written");
    let status = std::fs::read_to_string(output_dir.join("user_status/user_status.rs"))
        .expect("status enum module should be written");
    let root_mod =
        std::fs::read_to_string(output_dir.join("mod.rs")).expect("root module should be written");

    assert!(user.contains("pub struct User"));
    assert!(user.contains("use super::user_status::UserStatus;"));
    assert!(status.contains("pub enum UserStatus"));
    assert!(root_mod.contains("pub use user::User;"));
}

#[tokio::test]
async fn codegen_writes_protobuf_files_from_snapshot() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let snapshot_path = write_codegen_fixture_snapshot(temp_dir.path());
    let config_path = write_codegen_config(temp_dir.path(), &snapshot_path);
    let output_dir = temp_dir.path().join("generated-proto");

    run(Cli {
        config: config_path,
        common: CommonArgs::default(),
        command: Commands::Codegen {
            language: CodegenLanguage::Protobuf,
            out: Some(output_dir.clone()),
            schema: None,
            mode: Some(OutputMode::SingleModule),
            verbose: false,
        },
    })
    .await
    .expect("protobuf codegen should succeed");

    let user = std::fs::read_to_string(output_dir.join("user.proto"))
        .expect("user message should be written");
    let status = std::fs::read_to_string(output_dir.join("user_status.proto"))
        .expect("status enum should be written");

    assert!(user.contains("message User"));
    assert!(user.contains("import \"user_status.proto\";"));
    assert!(status.contains("enum UserStatus"));
}

#[tokio::test]
async fn compiler_turns_declarative_schema_sql_into_snapshot() {
    let ctx = PgTestContext::setup("compiler_single_file").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let table_name = ctx.unique_name("declared_users");
    let schema_path = ctx.root_dir().join("schema.sql");
    std::fs::write(
        &schema_path,
        format!("CREATE TABLE {table_name} (id integer primary key, name text not null);\n"),
    )
    .expect("failed to write declarative schema");

    let config = Config {
        root: ctx.root_dir().to_path_buf(),
        dialect: shki::schema::SqlDialect::Postgres,
        schema: schema_path,
        database_url: Some(ctx.database_url()),
        shadow_database_url: Some(shadow.database_url),
        ..Config::default()
    };

    let snapshot = ExternalShadowDBCompiler::from_config(&config)
        .expect("compiler should configure")
        .compile(&config)
        .await
        .expect("declarative schema should compile");

    let tables = snapshot.tables();
    let table = tables
        .iter()
        .find(|(id, _)| id.name == table_name)
        .map(|(_, table)| table)
        .expect("declared table should be in snapshot");

    assert!(table.columns.contains_key("id"));
    assert!(table.columns.contains_key("name"));
    assert!(!table.columns.get("name").expect("name column").nullable);

    ctx.cleanup().await;
}

#[tokio::test]
async fn compiler_consumes_directory_schema_with_i_includes() {
    let ctx = PgTestContext::setup("compiler_directory").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let table_name = ctx.unique_name("included_users");
    let schema_dir = ctx.root_dir().join("schema");
    let tables_dir = schema_dir.join("tables");
    std::fs::create_dir_all(&tables_dir).expect("failed to create schema directory");
    std::fs::write(schema_dir.join("main.sql"), "\\i tables/users.sql\n")
        .expect("failed to write main.sql");
    std::fs::write(
        tables_dir.join("users.sql"),
        format!("CREATE TABLE {table_name} (id integer primary key);\n"),
    )
    .expect("failed to write included table sql");

    let config = Config {
        root: ctx.root_dir().to_path_buf(),
        dialect: shki::schema::SqlDialect::Postgres,
        schema: schema_dir,
        database_url: Some(ctx.database_url()),
        shadow_database_url: Some(shadow.database_url),
        ..Config::default()
    };

    let snapshot = ExternalShadowDBCompiler::from_config(&config)
        .expect("compiler should configure")
        .compile(&config)
        .await
        .expect("directory schema should compile");

    assert!(snapshot.tables().keys().any(|id| id.name == table_name));

    ctx.cleanup().await;
}
