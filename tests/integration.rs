mod engines;
use engines::*;
mod common;

use shki::codegen::OutputMode;
use shki::codegen::lang::typescript::TypescriptFlavor;
use shki::compiler::{ExternalShadowDBCompiler, SchemaCompiler};
use shki::config::Config;
use shki::domain::migrate::manager::ApplyMigrationMode;
use shki::dump::SchemaExportFormat;
use shki::migrate::journal::{Journal, MigrationKind};
use shki::models::iden::Iden;
use shki::run;
use shki::schema::{Column, DataType, DbEnum, Table};
use shki::snapshots::{Introspectable, Snapshot};
use shki::{Cli, CodegenLanguage, Commands, CommonArgs};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Executor};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

use self::common::*;

async fn compile_extension_schema(image: &str, tag: &str, schema_sql: &str) -> Snapshot {
    let container = GenericImage::new(image, tag)
        .with_exposed_port(5432.tcp())
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_DB", "postgres")
        .start()
        .await
        .expect("failed to start PostgreSQL extension image");
    let host = container
        .get_host()
        .await
        .expect("failed to get PostgreSQL extension image host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("failed to get PostgreSQL extension image port");
    let database_url = format!("postgresql://postgres:postgres@{host}:{port}/postgres");
    let pool = connect_with_retries("PostgreSQL extension image", || {
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
    })
    .await;
    pool.execute("COMMENT ON DATABASE postgres IS 'shki:shadow'")
        .await
        .expect("failed to mark extension database as Shki-owned");

    let temp_dir = tempfile::tempdir().expect("failed to create extension schema fixture");
    std::fs::write(temp_dir.path().join("schema.sql"), schema_sql)
        .expect("failed to write extension schema fixture");
    let config = Config {
        root: temp_dir.path().to_path_buf(),
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        schema: "schema.sql".into(),
        shadow: shki::ShadowArgs {
            shadow_database_url: Some(database_url),
            ..Default::default()
        },
        ..Config::default()
    };

    ExternalShadowDBCompiler::from_config(&config)
        .expect("extension compiler should configure")
        .compile(&config)
        .await
        .expect("extension schema should compile")
}

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
        .apply(ApplyMigrationMode::All)
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
        .apply(ApplyMigrationMode::All)
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
        .apply(ApplyMigrationMode::All)
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

#[tokio::test]
async fn no_transaction_directive_allows_create_index_concurrently() {
    let ctx = PgTestContext::setup("no_tx_directive").await;
    let manager = ctx.manager();
    let schema = ctx.schema_name.clone();
    let table = ctx.unique_name("scan");

    manager
        .apply_migration(&ctx.write_migration(
            "0001_create_scan.sql",
            &ctx.create_table_sql(&table, &[format!("payload {} NOT NULL", ctx.text_type())]),
        ))
        .await
        .expect("failed to create table");

    let index_sql = |index: &str, column: &str| {
        format!(
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS \"{index}\" ON \"{schema}\".\"{table}\" ({column});"
        )
    };

    // Without the directive the wrapping transaction makes Postgres reject it,
    // and the error points at the escape hatch.
    let blocked = manager
        .apply_migration(
            &ctx.write_migration("0002_blocked.sql", &index_sql("idx_blocked", "payload")),
        )
        .await
        .expect_err("CONCURRENTLY inside a transaction should fail");
    let message = blocked.to_string();
    assert!(
        message.contains("cannot run inside a transaction block"),
        "{message}"
    );
    assert!(message.contains("shki:no-transaction"), "{message}");
    std::fs::remove_file(ctx.migrations_dir().join("0002_blocked.sql"))
        .expect("failed to remove blocked migration");

    // With it, each segment runs on its own connection outside any transaction.
    let path = ctx.write_migration(
        "0002_capture_profile.sql",
        &format!(
            "-- shki:no-transaction\n{}\n--> +statement\n{}\n",
            index_sql("idx_payload", "payload"),
            index_sql("idx_id", "id"),
        ),
    );

    manager
        .apply_migration(&path)
        .await
        .expect("no-transaction migration should apply");

    for index in ["idx_payload", "idx_id"] {
        let valid = sqlx::query_scalar::<_, bool>(
            "SELECT i.indisvalid FROM pg_index i
             JOIN pg_class c ON c.oid = i.indexrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2",
        )
        .bind(&schema)
        .bind(index)
        .fetch_one(&ctx.pg_pool)
        .await
        .unwrap_or_else(|e| panic!("index {index} should exist: {e}"));
        assert!(valid, "index {index} should be valid");
    }

    assert_eq!(
        ctx.applied_names(&manager).await,
        vec!["0001_create_scan", "0002_capture_profile"]
    );

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

async fn scenario_cli_migrate_steps_applies_limited_pending<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("users");
    let posts = ctx.unique_name("posts");
    let config_path = ctx.write_config();

    ctx.write_migrations(&[
        ("0001_create_users.sql", ctx.create_table_sql(&users, &[])),
        ("0002_create_posts.sql", ctx.create_table_sql(&posts, &[])),
    ]);

    run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Migrate {
            mode: Some(ApplyMigrationMode::Steps { num: 1 }),
            migrations: Default::default(),
            dry_run: false,
        },
    })
    .await
    .expect("cli migrate steps should succeed");

    assert!(ctx.table_exists(&users).await);
    assert!(!ctx.table_exists(&posts).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_users"]);

    ctx.cleanup().await;
}

async fn scenario_cli_migrate_to_applies_through_named_pending<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("users");
    let posts = ctx.unique_name("posts");
    let logs = ctx.unique_name("logs");
    let config_path = ctx.write_config();

    ctx.write_migrations(&[
        ("0001_create_users.sql", ctx.create_table_sql(&users, &[])),
        ("0002_create_posts.sql", ctx.create_table_sql(&posts, &[])),
        ("0003_create_logs.sql", ctx.create_table_sql(&logs, &[])),
    ]);

    run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Migrate {
            mode: Some(ApplyMigrationMode::To {
                name: "0002_create_posts".to_string(),
            }),
            migrations: Default::default(),
            dry_run: false,
        },
    })
    .await
    .expect("cli migrate to should succeed");

    assert!(ctx.table_exists(&users).await);
    assert!(ctx.table_exists(&posts).await);
    assert!(!ctx.table_exists(&logs).await);
    assert_eq!(
        ctx.applied_names(&manager).await,
        vec!["0001_create_users", "0002_create_posts"]
    );

    ctx.cleanup().await;
}

async fn scenario_cli_migrate_to_unknown_fails_without_applying<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("users");
    let config_path = ctx.write_config();

    ctx.write_migration("0001_create_users.sql", &ctx.create_table_sql(&users, &[]));

    let error = run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Migrate {
            mode: Some(ApplyMigrationMode::To {
                name: "9999_missing".to_string(),
            }),
            migrations: Default::default(),
            dry_run: false,
        },
    })
    .await
    .expect_err("unknown migration target should fail");

    assert!(error.to_string().contains("9999_missing"));
    assert!(!ctx.table_exists(&users).await);
    assert!(ctx.applied_names(&manager).await.is_empty());

    ctx.cleanup().await;
}

async fn scenario_cli_migrate_dry_run_does_not_modify_database<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("users");
    let config_path = ctx.write_config();

    ctx.write_migration("0001_create_users.sql", &ctx.create_table_sql(&users, &[]));

    run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Migrate {
            mode: None,
            migrations: Default::default(),
            dry_run: true,
        },
    })
    .await
    .expect("migrate dry-run should succeed");

    assert!(!ctx.table_exists(&users).await);
    assert!(!ctx.migration_table_exists(&manager).await);

    ctx.cleanup().await;
}

async fn scenario_cli_status_does_not_create_migration_table<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let users = ctx.unique_name("status_users");
    let config_path = ctx.write_config();

    ctx.write_migration(
        "0001_create_status_users.sql",
        &ctx.create_table_sql(&users, &[]),
    );

    run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Status {
            migrations: Default::default(),
        },
    })
    .await
    .expect("status should report local migrations without mutating database");

    assert!(!ctx.table_exists(&users).await);
    assert!(!ctx.migration_table_exists(&manager).await);

    ctx.cleanup().await;
}

async fn scenario_cli_create_records_custom_migration_in_journal<T: TestBackend>(ctx: T) {
    let config_path = ctx.write_config();

    run(shki::Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Create {
            migrations: Default::default(),
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

    let migration_path = ctx.migrations_dir().join("0000_add-audit-table.sql");
    assert!(migration_path.exists());

    ctx.cleanup().await;
}

async fn scenario_cli_drop_removes_pending_custom_migration<T: TestBackend>(ctx: T) {
    let config_path = ctx.write_config();

    run(shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Create {
            migrations: Default::default(),
            name: "Add audit table".to_string(),
            sql: Some(ctx.create_table_sql(&ctx.unique_name("audit"), &[])),
            sql_file: None,
            with_down: true,
            edit: false,
        },
    })
    .await
    .expect("cli create should succeed");

    let up_path = ctx.migrations_dir().join("0000_add-audit-table.sql");
    let down_path = ctx.migrations_dir().join("0000_add-audit-table.down.sql");
    let journal_path = ctx.migrations_dir().join("_meta/_journal.json");
    assert!(up_path.exists());
    assert!(down_path.exists());

    run(shki::Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Drop {
            migration: Some("add-audit-table".to_string()),
            force: false,
        },
    })
    .await
    .expect("cli drop should remove pending migration");

    assert!(!up_path.exists());
    assert!(!down_path.exists());
    let journal_json = std::fs::read_to_string(&journal_path).expect("journal should remain");
    let journal: Journal = serde_json::from_str(&journal_json).expect("journal should parse");
    assert!(journal.entries.is_empty());

    ctx.cleanup().await;
}

async fn scenario_cli_drop_refuses_applied_migration<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("applied_users");
    let up_path = ctx.write_migration(
        "0001_create_applied_users.sql",
        &ctx.create_table_sql(&table_name, &[]),
    );
    let down_path = ctx.write_migration(
        "0001_create_applied_users.down.sql",
        &ctx.drop_table_sql(&table_name),
    );

    manager
        .apply_migration(&up_path)
        .await
        .expect("failed to apply migration before drop");

    let error = run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Drop {
            migration: Some("create_applied_users".to_string()),
            force: false,
        },
    })
    .await
    .expect_err("drop should refuse applied migrations");

    assert!(error.to_string().contains("applied"));
    assert!(up_path.exists());
    assert!(down_path.exists());
    assert_eq!(
        ctx.applied_names(&manager).await,
        vec!["0001_create_applied_users"]
    );

    ctx.cleanup().await;
}

async fn scenario_cli_drop_force_skips_applied_check<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("forced_users");
    let up_path = ctx.write_migration(
        "0001_create_forced_users.sql",
        &ctx.create_table_sql(&table_name, &[]),
    );
    let down_path = ctx.write_migration(
        "0001_create_forced_users.down.sql",
        &ctx.drop_table_sql(&table_name),
    );

    manager
        .apply_migration(&up_path)
        .await
        .expect("failed to apply migration before drop");

    run(Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Drop {
            migration: Some("create_forced_users".to_string()),
            force: true,
        },
    })
    .await
    .expect("drop --force should remove applied migration without db validation");

    assert!(!up_path.exists());
    assert!(!down_path.exists());

    ctx.cleanup().await;
}

#[tokio::test]
async fn cli_diff_compiles_declarative_schema_and_previews_changes() {
    let ctx = PgTestContext::setup("cli_diff").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("diff_users");
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table_name} (id integer primary key, name text not null);\n"),
    )
    .expect("failed to write declarative schema");

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Diff {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
        },
    })
    .await
    .expect("diff should compile declarative schema and preview changes");

    let migration_files = std::fs::read_dir(ctx.migrations_dir())
        .expect("migrations dir should be readable")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("sql"))
        .count();
    assert_eq!(migration_files, 0, "diff preview must not write migrations");

    shadow.cleanup().await;
    ctx.cleanup().await;
}

#[tokio::test]
async fn cli_generate_writes_schema_migration_snapshot_and_journal_entry() {
    let ctx = PgTestContext::setup("cli_generate").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("generated_users");
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table_name} (id integer primary key, name text not null);\n"),
    )
    .expect("failed to write declarative schema");

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Generate {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            migrations: Default::default(),
            name: "create generated users".to_string(),
            custom: false,
            with_down: true,
        },
    })
    .await
    .expect("generate should write migration artifacts");

    let up_path = ctx.migrations_dir().join("0000_create-generated-users.sql");
    let down_path = ctx
        .migrations_dir()
        .join("0000_create-generated-users.down.sql");
    let snapshot_path = ctx
        .migrations_dir()
        .join("_meta/0000_create-generated-users.snapshot.json");
    let journal_path = ctx.migrations_dir().join("_meta/_journal.json");

    let up_sql = std::fs::read_to_string(&up_path).expect("up migration should exist");
    assert!(up_sql.contains("-- Type: schema"));
    assert!(up_sql.contains("CREATE TABLE"));
    assert!(up_sql.contains(&table_name));
    assert!(
        down_path.exists(),
        "requested down migration should be written"
    );

    let snapshot_json = std::fs::read_to_string(&snapshot_path).expect("snapshot should exist");
    let snapshot: Snapshot = serde_json::from_str(&snapshot_json).expect("snapshot should parse");
    assert!(snapshot.tables().keys().any(|id| id.name == table_name));

    let journal_json = std::fs::read_to_string(&journal_path).expect("journal should exist");
    let journal: Journal = serde_json::from_str(&journal_json).expect("journal should parse");
    assert_eq!(journal.entries.len(), 1);
    let entry = &journal.entries[0];
    assert_eq!(entry.migration, "0000_create-generated-users");
    assert_eq!(entry.kind, MigrationKind::Schema);
    assert!(!entry.checksum.is_empty());

    shadow.cleanup().await;
    ctx.cleanup().await;
}

#[tokio::test]
async fn cli_generate_custom_then_schema_migration_keeps_journal_order() {
    let ctx = PgTestContext::setup("cli_generate_after_custom").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("generated_after_custom");

    run(shki::Cli {
        config: config_path.clone(),
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Generate {
            shadow: Default::default(),
            migrations: Default::default(),
            name: "custom audit step".to_string(),
            custom: true,
            with_down: false,
        },
    })
    .await
    .expect("custom generate should write Custom Migration");

    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table_name} (id integer primary key, name text not null);\n"),
    )
    .expect("failed to write declarative schema");

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Generate {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            migrations: Default::default(),
            name: "create declared table".to_string(),
            custom: false,
            with_down: false,
        },
    })
    .await
    .expect("schema generate should succeed after Custom Migration");

    let journal_json = std::fs::read_to_string(ctx.migrations_dir().join("_meta/_journal.json"))
        .expect("journal should exist");
    let journal: Journal = serde_json::from_str(&journal_json).expect("journal should parse");

    assert_eq!(journal.entries.len(), 2);
    assert_eq!(journal.entries[0].migration, "0000_custom-audit-step");
    assert_eq!(journal.entries[0].kind, MigrationKind::Custom);
    assert_eq!(journal.entries[1].migration, "0001_create-declared-table");
    assert_eq!(journal.entries[1].kind, MigrationKind::Schema);
    assert!(
        ctx.migrations_dir()
            .join("_meta/0001_create-declared-table.snapshot.json")
            .exists()
    );

    shadow.cleanup().await;
    ctx.cleanup().await;
}

/// A Custom Migration that changes the schema shape must be reflected in the
/// next diff. shki backfills a Snapshot for it (replaying its SQL on a shadow
/// DB) so the baseline includes the custom change — otherwise the next
/// `generate` would re-emit DDL the custom migration already applied.
#[tokio::test]
async fn cli_generate_accounts_for_schema_changing_custom_migration() {
    let ctx = PgTestContext::setup("cli_generate_custom_ddl").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let table = ctx.unique_name("custom_ddl_users");

    // 1. A schema migration creating the table (id, name).
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table} (id integer primary key, name text not null);\n"),
    )
    .expect("write declarative schema");
    let shadow_args = || shki::ShadowArgs {
        shadow_database_url: Some(shadow.database_url.clone()),
        ..Default::default()
    };
    let common = || CommonArgs {
        dialect: Some(shki::schema::SqlDialect::Postgres),
        ..CommonArgs::default()
    };
    run(shki::Cli {
        config: config_path.clone(),
        common: common(),
        command: Commands::Generate {
            shadow: shadow_args(),
            migrations: Default::default(),
            name: "create users".to_string(),
            custom: false,
            with_down: false,
        },
    })
    .await
    .expect("schema generate should succeed");

    // 2. A Custom Migration that adds a column via hand-written DDL.
    run(shki::Cli {
        config: config_path.clone(),
        common: common(),
        command: Commands::Create {
            migrations: Default::default(),
            name: "add email".to_string(),
            sql: Some(format!("ALTER TABLE {table} ADD COLUMN email text;")),
            sql_file: None,
            with_down: false,
            edit: false,
        },
    })
    .await
    .expect("custom create should write a Custom Migration");

    // 3. Mirror the custom change in the declarative schema (the source of truth).
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table} (id integer primary key, name text not null, email text);\n"),
    )
    .expect("write updated declarative schema");

    // 4. Generate again. Because the custom migration's `email` column is now in
    //    the baseline, the declarative schema matches it and there are NO changes
    //    — so no third migration is written.
    run(shki::Cli {
        config: config_path,
        common: common(),
        command: Commands::Generate {
            shadow: shadow_args(),
            migrations: Default::default(),
            name: "should be empty".to_string(),
            custom: false,
            with_down: false,
        },
    })
    .await
    .expect("second schema generate should succeed");

    // The custom migration was backfilled with a Snapshot capturing `email`.
    let custom_snapshot_path = ctx
        .migrations_dir()
        .join("_meta/0001_add-email.snapshot.json");
    let snapshot: Snapshot = serde_json::from_str(
        &std::fs::read_to_string(&custom_snapshot_path)
            .expect("custom migration should have a backfilled snapshot"),
    )
    .expect("snapshot should parse");
    let users = snapshot
        .tables()
        .into_iter()
        .find(|(id, _)| id.name == table)
        .map(|(_, t)| t)
        .expect("snapshot should contain the table");
    assert!(
        users.columns.contains_key("email"),
        "backfilled custom-migration snapshot must include the column the custom DDL added"
    );

    // No spurious migration was generated for a change the custom migration made.
    let sql_files: Vec<String> = std::fs::read_dir(ctx.migrations_dir())
        .expect("migrations dir readable")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".sql"))
        .collect();
    assert_eq!(
        sql_files.len(),
        2,
        "only the schema + custom migration files should exist, got: {sql_files:?}"
    );

    shadow.cleanup().await;
    ctx.cleanup().await;
}

#[tokio::test]
async fn cli_migrate_steps_applies_limited_pending() {
    scenario_cli_migrate_steps_applies_limited_pending(
        SqliteTestContext::setup("cli_migrate_steps").await,
    )
    .await;
}

#[tokio::test]
async fn cli_migrate_to_applies_through_named_pending() {
    scenario_cli_migrate_to_applies_through_named_pending(
        SqliteTestContext::setup("cli_migrate_to").await,
    )
    .await;
}

#[tokio::test]
async fn cli_migrate_to_unknown_fails_without_applying() {
    scenario_cli_migrate_to_unknown_fails_without_applying(
        SqliteTestContext::setup("cli_migrate_missing").await,
    )
    .await;
}

#[tokio::test]
async fn cli_migrate_dry_run_does_not_modify_database() {
    scenario_cli_migrate_dry_run_does_not_modify_database(
        SqliteTestContext::setup("cli_migrate_dry").await,
    )
    .await;
}

#[tokio::test]
async fn cli_status_does_not_create_migration_table() {
    scenario_cli_status_does_not_create_migration_table(
        SqliteTestContext::setup("cli_status_readonly").await,
    )
    .await;
}

#[tokio::test]
async fn cli_drop_refuses_applied_migration() {
    scenario_cli_drop_refuses_applied_migration(SqliteTestContext::setup("cli_drop_applied").await)
        .await;
}

#[tokio::test]
async fn cli_drop_force_skips_applied_check() {
    scenario_cli_drop_force_skips_applied_check(SqliteTestContext::setup("cli_drop_force").await)
        .await;
}

#[tokio::test]
async fn cli_generate_validates_generated_sql_in_shadow_database() {
    let ctx = PgTestContext::setup("cli_generate_validates_sql").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let enum_name = ctx.unique_name("status");
    let table_name = ctx.unique_name("status_events");
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!(
            r#"CREATE TYPE "{enum_name}" AS ENUM ('active');
"#,
            enum_name = enum_name,
        ),
    )
    .expect("failed to write initial declarative schema");

    run(shki::Cli {
        config: config_path.clone(),
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Generate {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            migrations: Default::default(),
            name: "create status".to_string(),
            custom: false,
            with_down: false,
        },
    })
    .await
    .expect("initial generate should succeed");

    std::fs::write(
        ctx.root_dir().join("schema"),
        format!(
            r#"CREATE TYPE "{enum_name}" AS ENUM ('active', 'archived');
CREATE TABLE "{table_name}" (
    id integer primary key,
    status "{enum_name}" not null default 'archived'
);
"#,
            enum_name = enum_name,
            table_name = table_name,
        ),
    )
    .expect("failed to write changed declarative schema");

    let error = run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Generate {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            migrations: Default::default(),
            name: "use archived status".to_string(),
            custom: false,
            with_down: false,
        },
    })
    .await
    .expect_err("generate should fail when generated SQL fails validation");

    let message = error.to_string();
    assert!(message.contains("Generated migration SQL failed validation"));
    assert!(message.contains("source failed validation"));
    assert!(
        !ctx.migrations_dir()
            .join("0001_use-archived-status.sql")
            .exists(),
        "failed validation must not write the migration"
    );

    let journal_json = std::fs::read_to_string(ctx.migrations_dir().join("_meta/_journal.json"))
        .expect("journal should exist");
    let journal: Journal = serde_json::from_str(&journal_json).expect("journal should parse");
    assert_eq!(journal.entries.len(), 1);

    shadow.cleanup().await;
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

async fn scenario_cli_dump_json_introspects_schema<T: TestBackend>(ctx: T) {
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
        common: ctx.common_args(),
        command: Commands::Dump {
            format: SchemaExportFormat::Json,
            output: Some(output_path.clone()),
            dirs: false,
            force: false,
            schema: ctx.migration_schema().map(str::to_string),
        },
    })
    .await
    .expect("dump json should introspect database");

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

async fn scenario_cli_dump_sql_writes_declarative_schema<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("dumped_users");
    let output_path = ctx.root_dir().join("schema.sql");
    let migration_path = ctx.write_migration(
        "0001_create_dumped_users.sql",
        &ctx.create_table_sql(&table_name, &[format!("name {} NOT NULL", ctx.text_type())]),
    );

    manager
        .apply_migration(&migration_path)
        .await
        .expect("failed to apply migration before dump");

    run(shki::Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Dump {
            format: SchemaExportFormat::Sql,
            output: Some(output_path.clone()),
            dirs: false,
            force: false,
            schema: ctx.migration_schema().map(str::to_string),
        },
    })
    .await
    .expect("dump sql should introspect and write schema SQL");

    let schema_sql = std::fs::read_to_string(&output_path).expect("schema SQL should be written");
    assert!(schema_sql.contains("CREATE TABLE"));
    assert!(schema_sql.contains(&table_name));
    assert!(schema_sql.contains("name"));
    assert!(schema_sql.contains("NOT NULL"));

    ctx.cleanup().await;
}

async fn scenario_cli_dump_dirs_writes_directory_schema<T: TestBackend>(ctx: T) {
    let manager = ctx.manager();
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("directory_users");
    let output_dir = ctx.root_dir().join("schema-dir");
    let migration_path = ctx.write_migration(
        "0001_create_directory_users.sql",
        &ctx.create_table_sql(&table_name, &[format!("name {} NOT NULL", ctx.text_type())]),
    );

    manager
        .apply_migration(&migration_path)
        .await
        .expect("failed to apply migration before directory dump");

    run(shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Dump {
            format: SchemaExportFormat::Sql,
            output: Some(output_dir.clone()),
            dirs: true,
            force: false,
            schema: ctx.migration_schema().map(str::to_string),
        },
    })
    .await
    .expect("dump dirs should write Directory Schema");

    let main_sql = std::fs::read_to_string(output_dir.join("main.sql"))
        .expect("Directory Schema main.sql should be written");
    let table_include = main_sql
        .lines()
        .find_map(|line| {
            line.strip_prefix("\\i ")
                .filter(|path| path.ends_with(&format!("tables/{table_name}.sql")))
        })
        .expect("main.sql should include the dumped table file");
    let table_path = output_dir.join(table_include);
    let table_sql =
        std::fs::read_to_string(&table_path).expect("Directory Schema table SQL should be written");

    assert!(table_sql.contains("CREATE TABLE"));
    assert!(table_sql.contains(&table_name));

    let collision = run(shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Dump {
            format: SchemaExportFormat::Sql,
            output: Some(output_dir.clone()),
            dirs: true,
            force: false,
            schema: ctx.migration_schema().map(str::to_string),
        },
    })
    .await;
    assert!(collision.is_err());

    run(shki::Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Dump {
            format: SchemaExportFormat::Sql,
            output: Some(output_dir),
            dirs: true,
            force: true,
            schema: ctx.migration_schema().map(str::to_string),
        },
    })
    .await
    .expect("dump dirs --force should overwrite generated files");

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_dump_dirs_writes_catalog_object_layout() {
    let ctx = PgTestContext::setup("dump_dirs_catalog_layout").await;
    let config_path = ctx.write_config();
    let enum_name = ctx.unique_name("user_status");
    let table_name = ctx.unique_name("users");
    let index_name = ctx.unique_name("users_name_idx");
    let view_name = ctx.unique_name("active_users");
    let materialized_view_name = ctx.unique_name("user_stats");
    let function_name = ctx.unique_name("normalize_name");
    let trigger_function_name = ctx.unique_name("touch_user");
    let trigger_name = ctx.unique_name("users_touch");
    let output_dir = ctx.root_dir().join("catalog-schema-dir");

    ctx.pg_pool
        .execute("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        .await
        .expect("failed to create extension fixture");
    let fixture_sql = format!(
        r#"
        CREATE TYPE "{schema}"."{enum_name}" AS ENUM ('active', 'inactive');
        CREATE TABLE "{schema}"."{table_name}" (
            id integer primary key,
            name text not null,
            status "{schema}"."{enum_name}" not null,
            CONSTRAINT "{table_name}_name_unique" UNIQUE (name)
        );
        CREATE INDEX "{index_name}" ON "{schema}"."{table_name}" (name);
        CREATE VIEW "{schema}"."{view_name}" AS
            SELECT id, name FROM "{schema}"."{table_name}" WHERE status = 'active';
        CREATE MATERIALIZED VIEW "{schema}"."{materialized_view_name}" AS
            SELECT status, count(*) AS count FROM "{schema}"."{table_name}" GROUP BY status;
        CREATE FUNCTION "{schema}"."{function_name}"(value text)
        RETURNS text
        LANGUAGE sql
        AS $$ SELECT lower(value) $$;
        CREATE FUNCTION "{schema}"."{trigger_function_name}"()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$ BEGIN RETURN NEW; END; $$;
        CREATE TRIGGER "{trigger_name}"
        BEFORE INSERT ON "{schema}"."{table_name}"
        FOR EACH ROW
        EXECUTE FUNCTION "{schema}"."{trigger_function_name}"();
        "#,
        schema = ctx.schema_name,
        enum_name = enum_name,
        table_name = table_name,
        index_name = index_name,
        view_name = view_name,
        materialized_view_name = materialized_view_name,
        function_name = function_name,
        trigger_function_name = trigger_function_name,
        trigger_name = trigger_name,
    );
    sqlx::raw_sql(AssertSqlSafe(fixture_sql))
        .execute(&ctx.pg_pool)
        .await
        .expect("failed to create catalog dump fixture");

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            database_url: Some(ctx.database_url()),
            ..CommonArgs::default()
        },
        command: Commands::Dump {
            format: SchemaExportFormat::Sql,
            output: Some(output_dir.clone()),
            dirs: true,
            force: false,
            schema: Some(ctx.schema_name.clone()),
        },
    })
    .await
    .expect("postgres dump dirs should write catalog object layout");

    let main_sql =
        std::fs::read_to_string(output_dir.join("main.sql")).expect("main.sql should be written");
    let schema_root = output_dir.join(&ctx.schema_name);

    assert!(main_sql.contains("\\i extensions/pgcrypto.sql"));
    assert!(main_sql.contains(&format!("\\i {}/types/{}.sql", ctx.schema_name, enum_name)));
    assert!(main_sql.contains(&format!(
        "\\i {}/tables/{}.sql",
        ctx.schema_name, table_name
    )));
    assert!(!main_sql.contains(&format!(
        "\\i {}/indexes/{}.sql",
        ctx.schema_name, index_name
    )));
    assert!(main_sql.contains(&format!("\\i {}/views/{}.sql", ctx.schema_name, view_name)));
    assert!(main_sql.contains(&format!(
        "\\i {}/materialized_views/{}.sql",
        ctx.schema_name, materialized_view_name
    )));
    let function_include = main_sql
        .lines()
        .find_map(|line| {
            line.strip_prefix("\\i ").filter(|path| {
                path.starts_with(&format!("{}/functions/{}", ctx.schema_name, function_name))
                    && path.ends_with(".sql")
            })
        })
        .expect("main.sql should include the dumped function file");
    assert!(!main_sql.contains(&format!(
        "\\i {}/triggers/{}.sql",
        ctx.schema_name, trigger_name
    )));

    assert!(
        std::fs::read_to_string(output_dir.join("extensions/pgcrypto.sql"))
            .expect("extension file should be written")
            .contains("CREATE EXTENSION")
    );
    assert!(
        std::fs::read_to_string(schema_root.join("types").join(format!("{enum_name}.sql")))
            .expect("enum file should be written")
            .contains("CREATE TYPE")
    );
    let table_sql =
        std::fs::read_to_string(schema_root.join("tables").join(format!("{table_name}.sql")))
            .expect("table file should be written");
    assert!(table_sql.contains("CREATE TABLE"));
    assert!(table_sql.contains(&format!("CONSTRAINT \"{table_name}_name_unique\" UNIQUE")));
    assert!(table_sql.contains("CREATE INDEX"));
    assert!(table_sql.contains(&format!("CREATE TRIGGER \"{trigger_name}\"")));
    assert!(
        !schema_root
            .join("indexes")
            .join(format!("{index_name}.sql"))
            .exists()
    );
    assert!(
        std::fs::read_to_string(schema_root.join("views").join(format!("{view_name}.sql")))
            .expect("view file should be written")
            .contains("CREATE VIEW")
    );
    assert!(
        std::fs::read_to_string(
            schema_root
                .join("materialized_views")
                .join(format!("{materialized_view_name}.sql"))
        )
        .expect("materialized view file should be written")
        .contains("CREATE MATERIALIZED VIEW")
    );
    assert!(
        std::fs::read_to_string(output_dir.join(function_include))
            .expect("function file should be written")
            .contains("CREATE FUNCTION")
    );
    assert!(
        !schema_root
            .join("triggers")
            .join(format!("{trigger_name}.sql"))
            .exists()
    );

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
            async fn cli_drop_removes_pending_custom_migration() {
                scenario_cli_drop_removes_pending_custom_migration(
                    <$backend as TestBackend>::setup("cli_drop_pending").await,
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
            async fn cli_dump_json_introspects_schema() {
                scenario_cli_dump_json_introspects_schema(
                    <$backend as TestBackend>::setup("cli_dump_json").await,
                )
                .await;
            }

            #[tokio::test]
            async fn cli_dump_sql_writes_declarative_schema() {
                scenario_cli_dump_sql_writes_declarative_schema(
                    <$backend as TestBackend>::setup("cli_dump_sql").await,
                )
                .await;
            }

            #[tokio::test]
            async fn cli_dump_dirs_writes_directory_schema() {
                scenario_cli_dump_dirs_writes_directory_schema(
                    <$backend as TestBackend>::setup("cli_dump_dirs").await,
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
            shadow: Default::default(),
            codegen: shki::CodegenArgs {
                config: shki::codegen::CodegenConfig {
                    output: Some(output_dir.clone()),
                    format: Some(OutputMode::Module),
                    ..Default::default()
                },
                ..Default::default()
            },
            language: CodegenLanguage::Typescript {
                flavor: TypescriptFlavor::Interface,
            },
            source: Some(snapshot_path),
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
            shadow: Default::default(),
            codegen: shki::CodegenArgs {
                config: shki::codegen::CodegenConfig {
                    output: Some(output_dir.clone()),
                    format: Some(OutputMode::Modules),
                    ..Default::default()
                },
                ..Default::default()
            },
            language: CodegenLanguage::Rust,
            source: Some(snapshot_path),
        },
    })
    .await
    .expect("rust codegen should succeed");

    let user_def = std::fs::read_to_string(output_dir.join("user/_def.rs"))
        .expect("user struct definition should be written");
    let user_mod = std::fs::read_to_string(output_dir.join("user/mod.rs"))
        .expect("user module index should be written");
    let status_def = std::fs::read_to_string(output_dir.join("user_status/_def.rs"))
        .expect("status enum definition should be written");
    let root_mod =
        std::fs::read_to_string(output_dir.join("mod.rs")).expect("root module should be written");

    assert!(user_def.contains("pub struct User"));
    // The directory name is not mirrored as a file name (avoids clippy's
    // `module_inception`); the generated definition lives in `user/_def.rs`.
    assert!(!output_dir.join("user/user.rs").exists());
    // The thin, user-editable mod.rs mounts the generated _def.rs.
    assert!(user_mod.contains("mod _def;"));
    assert!(user_mod.contains("pub use _def::*;"));
    // The definition lives two levels deep, so a sibling type is reached via the
    // top-level re-export: `super::super::<Type>`.
    assert!(user_def.contains("use super::super::UserStatus;"));
    assert!(!user_def.contains("use super::user_status::UserStatus;"));
    assert!(status_def.contains("pub enum UserStatus"));
    assert!(root_mod.contains("pub use user::*;"));
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
            shadow: Default::default(),
            codegen: shki::CodegenArgs {
                config: shki::codegen::CodegenConfig {
                    output: Some(output_dir.clone()),
                    format: Some(OutputMode::Module),
                    ..Default::default()
                },
                ..Default::default()
            },
            language: CodegenLanguage::Protobuf,
            source: Some(snapshot_path),
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
async fn codegen_compiles_current_declarative_schema_with_shadow_database() {
    let ctx = PgTestContext::setup("codegen_shadow").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let output_dir = ctx.root_dir().join("generated-shadow-ts");
    let table_name = ctx.unique_name("codegen_users");
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table_name} (id integer primary key, email text not null);\n"),
    )
    .expect("failed to write declarative schema");

    run(Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Codegen {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            codegen: shki::CodegenArgs {
                config: shki::codegen::CodegenConfig {
                    output: Some(output_dir.clone()),
                    format: Some(OutputMode::Module),
                    ..Default::default()
                },
                ..Default::default()
            },
            language: CodegenLanguage::Typescript {
                flavor: TypescriptFlavor::Interface,
            },
            source: None,
        },
    })
    .await
    .expect("typescript codegen should compile current declarative schema");

    let user = std::fs::read_dir(&output_dir)
        .expect("generated output dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("ts"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .find(|content| content.contains("email"))
        .expect("generated interface should be written");
    assert!(user.contains("interface"));
    assert!(user.contains("email"));

    shadow.cleanup().await;
    ctx.cleanup().await;
}

#[cfg(feature = "querygen")]
#[tokio::test]
async fn querygen_describes_and_writes_typed_wrappers() {
    let ctx = PgTestContext::setup("querygen").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let output = ctx.root_dir().join("queries.rs");
    let users = ctx.unique_name("users");

    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {users} (id integer primary key, email text not null, bio text);\n"),
    )
    .expect("failed to write declarative schema");
    let query_dir = ctx.root_dir().join("queries");
    std::fs::create_dir(&query_dir).expect("failed to create query dir");
    std::fs::write(
        query_dir.join("users.sql"),
        format!(
            "-- name: user_by_email :one\n\
             SELECT * FROM {users} WHERE email = $email;\n\n\
             -- name: user_emails :many\n\
             SELECT id, email FROM {users} WHERE email <> $excluded;\n\n\
             -- name: deactivate_user :exec :tx\n\
             UPDATE {users} SET bio = NULL WHERE id = $1;\n\n\
             -- name: user_page :batch\n\
             SELECT * FROM {users} ORDER BY id LIMIT $limit OFFSET $offset;\n\n\
             -- name: users_after :batch :keyset $cursor=id\n\
             SELECT * FROM {users} WHERE id > $cursor ORDER BY id LIMIT $limit;\n"
        ),
    )
    .expect("failed to write query fixture");

    run(Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Queries {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            querygen: shki::QueriesArgs {
                config: shki::codegen::queries::QueriesConfig {
                    output: Some(output.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
        },
    })
    .await
    .expect("querygen should describe and write query wrappers");

    let generated = std::fs::read_to_string(output).expect("queries should be written");
    assert!(generated.contains("pub async fn user_by_email"));
    assert!(generated.contains("email: String"));
    assert!(generated.contains("sqlx::Result<Option<"));
    assert!(!generated.contains("UserByEmailRow"));
    assert!(generated.contains("pub struct UserEmailsRow"));
    assert!(generated.contains("pub async fn deactivate_user"));
    assert!(generated.contains("transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>"));
    assert!(generated.contains(".execute(&mut **transaction)"));
    assert!(generated.contains("sqlx::Result<u64>"));
    assert!(generated.contains("pub struct Pagination"));
    assert!(generated.contains("sqlx::Result<Page<"));
    assert!(generated.contains("pub struct KeysetPage"));
    assert!(generated.contains("sqlx::Result<KeysetPage<"));
    assert!(generated.contains("CursorPagination::new(row.id.clone())"));

    shadow.cleanup().await;
    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_catalog_includes_functions_and_triggers() {
    let ctx = PgTestContext::setup("catalog_functions_triggers").await;
    let table_name = ctx.unique_name("audited_users");
    let function_name = ctx.unique_name("format_label");
    let trigger_function_name = ctx.unique_name("touch_updated_at");
    let trigger_name = ctx.unique_name("set_updated_at");

    let fixture_sql = format!(
        r#"
        CREATE TABLE "{schema}"."{table}" (
            id integer primary key,
            updated_at timestamp without time zone
        );

        CREATE FUNCTION "{schema}"."{function}"(prefix text, retries integer DEFAULT 0)
        RETURNS text
        LANGUAGE sql
        AS $$
            SELECT prefix || retries::text
        $$;

        CREATE FUNCTION "{schema}"."{trigger_function}"()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$
        BEGIN
            NEW.updated_at = now();
            RETURN NEW;
        END;
        $$;

        CREATE TRIGGER "{trigger}"
        BEFORE INSERT OR UPDATE ON "{schema}"."{table}"
        FOR EACH ROW
        EXECUTE FUNCTION "{schema}"."{trigger_function}"();
        "#,
        schema = ctx.schema_name,
        table = table_name,
        function = function_name,
        trigger_function = trigger_function_name,
        trigger = trigger_name,
    );
    sqlx::raw_sql(AssertSqlSafe(fixture_sql))
        .execute(&ctx.pg_pool)
        .await
        .expect("failed to create function and trigger fixture");

    let config = Config {
        common: shki::CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..Default::default()
        },
        ..Config::default()
    };
    let snapshot = ctx
        .engine(Iden::new(
            "__shki_migrations",
            Some(ctx.schema_name.clone()),
        ))
        .introspect(&config, &Some(ctx.schema_name.clone()))
        .await
        .expect("postgres snapshot should introspect functions and triggers");

    let functions = snapshot.functions();
    let function = functions
        .values()
        .find(|function| function.name == function_name)
        .expect("trigger function should be present in catalog");
    assert_eq!(function.schema.as_deref(), Some(ctx.schema_name.as_str()));
    assert_eq!(function.return_type, Some(DataType::Text));
    assert_eq!(function.language.as_deref(), Some("sql"));
    assert_eq!(function.parameters.len(), 2);
    assert_eq!(function.parameters[0].name.as_deref(), Some("prefix"));
    assert_eq!(function.parameters[0].data_type, DataType::Text);
    assert_eq!(function.parameters[1].name.as_deref(), Some("retries"));
    assert_eq!(function.parameters[1].data_type, DataType::Integer);
    assert!(
        function
            .body
            .as_deref()
            .unwrap_or_default()
            .contains("prefix")
    );

    let triggers = snapshot.triggers();
    let trigger = triggers
        .values()
        .find(|trigger| trigger.name == trigger_name)
        .expect("table trigger should be present in catalog");
    assert_eq!(
        trigger.table,
        Iden::new(table_name, Some(ctx.schema_name.clone()))
    );
    assert_eq!(
        trigger.function,
        Iden::new(trigger_function_name, Some(ctx.schema_name.clone()))
    );
    assert_eq!(trigger.timing, Some(shki::schema::TriggerTiming::Before));
    assert_eq!(
        trigger.orientation,
        Some(shki::schema::TriggerOrientation::Row)
    );
    assert!(trigger.events.contains(&shki::schema::TriggerEvent::Insert));
    assert!(trigger.events.contains(&shki::schema::TriggerEvent::Update));

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_catalog_includes_composite_types_and_domains() {
    let ctx = PgTestContext::setup("catalog_types_domains").await;
    let composite_name = ctx.unique_name("postal_address");
    let domain_name = ctx.unique_name("email_address");

    let fixture_sql = format!(
        r#"
        CREATE TYPE "{schema}"."{composite}" AS (
            street text,
            zip_code integer
        );

        CREATE DOMAIN "{schema}"."{domain}" AS text
        NOT NULL
        CHECK (VALUE LIKE '%@%');
        "#,
        schema = ctx.schema_name,
        composite = composite_name,
        domain = domain_name,
    );
    sqlx::raw_sql(AssertSqlSafe(fixture_sql))
        .execute(&ctx.pg_pool)
        .await
        .expect("failed to create composite type and domain fixture");

    let config = Config {
        common: shki::CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..Default::default()
        },
        ..Config::default()
    };
    let snapshot = ctx
        .engine(Iden::new(
            "__shki_migrations",
            Some(ctx.schema_name.clone()),
        ))
        .introspect(&config, &Some(ctx.schema_name.clone()))
        .await
        .expect("postgres snapshot should introspect composite types and domains");

    let composite_types = snapshot.composite_types();
    let composite = composite_types
        .values()
        .find(|composite_type| composite_type.name == composite_name)
        .expect("composite type should be present in catalog");
    assert_eq!(composite.schema.as_deref(), Some(ctx.schema_name.as_str()));
    assert_eq!(composite.columns.len(), 2);
    assert_eq!(composite.columns[0].name, "street");
    assert_eq!(composite.columns[0].data_type, DataType::Text);
    assert_eq!(composite.columns[1].name, "zip_code");
    assert_eq!(composite.columns[1].data_type, DataType::Integer);

    let domains = snapshot.domains();
    let domain = domains
        .values()
        .find(|domain| domain.name == domain_name)
        .expect("domain should be present in catalog");
    assert_eq!(domain.schema.as_deref(), Some(ctx.schema_name.as_str()));
    assert_eq!(domain.base_type, DataType::Text);
    assert!(domain.not_null);
    assert!(domain.constraints.iter().any(|constraint| {
        constraint.definition.contains("CHECK") && constraint.definition.contains("VALUE")
    }));

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_catalog_includes_procedures_aggregates_rls_and_partitions() {
    let ctx = PgTestContext::setup("catalog_remaining_objects").await;
    let procedure_name = ctx.unique_name("record_audit");
    let state_function_name = ctx.unique_name("sum_state");
    let aggregate_name = ctx.unique_name("sum_all");
    let rls_table = ctx.unique_name("tenant_docs");
    let policy_name = ctx.unique_name("tenant_docs_policy");
    let parent_table = ctx.unique_name("events");
    let child_table = ctx.unique_name("events_2026");

    let fixture_sql = format!(
        r#"
        CREATE PROCEDURE "{schema}"."{procedure}"(message text)
        LANGUAGE plpgsql
        AS $$
        BEGIN
            PERFORM message;
        END;
        $$;

        CREATE FUNCTION "{schema}"."{state_function}"(state integer, value integer)
        RETURNS integer
        LANGUAGE sql
        IMMUTABLE
        AS $$ SELECT COALESCE(state, 0) + COALESCE(value, 0) $$;

        CREATE AGGREGATE "{schema}"."{aggregate}"(integer) (
            SFUNC = "{schema}"."{state_function}",
            STYPE = integer,
            INITCOND = '0'
        );

        CREATE TABLE "{schema}"."{rls_table}" (
            tenant_id integer,
            body text
        );
        ALTER TABLE "{schema}"."{rls_table}" ENABLE ROW LEVEL SECURITY;
        CREATE POLICY "{policy}" ON "{schema}"."{rls_table}"
            FOR SELECT
            USING (tenant_id > 0);

        CREATE TABLE "{schema}"."{parent}" (
            id integer,
            created_at date not null
        ) PARTITION BY RANGE (created_at);
        CREATE TABLE "{schema}"."{child}" PARTITION OF "{schema}"."{parent}"
            FOR VALUES FROM ('2026-01-01') TO ('2027-01-01');
        "#,
        schema = ctx.schema_name,
        procedure = procedure_name,
        state_function = state_function_name,
        aggregate = aggregate_name,
        rls_table = rls_table,
        policy = policy_name,
        parent = parent_table,
        child = child_table,
    );
    sqlx::raw_sql(AssertSqlSafe(fixture_sql))
        .execute(&ctx.pg_pool)
        .await
        .expect("failed to create remaining catalog fixture");

    let config = Config {
        common: shki::CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..Default::default()
        },
        ..Config::default()
    };
    let snapshot = ctx
        .engine(Iden::new(
            "__shki_migrations",
            Some(ctx.schema_name.clone()),
        ))
        .introspect(&config, &Some(ctx.schema_name.clone()))
        .await
        .expect("postgres snapshot should introspect remaining catalog objects");

    let procedure = snapshot
        .procedures()
        .values()
        .find(|procedure| procedure.name == procedure_name)
        .expect("procedure should be present in catalog")
        .clone();
    assert_eq!(procedure.language.as_deref(), Some("plpgsql"));
    assert_eq!(procedure.parameters.len(), 1);
    assert_eq!(procedure.parameters[0].data_type, DataType::Text);

    let aggregate = snapshot
        .aggregates()
        .values()
        .find(|aggregate| aggregate.name == aggregate_name)
        .expect("aggregate should be present in catalog")
        .clone();
    assert_eq!(aggregate.return_type, DataType::Integer);
    assert_eq!(aggregate.state_type, DataType::Integer);
    assert_eq!(aggregate.parameters.len(), 1);
    assert_eq!(aggregate.parameters[0].data_type, DataType::Integer);

    let rls = snapshot.row_level_security();
    assert!(rls.contains_key(&Iden::new(rls_table.clone(), Some(ctx.schema_name.clone()))));

    let policies = snapshot.row_level_security_policies();
    let policy = policies
        .values()
        .find(|policy| policy.name == policy_name)
        .expect("RLS policy should be present in catalog");
    assert_eq!(
        policy.table,
        Iden::new(rls_table, Some(ctx.schema_name.clone()))
    );
    assert_eq!(policy.command, "SELECT");
    assert!(
        policy
            .using_expression
            .as_deref()
            .unwrap_or_default()
            .contains("tenant_id")
    );

    let attachments = snapshot.partition_attachments();
    let attachment = attachments
        .values()
        .find(|attachment| attachment.child.name == child_table)
        .expect("partition attachment should be present in catalog");
    assert_eq!(
        attachment.parent,
        Iden::new(parent_table, Some(ctx.schema_name.clone()))
    );
    assert!(attachment.bound.contains("2026-01-01"));

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_catalog_includes_privileges() {
    let ctx = PgTestContext::setup("catalog_privileges").await;
    let table_name = ctx.unique_name("secure_docs");
    let role_name = format!("shki_test_role_{}", unique_suffix());

    let fixture_sql = format!(
        r#"
        CREATE ROLE "{role}";
        CREATE TABLE "{schema}"."{table}" (
            id integer,
            body text
        );
        GRANT SELECT ON "{schema}"."{table}" TO "{role}";
        GRANT UPDATE (body) ON "{schema}"."{table}" TO "{role}";
        ALTER DEFAULT PRIVILEGES IN SCHEMA "{schema}" GRANT SELECT ON TABLES TO "{role}";
        ALTER DEFAULT PRIVILEGES IN SCHEMA "{schema}" REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;
        "#,
        schema = ctx.schema_name,
        table = table_name,
        role = role_name,
    );
    sqlx::raw_sql(AssertSqlSafe(fixture_sql))
        .execute(&ctx.pg_pool)
        .await
        .expect("failed to create privilege fixture");

    let config = Config {
        common: shki::CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..Default::default()
        },
        ..Config::default()
    };
    let snapshot = ctx
        .engine(Iden::new(
            "__shki_migrations",
            Some(ctx.schema_name.clone()),
        ))
        .introspect(&config, &Some(ctx.schema_name.clone()))
        .await
        .expect("postgres snapshot should introspect privileges");

    assert!(snapshot.object_privileges().iter().any(|privilege| {
        privilege.object == Iden::new(table_name.clone(), Some(ctx.schema_name.clone()))
            && privilege.grantee == role_name
            && privilege.privilege_type == "SELECT"
    }));
    assert!(snapshot.column_privileges().iter().any(|privilege| {
        privilege.table == Iden::new(table_name.clone(), Some(ctx.schema_name.clone()))
            && privilege.column == "body"
            && privilege.grantee == role_name
            && privilege.privilege_type == "UPDATE"
    }));
    assert!(snapshot.default_privileges().iter().any(|privilege| {
        privilege.object_type == "TABLES"
            && privilege.grantee == role_name
            && privilege.privilege_type == "SELECT"
    }));

    let cleanup_sql = format!(
        r#"ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA "{schema}" REVOKE ALL ON TABLES FROM "{role}";
DROP OWNED BY "{role}";
DROP ROLE IF EXISTS "{role}""#,
        schema = ctx.schema_name,
        role = role_name,
    );
    sqlx::raw_sql(AssertSqlSafe(cleanup_sql))
        .execute(&ctx.pg_pool)
        .await
        .expect("failed to drop test role");
    ctx.cleanup().await;
}

#[tokio::test]
async fn cli_drop_removes_pending_schema_migration_snapshot_and_journal_entry() {
    let ctx = PgTestContext::setup("cli_drop_schema").await;
    let shadow = engines::pg::TestDatabase::start().await;
    let config_path = ctx.write_config();
    let table_name = ctx.unique_name("drop_generated_users");
    std::fs::write(
        ctx.root_dir().join("schema"),
        format!("CREATE TABLE {table_name} (id integer primary key, name text not null);\n"),
    )
    .expect("failed to write declarative schema");

    run(shki::Cli {
        config: config_path.clone(),
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Generate {
            shadow: shki::ShadowArgs {
                shadow_database_url: Some(shadow.database_url.clone()),
                ..Default::default()
            },
            migrations: Default::default(),
            name: "create generated users".to_string(),
            custom: false,
            with_down: true,
        },
    })
    .await
    .expect("generate should write migration artifacts");

    let up_path = ctx.migrations_dir().join("0000_create-generated-users.sql");
    let down_path = ctx
        .migrations_dir()
        .join("0000_create-generated-users.down.sql");
    let snapshot_path = ctx
        .migrations_dir()
        .join("_meta/0000_create-generated-users.snapshot.json");
    let journal_path = ctx.migrations_dir().join("_meta/_journal.json");
    assert!(up_path.exists());
    assert!(down_path.exists());
    assert!(snapshot_path.exists());

    run(shki::Cli {
        config: config_path,
        common: CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            ..CommonArgs::default()
        },
        command: Commands::Drop {
            migration: Some("create-generated-users".to_string()),
            force: false,
        },
    })
    .await
    .expect("drop should remove pending schema migration artifacts");

    assert!(!up_path.exists());
    assert!(!down_path.exists());
    assert!(!snapshot_path.exists());
    let journal_json = std::fs::read_to_string(&journal_path).expect("journal should remain");
    let journal: Journal = serde_json::from_str(&journal_json).expect("journal should parse");
    assert!(journal.entries.is_empty());

    shadow.cleanup().await;
    ctx.cleanup().await;
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
        common: shki::CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            database_url: Some(ctx.database_url()),
            ..Default::default()
        },
        schema: schema_path,
        shadow: shki::ShadowArgs {
            shadow_database_url: Some(shadow.database_url.clone()),
            ..Default::default()
        },
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

    shadow.cleanup().await;
    ctx.cleanup().await;
}

#[tokio::test]
async fn compiler_tracks_postgis_extension_and_geometry_columns() {
    let snapshot = compile_extension_schema(
        "postgis/postgis",
        "16-3.4-alpine",
        r#"
        CREATE EXTENSION postgis;
        CREATE TABLE places (
            location geometry(Point, 4326) NOT NULL,
            route geometry(LineString, 4326),
            boundary geometry(Polygon, 4326),
            coverage geography(Point, 4326)
        );
        "#,
    )
    .await;

    assert!(snapshot.extensions().contains(&"postgis".to_string()));
    let places = snapshot
        .tables()
        .into_iter()
        .find(|(iden, _)| iden.name == "places")
        .expect("places should be in the snapshot");
    assert_eq!(
        places.1.columns["location"].data_type,
        DataType::Custom {
            name: "geometry(Point,4326)".to_string(),
            schema: None,
        }
    );
    assert_eq!(
        places.1.columns["route"].data_type,
        DataType::Custom {
            name: "geometry(LineString,4326)".to_string(),
            schema: None,
        }
    );
    assert_eq!(
        places.1.columns["boundary"].data_type,
        DataType::Custom {
            name: "geometry(Polygon,4326)".to_string(),
            schema: None,
        }
    );
    assert_eq!(
        places.1.columns["coverage"].data_type,
        DataType::Custom {
            name: "geography(Point,4326)".to_string(),
            schema: None,
        }
    );

    let sql = shki::dump::render_snapshot(&snapshot, &SchemaExportFormat::Sql)
        .expect("snapshot should render as SQL");
    assert!(sql.contains("\"location\" \"geometry\"(Point,4326) NOT NULL"));
    assert!(sql.contains("\"route\" \"geometry\"(LineString,4326)"));
    assert!(sql.contains("\"boundary\" \"geometry\"(Polygon,4326)"));
    assert!(sql.contains("\"coverage\" \"geography\"(Point,4326)"));
}

#[tokio::test]
async fn compiler_tracks_pgvector_extension_and_vector_columns() {
    let snapshot = compile_extension_schema(
        "pgvector/pgvector",
        "0.8.0-pg16",
        "CREATE EXTENSION vector;\nCREATE TABLE embeddings (embedding vector(3) NOT NULL);\n",
    )
    .await;

    assert!(snapshot.extensions().contains(&"vector".to_string()));
    let embeddings = snapshot
        .tables()
        .into_iter()
        .find(|(iden, _)| iden.name == "embeddings")
        .expect("embeddings should be in the snapshot");
    assert_eq!(
        embeddings.1.columns["embedding"].data_type,
        DataType::Custom {
            name: "vector(3)".to_string(),
            schema: None,
        }
    );
}

#[tokio::test]
async fn dump_retains_pgvector_type_modifiers() {
    let snapshot = compile_extension_schema(
        "pgvector/pgvector",
        "0.8.0-pg16",
        "CREATE EXTENSION vector;\nCREATE TABLE embeddings (embedding vector(3) NOT NULL);\n",
    )
    .await;

    let sql = shki::dump::render_snapshot(&snapshot, &SchemaExportFormat::Sql)
        .expect("snapshot should render as SQL");
    assert!(sql.contains("\"embedding\" \"vector\"(3) NOT NULL"));
}

#[tokio::test]
async fn diff_alters_pgvector_halfvec_dimensions() {
    let before = compile_extension_schema(
        "pgvector/pgvector",
        "0.8.0-pg16",
        "CREATE EXTENSION vector;\nCREATE TABLE embeddings (embedding halfvec(384) NOT NULL);\n",
    )
    .await;
    let after = compile_extension_schema(
        "pgvector/pgvector",
        "0.8.0-pg16",
        "CREATE EXTENSION vector;\nCREATE TABLE embeddings (embedding halfvec(768) NOT NULL);\n",
    )
    .await;

    let before_type = before
        .tables()
        .into_iter()
        .find(|(iden, _)| iden.name == "embeddings")
        .expect("baseline embeddings should be in the snapshot")
        .1
        .columns["embedding"]
        .data_type
        .clone();
    let after_type = after
        .tables()
        .into_iter()
        .find(|(iden, _)| iden.name == "embeddings")
        .expect("altered embeddings should be in the snapshot")
        .1
        .columns["embedding"]
        .data_type
        .clone();
    assert_eq!(
        before_type,
        DataType::Custom {
            name: "halfvec(384)".to_string(),
            schema: None,
        }
    );
    assert_eq!(
        after_type,
        DataType::Custom {
            name: "halfvec(768)".to_string(),
            schema: None,
        }
    );

    let diff = shki::diff::diff_snapshots(&before, &after).expect("snapshots should diff");
    assert!(diff.statements.iter().any(|statement| {
        matches!(
            statement,
            shki::diff::DiffStatement::AlterColumn { table, column, changes, .. }
                if table == "embeddings"
                    && column == "embedding"
                    && changes.iter().any(|change| matches!(
                        change,
                        shki::diff::ColumnChange::SetType(data_type) if data_type == "\"halfvec\"(768)"
                    ))
        )
    }));
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
        common: shki::CommonArgs {
            dialect: Some(shki::schema::SqlDialect::Postgres),
            database_url: Some(ctx.database_url()),
            ..Default::default()
        },
        schema: schema_dir,
        shadow: shki::ShadowArgs {
            shadow_database_url: Some(shadow.database_url.clone()),
            ..Default::default()
        },
        ..Config::default()
    };

    let snapshot = ExternalShadowDBCompiler::from_config(&config)
        .expect("compiler should configure")
        .compile(&config)
        .await
        .expect("directory schema should compile");

    assert!(snapshot.tables().keys().any(|id| id.name == table_name));

    shadow.cleanup().await;
    ctx.cleanup().await;
}

// --- Bootstrap (authoring) and Adopt (environment adoption) ---------------

fn seed_table_sql(schema: &str, table: &str) -> String {
    format!(
        "CREATE TABLE \"{schema}\".\"{table}\" (id integer primary key, name varchar(255) not null);"
    )
}

#[tokio::test]
async fn postgres_bootstrap_authors_baseline_without_touching_db() {
    let ctx = PgTestContext::setup("bootstrap_author").await;
    let config_path = ctx.write_config();
    let schema = ctx.migration_schema().expect("schema").to_string();
    let users = ctx.unique_name("users");

    ctx.pg_pool
        .execute(AssertSqlSafe(seed_table_sql(&schema, &users)))
        .await
        .expect("failed to seed existing schema");

    run(shki::Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Bootstrap {
            migrations: Default::default(),
            name: None,
            dry_run: false,
            force: false,
            schema: Some(schema.clone()),
        },
    })
    .await
    .expect("bootstrap should author baseline artifacts");

    // Artifacts written.
    let up_path = ctx.migrations_dir().join("0000_bootstrap.sql");
    let snapshot_path = ctx
        .migrations_dir()
        .join("_meta")
        .join("0000_bootstrap.snapshot.json");
    assert!(up_path.exists(), "baseline migration should be written");
    assert!(
        snapshot_path.exists(),
        "baseline snapshot should be written"
    );
    let up_sql = std::fs::read_to_string(&up_path).expect("read baseline migration");
    assert!(up_sql.contains("CREATE TABLE"));
    assert!(up_sql.contains(&users));

    let manager = ctx.manager();
    let journal = manager.load_journal().expect("journal should load");
    assert_eq!(journal.entries.len(), 1);
    assert_eq!(journal.entries[0].migration, "0000_bootstrap");
    assert_eq!(journal.entries[0].kind, MigrationKind::Schema);

    // Bootstrap must not touch the target database.
    assert!(
        !ctx.migration_table_exists(&manager).await,
        "bootstrap should not create the migrations table"
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_adopt_marks_baseline_and_applies_newer_migration() {
    let ctx = PgTestContext::setup("adopt_marks_baseline").await;
    let config_path = ctx.write_config();
    let schema = ctx.migration_schema().expect("schema").to_string();
    let users = ctx.unique_name("users");
    let posts = ctx.unique_name("posts");

    ctx.pg_pool
        .execute(AssertSqlSafe(seed_table_sql(&schema, &users)))
        .await
        .expect("failed to seed existing schema");

    // Author the baseline from the existing database.
    run(shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Bootstrap {
            migrations: Default::default(),
            name: None,
            dry_run: false,
            force: false,
            schema: Some(schema.clone()),
        },
    })
    .await
    .expect("bootstrap should author baseline");

    // A newer shki migration is created after the baseline.
    ctx.write_migration(
        "0001_create_posts.sql",
        &format!("CREATE TABLE \"{schema}\".\"{posts}\" (id integer primary key);"),
    );

    // Adopt the existing database: validate, mark baseline applied, apply the rest.
    run(shki::Cli {
        config: config_path,
        common: ctx.common_args(),
        command: Commands::Adopt {
            migrations: Default::default(),
            name: None,
            mark_only: false,
            force: false,
            dry_run: false,
            schema: Some(schema.clone()),
        },
    })
    .await
    .expect("adopt should validate, mark baseline, and apply newer migration");

    let manager = ctx.manager();
    assert_eq!(
        ctx.applied_names(&manager).await,
        vec![
            "0000_bootstrap".to_string(),
            "0001_create_posts".to_string()
        ]
    );
    // Baseline table was never re-run, and the newer migration created its table.
    assert!(ctx.table_exists(&users).await);
    assert!(ctx.table_exists(&posts).await);

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_adopt_refuses_on_drift_then_succeeds_with_force() {
    let ctx = PgTestContext::setup("adopt_drift").await;
    let config_path = ctx.write_config();
    let schema = ctx.migration_schema().expect("schema").to_string();
    let users = ctx.unique_name("users");
    let drift = ctx.unique_name("drift");

    ctx.pg_pool
        .execute(AssertSqlSafe(seed_table_sql(&schema, &users)))
        .await
        .expect("failed to seed existing schema");

    run(shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Bootstrap {
            migrations: Default::default(),
            name: None,
            dry_run: false,
            force: false,
            schema: Some(schema.clone()),
        },
    })
    .await
    .expect("bootstrap should author baseline");

    // Introduce drift: the live database now has a table the baseline doesn't.
    ctx.pg_pool
        .execute(AssertSqlSafe(format!(
            "CREATE TABLE \"{schema}\".\"{drift}\" (id integer primary key);"
        )))
        .await
        .expect("failed to introduce drift");

    let adopt_cli = |force: bool| shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Adopt {
            migrations: Default::default(),
            name: None,
            mark_only: false,
            force,
            dry_run: false,
            schema: Some(schema.clone()),
        },
    };

    let error = run(adopt_cli(false))
        .await
        .expect_err("adopt should refuse when the live database drifts from the baseline");
    assert!(error.to_string().contains("differs from"));

    let manager = ctx.manager();
    assert!(
        !ctx.migration_table_exists(&manager).await,
        "a refused adopt should not record anything"
    );

    run(adopt_cli(true))
        .await
        .expect("adopt --force should proceed despite drift");
    assert_eq!(
        ctx.applied_names(&manager).await,
        vec!["0000_bootstrap".to_string()]
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn postgres_fresh_environment_runs_baseline_via_migrate() {
    let ctx = PgTestContext::setup("fresh_env_migrate").await;
    let config_path = ctx.write_config();
    let schema = ctx.migration_schema().expect("schema").to_string();
    let users = ctx.unique_name("users");

    ctx.pg_pool
        .execute(AssertSqlSafe(seed_table_sql(&schema, &users)))
        .await
        .expect("failed to seed existing schema");

    run(shki::Cli {
        config: config_path.clone(),
        common: ctx.common_args(),
        command: Commands::Bootstrap {
            migrations: Default::default(),
            name: None,
            dry_run: false,
            force: false,
            schema: Some(schema.clone()),
        },
    })
    .await
    .expect("bootstrap should author baseline");

    // Simulate a fresh environment: drop the table so the baseline must recreate it.
    ctx.pg_pool
        .execute(AssertSqlSafe(format!(
            "DROP TABLE \"{schema}\".\"{users}\";"
        )))
        .await
        .expect("failed to reset to a fresh environment");
    assert!(!ctx.table_exists(&users).await);

    // On a fresh database the baseline runs like any migration. The baseline's
    // `CREATE SCHEMA IF NOT EXISTS` tolerates the schema the migrations table lives in.
    run(ctx.migrate_cli(config_path))
        .await
        .expect("migrate should run the baseline on a fresh database");

    let manager = ctx.manager();
    assert!(ctx.table_exists(&users).await);
    assert_eq!(
        ctx.applied_names(&manager).await,
        vec!["0000_bootstrap".to_string()]
    );

    ctx.cleanup().await;
}
