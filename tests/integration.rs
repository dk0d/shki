mod engines;
use engines::*;
mod common;

use shki::migrate::manager::MigrationManager;
use shki::run;

use self::common::*;

async fn scenario_apply_simple(ctx: BackendContext) {
    let manager = ctx.manager();
    let table_name = ctx.unique_name("users");
    let migration_path = ctx.write_migration(
        "0001_create_users.sql",
        &ctx.create_table_sql(&table_name, &[format!("name {} NOT NULL", ctx.text_type())]),
    );

    manager
        .apply_migration(&ctx.pool().await, &migration_path)
        .await
        .expect("failed to apply migration");

    assert!(ctx.table_exists(&table_name).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_users"]);

    ctx.cleanup().await;
}

async fn scenario_apply_all_and_pending_detection(ctx: BackendContext) {
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
                .get_pending_migrations(&ctx.pool().await)
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
        .apply_migration(
            &ctx.pool().await,
            &ctx.migrations_dir().join("0001_create_users.sql"),
        )
        .await
        .expect("failed to apply first migration");

    assert_eq!(
        migration_names(
            manager
                .get_pending_migrations(&ctx.pool().await)
                .await
                .expect("failed to reload pending migrations")
        ),
        vec![
            "0002_create_posts".to_string(),
            "0003_create_logs".to_string()
        ]
    );

    let applied = manager
        .apply_all(&ctx.pool().await)
        .await
        .expect("failed to apply all pending migrations");

    assert_eq!(applied, vec!["0002_create_posts", "0003_create_logs"]);
    assert!(ctx.table_exists(&users).await);
    assert!(ctx.table_exists(&posts).await);
    assert!(ctx.table_exists(&logs).await);
    assert!(
        manager
            .get_pending_migrations(&ctx.pool().await)
            .await
            .expect("failed to read final pending migrations")
            .is_empty()
    );

    ctx.cleanup().await;
}

async fn scenario_rollback_single(ctx: BackendContext) {
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
        .apply_migration(&ctx.pool().await, &up_path)
        .await
        .expect("failed to apply migration");
    manager
        .rollback_migration(&ctx.pool().await, &down_path)
        .await
        .expect("failed to rollback migration");

    assert!(!ctx.table_exists(&table_name).await);
    assert!(ctx.applied_names(&manager).await.is_empty());

    ctx.cleanup().await;
}

async fn scenario_rollback_all(ctx: BackendContext) {
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
        .apply_all(&ctx.pool().await)
        .await
        .expect("failed to apply all migrations");

    let rolled_back = manager
        .rollback_all(&ctx.pool().await)
        .await
        .expect("failed to rollback all migrations");

    assert_eq!(rolled_back, vec!["0002_create_posts", "0001_create_users"]);
    assert!(!ctx.table_exists(&users).await);
    assert!(!ctx.table_exists(&posts).await);

    ctx.cleanup().await;
}

async fn scenario_rollback_count(ctx: BackendContext) {
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
        .apply_all(&ctx.pool().await)
        .await
        .expect("failed to apply all migrations");

    let rolled_back = manager
        .rollback_count(&ctx.pool().await, 2)
        .await
        .expect("failed to rollback migrations");

    assert_eq!(rolled_back, vec!["0003_create_tbl3", "0002_create_tbl2"]);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_tbl1"]);

    ctx.cleanup().await;
}

async fn scenario_transaction_rollback_on_error(ctx: BackendContext) {
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

    let result = manager
        .apply_migration(&ctx.pool().await, &migration_path)
        .await;

    assert!(result.is_err());
    assert!(!ctx.table_exists(&table_name).await);
    assert!(ctx.applied_names(&manager).await.is_empty());

    ctx.cleanup().await;
}

async fn scenario_checksum_validation_blocks_new_migrations(ctx: BackendContext) {
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
        .apply_migration(&ctx.pool().await, &first)
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

async fn scenario_custom_migration_table(ctx: BackendContext) {
    let manager = match ctx.migration_schema() {
        Some(schema) => MigrationManager::new(ctx.migrations_dir())
            .with_dialect(ctx.dialect())
            .with_table_schema(schema)
            .with_table_name("custom_migrations"),
        None => MigrationManager::new(ctx.migrations_dir())
            .with_dialect(ctx.dialect())
            .with_table_name("custom_migrations"),
    };

    manager
        .ensure_migrations_table(&ctx.pool().await)
        .await
        .expect("failed to ensure custom migrations table");

    assert!(ctx.migration_table_exists(&manager).await);

    ctx.cleanup().await;
}

async fn scenario_cli_migrate_applies_pending(ctx: BackendContext) {
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

async fn scenario_cli_down_dry_run_does_not_modify_database(ctx: BackendContext) {
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
        .apply_migration(&ctx.pool().await, &up_path)
        .await
        .expect("failed to apply migration");

    run(ctx.down_cli(config_path, Some(1), true))
        .await
        .expect("dry-run down should succeed");

    assert!(ctx.table_exists(&table_name).await);
    assert_eq!(ctx.applied_names(&manager).await, vec!["0001_create_logs"]);

    ctx.cleanup().await;
}

macro_rules! backend_suite {
    ($module:ident, $setup:expr) => {
        mod $module {
            use super::*;

            #[tokio::test]
            async fn apply_simple() {
                scenario_apply_simple($setup("apply_simple").await).await;
            }

            #[tokio::test]
            async fn apply_all_and_pending_detection() {
                scenario_apply_all_and_pending_detection($setup("apply_all").await).await;
            }

            #[tokio::test]
            async fn rollback_single() {
                scenario_rollback_single($setup("rollback_single").await).await;
            }

            #[tokio::test]
            async fn rollback_all() {
                scenario_rollback_all($setup("rollback_all").await).await;
            }

            #[tokio::test]
            async fn rollback_count() {
                scenario_rollback_count($setup("rollback_count").await).await;
            }

            #[tokio::test]
            async fn transaction_rollback_on_error() {
                scenario_transaction_rollback_on_error($setup("tx_rollback").await).await;
            }

            #[tokio::test]
            async fn checksum_validation_blocks_new_migrations() {
                scenario_checksum_validation_blocks_new_migrations($setup("checksum").await).await;
            }

            #[tokio::test]
            async fn custom_migration_table() {
                scenario_custom_migration_table($setup("migration_table").await).await;
            }

            #[tokio::test]
            async fn cli_migrate_applies_pending() {
                scenario_cli_migrate_applies_pending($setup("cli_migrate").await).await;
            }

            #[tokio::test]
            async fn cli_down_dry_run_does_not_modify_database() {
                scenario_cli_down_dry_run_does_not_modify_database($setup("cli_down").await).await;
            }
        }
    };
}

backend_suite!(sqlite, BackendContext::sqlite);
backend_suite!(postgres, BackendContext::postgres);
