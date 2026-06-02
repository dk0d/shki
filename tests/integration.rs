mod engines;
use engines::*;
mod common;

use shki::models::iden::Iden;
use shki::run;
use shki::snapshots::Snapshot;
use shki::{Commands, CommonArgs, PullFormat};

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
