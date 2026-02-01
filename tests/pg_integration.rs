//! PostgreSQL integration tests
//!
//! These tests require a running PostgreSQL instance.
//! Use `docker compose up -d postgres` to start the test database.
//!
//! Connection URL: postgresql://postgres:postgres@localhost:5432/shki_test
//!
//! Run these tests with: `cargo test --test pg_integration -- --ignored`

use shki::cli::commands::introspect::pg::introspect_postgres_schema;
use shki::migration::MigrationManager;
use shki::schema::SchemaDialect;
use shki::snapshot::ConstraintType;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AnyPool, Executor, Pool, Postgres};
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

/// Get the database URL from environment or use default
fn get_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/shki".into())
}

/// Create a connection pool for testing with retries
async fn create_pool() -> Pool<Postgres> {
    let url = get_database_url();
    let max_retries = 5;
    let retry_delay = Duration::from_secs(2);

    for attempt in 1..=max_retries {
        match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
            .await
        {
            Ok(pool) => return pool,
            Err(e) if attempt < max_retries => {
                eprintln!(
                    "Connection attempt {}/{} failed: {}. Retrying in {:?}...",
                    attempt, max_retries, e, retry_delay
                );
                tokio::time::sleep(retry_delay).await;
            }
            Err(e) => {
                panic!(
                    "Failed to connect to PostgreSQL after {} attempts. \
                     Is the database running? Use `docker compose up -d postgres`. \
                     Error: {}",
                    max_retries, e
                );
            }
        }
    }
    unreachable!()
}

/// Create an AnyPool for migration manager tests with retries
async fn create_any_pool() -> AnyPool {
    let url = get_database_url();
    sqlx::any::install_default_drivers();

    let max_retries = 5;
    let retry_delay = Duration::from_secs(2);

    for attempt in 1..=max_retries {
        match sqlx::any::AnyPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
            .await
        {
            Ok(pool) => return pool,
            Err(e) if attempt < max_retries => {
                eprintln!(
                    "AnyPool connection attempt {}/{} failed: {}. Retrying in {:?}...",
                    attempt, max_retries, e, retry_delay
                );
                tokio::time::sleep(retry_delay).await;
            }
            Err(e) => {
                panic!(
                    "Failed to connect to PostgreSQL with AnyPool after {} attempts. \
                     Is the database running? Use `docker compose up -d postgres`. \
                     Error: {}",
                    max_retries, e
                );
            }
        }
    }
    unreachable!()
}

/// Generate a unique schema name for test isolation
fn unique_schema_name(prefix: &str) -> String {
    let uuid = Uuid::new_v4().to_string().replace('-', "");
    format!("{}_{}", prefix, &uuid[..8])
}

/// Clean up test schema and recreate it
async fn setup_test_schema(pool: &Pool<Postgres>, schema_name: &str) {
    // Drop and recreate the schema for a clean slate
    pool.execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name).as_str())
        .await
        .expect("Failed to drop schema");
    pool.execute(format!("CREATE SCHEMA \"{}\"", schema_name).as_str())
        .await
        .expect("Failed to create schema");
}

/// Clean up after tests
async fn cleanup_test_schema(pool: &Pool<Postgres>, schema_name: &str) {
    pool.execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema_name).as_str())
        .await
        .expect("Failed to cleanup schema");
}

// ============================================================================
// Introspection Tests
// ============================================================================

mod introspection {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL: `cargo test --test pg_integration -- --ignored`
    async fn test_introspect_empty_database() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("empty");

        setup_test_schema(&pool, &schema_name).await;

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        // Should have the test schema
        assert!(snapshot.schemas.contains(&schema_name));

        // Should have no tables in our test schema
        assert!(snapshot.tables.is_empty());

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_simple_table() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("simple");
        // Use unique table name to avoid conflicts with parallel tests
        let table_name = format!("users_{}", &schema_name[7..15]);

        setup_test_schema(&pool, &schema_name).await;

        // Create a simple table
        pool.execute(
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    email VARCHAR(255) UNIQUE,
                    age INTEGER,
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                )
                "#,
                schema = schema_name,
                table = table_name
            )
            .as_str(),
        )
        .await
        .expect("Failed to create table");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        // Find the table
        let users_table = snapshot
            .tables
            .get(&table_name)
            .expect("users table not found");

        assert_eq!(users_table.name, table_name);
        assert_eq!(users_table.schema, Some(schema_name.clone()));

        // Check columns
        assert_eq!(users_table.columns.len(), 5);

        let id_col = users_table.columns.get("id").expect("id column not found");
        assert_eq!(id_col.data_type, "INTEGER");
        assert!(!id_col.nullable);

        let name_col = users_table
            .columns
            .get("name")
            .expect("name column not found");
        assert!(name_col.data_type.contains("VARCHAR"));
        assert!(!name_col.nullable);

        let email_col = users_table
            .columns
            .get("email")
            .expect("email column not found");
        assert!(email_col.nullable);

        let age_col = users_table
            .columns
            .get("age")
            .expect("age column not found");
        assert_eq!(age_col.data_type, "INTEGER");
        assert!(age_col.nullable);

        let created_at_col = users_table
            .columns
            .get("created_at")
            .expect("created_at column not found");
        assert_eq!(created_at_col.data_type, "TIMESTAMPTZ");

        // Check constraints
        let pk_constraint = users_table
            .constraints
            .iter()
            .find(|c| c.constraint_type == ConstraintType::PrimaryKey)
            .expect("Primary key constraint not found");
        assert!(pk_constraint.columns.contains(&"id".to_string()));

        let unique_constraint = users_table
            .constraints
            .iter()
            .find(|c| c.constraint_type == ConstraintType::Unique)
            .expect("Unique constraint not found");
        assert!(unique_constraint.columns.contains(&"email".to_string()));

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_foreign_key() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("fk");
        let users_table = format!("users_{}", &schema_name[3..11]);
        let posts_table = format!("posts_{}", &schema_name[3..11]);

        setup_test_schema(&pool, &schema_name).await;

        // Create tables with foreign key relationship
        pool.execute(
            format!(
                r#"
                CREATE TABLE "{schema}"."{users}" (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL
                );

                CREATE TABLE "{schema}"."{posts}" (
                    id SERIAL PRIMARY KEY,
                    title VARCHAR(255) NOT NULL,
                    user_id INTEGER NOT NULL REFERENCES "{schema}"."{users}"(id) ON DELETE CASCADE
                );
                "#,
                schema = schema_name,
                users = users_table,
                posts = posts_table
            )
            .as_str(),
        )
        .await
        .expect("Failed to create tables");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        let posts = snapshot
            .tables
            .get(&posts_table)
            .expect("posts table not found");

        // Find the foreign key constraint
        let fk_constraint = posts
            .constraints
            .iter()
            .find(|c| c.constraint_type == ConstraintType::ForeignKey)
            .expect("Foreign key constraint not found");

        assert!(fk_constraint.columns.contains(&"user_id".to_string()));

        let fk_ref = fk_constraint
            .references
            .as_ref()
            .expect("FK reference not found");
        assert_eq!(fk_ref.table, users_table);
        assert!(fk_ref.columns.contains(&"id".to_string()));
        assert_eq!(fk_ref.on_delete, "CASCADE");

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_indexes() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("idx");
        let table_name = format!("products_{}", &schema_name[4..12]);

        setup_test_schema(&pool, &schema_name).await;

        // Create table with indexes
        pool.execute(
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    sku VARCHAR(100) NOT NULL,
                    price NUMERIC(10, 2),
                    category VARCHAR(100)
                );

                CREATE INDEX idx_{table}_name ON "{schema}"."{table}"(name);
                CREATE UNIQUE INDEX idx_{table}_sku ON "{schema}"."{table}"(sku);
                CREATE INDEX idx_{table}_cat_price ON "{schema}"."{table}"(category, price);
                "#,
                schema = schema_name,
                table = table_name
            )
            .as_str(),
        )
        .await
        .expect("Failed to create table and indexes");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        let products = snapshot
            .tables
            .get(&table_name)
            .expect("products table not found");

        // Check indexes (excluding primary key index)
        assert!(products.indexes.len() >= 3);

        let name_idx = products
            .indexes
            .get(&format!("idx_{}_name", table_name))
            .expect("name index not found");
        assert!(!name_idx.unique);
        assert!(name_idx.columns.iter().any(|c| c.contains("name")));

        let sku_idx = products
            .indexes
            .get(&format!("idx_{}_sku", table_name))
            .expect("sku index not found");
        assert!(sku_idx.unique);

        let composite_idx = products
            .indexes
            .get(&format!("idx_{}_cat_price", table_name))
            .expect("composite index not found");
        assert_eq!(composite_idx.columns.len(), 2);

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_enum_type() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("enum");
        let enum_name = format!("status_{}", &schema_name[5..13]);
        let table_name = format!("orders_{}", &schema_name[5..13]);

        setup_test_schema(&pool, &schema_name).await;

        // Create enum type and table using it
        pool.execute(
            format!(
                r#"
                CREATE TYPE "{schema}"."{enum_type}" AS ENUM ('pending', 'active', 'inactive', 'deleted');

                CREATE TABLE "{schema}"."{table}" (
                    id SERIAL PRIMARY KEY,
                    status "{schema}"."{enum_type}" NOT NULL DEFAULT 'pending'
                );
                "#,
                schema = schema_name,
                enum_type = enum_name,
                table = table_name
            )
            .as_str(),
        )
        .await
        .expect("Failed to create enum and table");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        // Check enum was introspected
        let status_enum = snapshot
            .enums
            .get(&enum_name)
            .expect("status enum not found");

        assert_eq!(status_enum.name, enum_name);
        assert_eq!(status_enum.schema, Some(schema_name.clone()));
        assert_eq!(
            status_enum.values,
            vec!["pending", "active", "inactive", "deleted"]
        );

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_check_constraint() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("check");
        let table_name = format!("employees_{}", &schema_name[6..14]);

        setup_test_schema(&pool, &schema_name).await;

        // Create table with check constraint
        pool.execute(
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    salary NUMERIC(10, 2) NOT NULL,
                    CONSTRAINT salary_positive CHECK (salary > 0)
                );
                "#,
                schema = schema_name,
                table = table_name
            )
            .as_str(),
        )
        .await
        .expect("Failed to create table");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        let employees = snapshot
            .tables
            .get(&table_name)
            .expect("employees table not found");

        // Find the named check constraint (filter out system-generated NOT NULL checks)
        let check_constraint = employees
            .constraints
            .iter()
            .find(|c| {
                c.constraint_type == ConstraintType::Check
                    && c.name.as_ref().is_some_and(|n| n == "salary_positive")
            })
            .expect("Check constraint 'salary_positive' not found");

        assert_eq!(check_constraint.name, Some("salary_positive".to_string()));
        assert!(check_constraint.expression.is_some());

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_column_types() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("types");
        let table_name = format!("type_test_{}", &schema_name[6..14]);

        setup_test_schema(&pool, &schema_name).await;

        // Create table with various column types
        pool.execute(
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (
                    id SERIAL PRIMARY KEY,
                    bool_col BOOLEAN,
                    smallint_col SMALLINT,
                    int_col INTEGER,
                    bigint_col BIGINT,
                    real_col REAL,
                    double_col DOUBLE PRECISION,
                    numeric_col NUMERIC(10, 2),
                    char_col CHAR(10),
                    varchar_col VARCHAR(255),
                    text_col TEXT,
                    date_col DATE,
                    time_col TIME,
                    timestamp_col TIMESTAMP,
                    timestamptz_col TIMESTAMPTZ,
                    uuid_col UUID,
                    json_col JSON,
                    jsonb_col JSONB,
                    int_array_col INTEGER[],
                    text_array_col TEXT[]
                );
                "#,
                schema = schema_name,
                table = table_name
            )
            .as_str(),
        )
        .await
        .expect("Failed to create table");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        let type_test = snapshot
            .tables
            .get(&table_name)
            .expect("type_test table not found");

        // Verify various column types
        let columns = &type_test.columns;

        assert_eq!(
            columns.get("bool_col").unwrap().data_type.to_uppercase(),
            "BOOLEAN"
        );
        assert_eq!(
            columns
                .get("smallint_col")
                .unwrap()
                .data_type
                .to_uppercase(),
            "SMALLINT"
        );
        assert_eq!(
            columns.get("int_col").unwrap().data_type.to_uppercase(),
            "INTEGER"
        );
        assert_eq!(
            columns.get("bigint_col").unwrap().data_type.to_uppercase(),
            "BIGINT"
        );
        assert!(
            columns
                .get("numeric_col")
                .unwrap()
                .data_type
                .contains("NUMERIC")
        );
        assert!(
            columns
                .get("varchar_col")
                .unwrap()
                .data_type
                .contains("VARCHAR")
        );
        assert_eq!(
            columns.get("text_col").unwrap().data_type.to_uppercase(),
            "TEXT"
        );
        assert_eq!(
            columns
                .get("timestamp_col")
                .unwrap()
                .data_type
                .to_uppercase(),
            "TIMESTAMP"
        );
        assert_eq!(
            columns
                .get("timestamptz_col")
                .unwrap()
                .data_type
                .to_uppercase(),
            "TIMESTAMPTZ"
        );
        assert_eq!(
            columns.get("uuid_col").unwrap().data_type.to_uppercase(),
            "UUID"
        );
        assert_eq!(
            columns.get("json_col").unwrap().data_type.to_uppercase(),
            "JSON"
        );
        assert_eq!(
            columns.get("jsonb_col").unwrap().data_type.to_uppercase(),
            "JSONB"
        );

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_identity_column() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("identity");
        let table_name = format!("identity_test_{}", &schema_name[9..17]);

        setup_test_schema(&pool, &schema_name).await;

        // Create table with identity column
        pool.execute(
            format!(
                r#"
                CREATE TABLE "{schema}"."{table}" (
                    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                    name VARCHAR(255)
                );
                "#,
                schema = schema_name,
                table = table_name
            )
            .as_str(),
        )
        .await
        .expect("Failed to create table");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        let table = snapshot
            .tables
            .get(&table_name)
            .expect("identity_test table not found");

        let id_col = table.columns.get("id").expect("id column not found");
        assert!(id_col.identity.is_some());
        assert_eq!(id_col.identity.as_deref(), Some("ALWAYS"));

        cleanup_test_schema(&pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_introspect_complex_schema() {
        let pool = create_pool().await;
        let schema_name = unique_schema_name("complex");
        let suffix = &schema_name[8..16];

        setup_test_schema(&pool, &schema_name).await;

        // Create a complex schema with multiple tables and relationships
        pool.execute(
            format!(
                r#"
                -- Users table
                CREATE TABLE "{schema}".users_{s} (
                    id SERIAL PRIMARY KEY,
                    username VARCHAR(50) NOT NULL UNIQUE,
                    email VARCHAR(255) NOT NULL UNIQUE,
                    password_hash VARCHAR(255) NOT NULL,
                    is_active BOOLEAN DEFAULT true,
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                );

                -- Roles table
                CREATE TABLE "{schema}".roles_{s} (
                    id SERIAL PRIMARY KEY,
                    name VARCHAR(50) NOT NULL UNIQUE,
                    description TEXT
                );

                -- User roles (many-to-many)
                CREATE TABLE "{schema}".user_roles_{s} (
                    user_id INTEGER NOT NULL REFERENCES "{schema}".users_{s}(id) ON DELETE CASCADE,
                    role_id INTEGER NOT NULL REFERENCES "{schema}".roles_{s}(id) ON DELETE CASCADE,
                    assigned_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (user_id, role_id)
                );

                -- Posts table
                CREATE TABLE "{schema}".posts_{s} (
                    id SERIAL PRIMARY KEY,
                    user_id INTEGER NOT NULL REFERENCES "{schema}".users_{s}(id) ON DELETE CASCADE,
                    title VARCHAR(255) NOT NULL,
                    content TEXT,
                    is_published BOOLEAN DEFAULT false,
                    published_at TIMESTAMPTZ,
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                );

                -- Comments table
                CREATE TABLE "{schema}".comments_{s} (
                    id SERIAL PRIMARY KEY,
                    post_id INTEGER NOT NULL REFERENCES "{schema}".posts_{s}(id) ON DELETE CASCADE,
                    user_id INTEGER NOT NULL REFERENCES "{schema}".users_{s}(id) ON DELETE CASCADE,
                    parent_id INTEGER REFERENCES "{schema}".comments_{s}(id) ON DELETE CASCADE,
                    content TEXT NOT NULL,
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                );

                -- Indexes
                CREATE INDEX idx_posts_{s}_user_id ON "{schema}".posts_{s}(user_id);
                CREATE INDEX idx_posts_{s}_published ON "{schema}".posts_{s}(is_published) WHERE is_published = true;
                CREATE INDEX idx_comments_{s}_post_id ON "{schema}".comments_{s}(post_id);
                CREATE INDEX idx_comments_{s}_user_id ON "{schema}".comments_{s}(user_id);
                "#,
                schema = schema_name,
                s = suffix
            )
            .as_str(),
        )
        .await
        .expect("Failed to create complex schema");

        let snapshot = introspect_postgres_schema(&pool, &schema_name)
            .await
            .unwrap();

        // Verify all tables were introspected
        assert!(snapshot.tables.contains_key(&format!("users_{}", suffix)));
        assert!(snapshot.tables.contains_key(&format!("roles_{}", suffix)));
        assert!(
            snapshot
                .tables
                .contains_key(&format!("user_roles_{}", suffix))
        );
        assert!(snapshot.tables.contains_key(&format!("posts_{}", suffix)));
        assert!(
            snapshot
                .tables
                .contains_key(&format!("comments_{}", suffix))
        );

        // Check user_roles composite primary key
        let user_roles = snapshot
            .tables
            .get(&format!("user_roles_{}", suffix))
            .unwrap();
        let pk = user_roles
            .constraints
            .iter()
            .find(|c| c.constraint_type == ConstraintType::PrimaryKey)
            .expect("PK not found");
        assert_eq!(pk.columns.len(), 2);
        assert!(pk.columns.contains(&"user_id".to_string()));
        assert!(pk.columns.contains(&"role_id".to_string()));

        // Check self-referential foreign key in comments
        let comments = snapshot
            .tables
            .get(&format!("comments_{}", suffix))
            .unwrap();
        let self_fk = comments
            .constraints
            .iter()
            .find(|c| {
                c.constraint_type == ConstraintType::ForeignKey
                    && c.columns.contains(&"parent_id".to_string())
            })
            .expect("Self-referential FK not found");
        assert_eq!(
            self_fk.references.as_ref().unwrap().table,
            format!("comments_{}", suffix)
        );

        // Check indexes exist
        let posts = snapshot.tables.get(&format!("posts_{}", suffix)).unwrap();
        assert!(
            posts
                .indexes
                .contains_key(&format!("idx_posts_{}_user_id", suffix))
        );

        cleanup_test_schema(&pool, &schema_name).await;
    }
}

// ============================================================================
// Migration Tests
// ============================================================================

mod migrations {

    use super::*;

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_apply_simple() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("migrate_simple");
        let table_name = format!("test_table_{}", &schema_name[15..23]);
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        // Create a migration manager
        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Ensure migrations table exists
        manager.ensure_migrations_table(&pool).await.unwrap();

        // Create a migration file manually
        let migration_sql = format!(
            r#"
            CREATE TABLE "{schema}"."{table}" (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL
            );
            "#,
            schema = schema_name,
            table = table_name
        );

        std::fs::write(
            temp_dir.path().join("0001_create_test_table.sql"),
            &migration_sql,
        )
        .unwrap();

        // Apply the migration
        manager
            .apply_migration(&pool, &temp_dir.path().join("0001_create_test_table.sql"))
            .await
            .unwrap();

        // Verify the table was created
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(snapshot.tables.contains_key(&table_name));

        // Verify migration was recorded
        let applied = manager.get_applied_migrations(&pool).await.unwrap();
        assert!(
            applied
                .iter()
                .map(|m| m.name.clone())
                .collect::<Vec<String>>()
                .contains(&"0001_create_test_table".to_string())
        );

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_apply_all_pending() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("migrate_all");
        let suffix = &schema_name[12..20];
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create multiple migration files
        let migration1 = format!(
            "CREATE TABLE \"{schema_name}\".users_{suffix} (id SERIAL PRIMARY KEY, name VARCHAR(255));",
        );
        let migration2 = format!(
            "CREATE TABLE \"{schema_name}\".posts_{suffix} (id SERIAL PRIMARY KEY, title VARCHAR(255), user_id INTEGER REFERENCES \"{schema_name}\".users_{suffix}(id));",
        );
        let migration3 = format!(
            "CREATE INDEX idx_posts_{suffix}_user_id ON \"{schema_name}\".posts_{suffix}(user_id);",
        );

        std::fs::write(temp_dir.path().join("0001_create_users.sql"), &migration1).unwrap();
        std::fs::write(temp_dir.path().join("0002_create_posts.sql"), &migration2).unwrap();
        std::fs::write(temp_dir.path().join("0003_add_index.sql"), &migration3).unwrap();

        // Apply all migrations
        let applied = manager.apply_all(&pool).await.unwrap();

        assert_eq!(applied.len(), 3);
        assert!(applied.contains(&"0001_create_users".to_string()));
        assert!(applied.contains(&"0002_create_posts".to_string()));
        assert!(applied.contains(&"0003_add_index".to_string()));

        // Verify tables exist
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(snapshot.tables.contains_key(&format!("users_{}", suffix)));
        assert!(snapshot.tables.contains_key(&format!("posts_{}", suffix)));

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_rollback_single() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("rollback_single");
        let table_name = format!("rollback_test_{}", &schema_name[16..24]);
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Ensure migrations table exists first
        manager.ensure_migrations_table(&pool).await.unwrap();

        // Create up and down migrations
        let up_sql = format!(
            "CREATE TABLE \"{schema}\".\"{table}\" (id SERIAL PRIMARY KEY, name VARCHAR(255));",
            schema = schema_name,
            table = table_name
        );
        let down_sql = format!(
            "DROP TABLE \"{schema}\".\"{table}\";",
            schema = schema_name,
            table = table_name
        );

        std::fs::write(temp_dir.path().join("0001_create_table.sql"), &up_sql).unwrap();
        std::fs::write(
            temp_dir.path().join("0001_create_table.down.sql"),
            &down_sql,
        )
        .unwrap();

        // Apply the migration
        manager
            .apply_migration(&pool, &temp_dir.path().join("0001_create_table.sql"))
            .await
            .unwrap();

        // Verify table exists
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(snapshot.tables.contains_key(&table_name));

        // Rollback the migration
        manager
            .rollback_migration(&pool, &temp_dir.path().join("0001_create_table.down.sql"))
            .await
            .unwrap();

        // Verify table was dropped
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(!snapshot.tables.contains_key(&table_name));

        // Verify migration record was removed
        let applied = manager.get_applied_migrations(&pool).await.unwrap();
        assert!(
            !applied
                .iter()
                .map(|m| m.name.clone())
                .collect::<Vec<String>>()
                .contains(&"0001_create_table".to_string())
        );

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_rollback_all() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("rollback_all");
        let suffix = &schema_name[12..20];
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create multiple migrations with down files
        std::fs::write(
            temp_dir.path().join("0001_create_users.sql"),
            format!(
                "CREATE TABLE \"{schema}\".users_{s} (id SERIAL PRIMARY KEY);",
                schema = schema_name,
                s = suffix
            ),
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("0001_create_users.down.sql"),
            format!(
                "DROP TABLE \"{schema}\".users_{s};",
                schema = schema_name,
                s = suffix
            ),
        )
        .unwrap();

        std::fs::write(
            temp_dir.path().join("0002_create_posts.sql"),
            format!(
                "CREATE TABLE \"{schema}\".posts_{s} (id SERIAL PRIMARY KEY);",
                schema = schema_name,
                s = suffix
            ),
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("0002_create_posts.down.sql"),
            format!(
                "DROP TABLE \"{schema}\".posts_{s};",
                schema = schema_name,
                s = suffix
            ),
        )
        .unwrap();

        // Apply all migrations
        manager.apply_all(&pool).await.unwrap();

        // Verify both tables exist
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(snapshot.tables.contains_key(&format!("users_{}", suffix)));
        assert!(snapshot.tables.contains_key(&format!("posts_{}", suffix)));

        // Rollback all migrations
        let rolled_back = manager.rollback_all(&pool).await.unwrap();

        // Should rollback in reverse order
        assert_eq!(rolled_back.len(), 2);
        assert_eq!(rolled_back[0], "0002_create_posts");
        assert_eq!(rolled_back[1], "0001_create_users");

        // Verify tables were dropped
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(!snapshot.tables.contains_key(&format!("users_{}", suffix)));
        assert!(!snapshot.tables.contains_key(&format!("posts_{}", suffix)));

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_rollback_count() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("rollback_count");
        let suffix = &schema_name[15..23];
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create 3 migrations with unique table names for this test
        for i in 1..=3 {
            std::fs::write(
                temp_dir
                    .path()
                    .join(format!("000{}_create_tbl{}.sql", i, i)),
                format!(
                    "CREATE TABLE \"{schema}\".tbl{i}_{s} (id SERIAL PRIMARY KEY);",
                    schema = schema_name,
                    i = i,
                    s = suffix
                ),
            )
            .unwrap();
            std::fs::write(
                temp_dir
                    .path()
                    .join(format!("000{}_create_tbl{}.down.sql", i, i)),
                format!(
                    "DROP TABLE \"{schema}\".tbl{i}_{s};",
                    schema = schema_name,
                    i = i,
                    s = suffix
                ),
            )
            .unwrap();
        }

        // Apply all
        manager.apply_all(&pool).await.unwrap();

        // Rollback only 2
        let rolled_back = manager.rollback_count(&pool, 2).await.unwrap();

        assert_eq!(rolled_back.len(), 2);

        // Only tbl1 should remain
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(snapshot.tables.contains_key(&format!("tbl1_{}", suffix)));
        assert!(!snapshot.tables.contains_key(&format!("tbl2_{}", suffix)));
        assert!(!snapshot.tables.contains_key(&format!("tbl3_{}", suffix)));

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_pending_detection() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("pending");
        let suffix = &schema_name[8..16];
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create 3 migrations
        for i in 1..=3 {
            std::fs::write(
                temp_dir
                    .path()
                    .join(format!("000{}_migration_{}.sql", i, i)),
                format!(
                    "CREATE TABLE \"{schema}\".pending_t{i}_{s} (id SERIAL PRIMARY KEY);",
                    schema = schema_name,
                    i = i,
                    s = suffix
                ),
            )
            .unwrap();
        }

        // Check pending before any applied
        let pending = manager.get_pending_migrations(&pool).await.unwrap();
        assert_eq!(pending.len(), 3);

        // Apply first migration
        manager
            .apply_migration(&pool, &temp_dir.path().join("0001_migration_1.sql"))
            .await
            .unwrap();

        // Check pending after one applied
        let pending = manager.get_pending_migrations(&pool).await.unwrap();
        assert_eq!(pending.len(), 2);

        // Apply all remaining
        manager.apply_all(&pool).await.unwrap();

        // Check pending after all applied
        let pending = manager.get_pending_migrations(&pool).await.unwrap();
        assert_eq!(pending.len(), 0);

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_transaction_rollback_on_error() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("tx_rollback");
        let table_name = format!("good_table_{}", &schema_name[12..20]);
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create a migration that will partially fail (duplicate table creation)
        let bad_migration = format!(
            r#"
            CREATE TABLE "{schema}"."{table}" (id SERIAL PRIMARY KEY);
            CREATE TABLE "{schema}"."{table}" (id SERIAL PRIMARY KEY);
            "#,
            schema = schema_name,
            table = table_name
        );

        std::fs::write(
            temp_dir.path().join("0001_bad_migration.sql"),
            &bad_migration,
        )
        .unwrap();

        // Apply should fail
        let result = manager
            .apply_migration(&pool, &temp_dir.path().join("0001_bad_migration.sql"))
            .await;

        assert!(result.is_err());

        // The first table should NOT exist due to transaction rollback
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(!snapshot.tables.contains_key(&table_name));

        // Migration should not be recorded
        let applied = manager.get_applied_migrations(&pool).await.unwrap();
        assert!(
            !applied
                .iter()
                .map(|m| m.name.clone())
                .collect::<Vec<String>>()
                .contains(&"0001_bad_migration".to_string())
        );

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_alter_table() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("alter");
        let table_name = format!("users_{}", &schema_name[6..14]);
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create initial table
        std::fs::write(
            temp_dir.path().join("0001_create_users.sql"),
            format!(
                "CREATE TABLE \"{schema}\".\"{table}\" (id SERIAL PRIMARY KEY, name VARCHAR(255));",
                schema = schema_name,
                table = table_name
            ),
        )
        .unwrap();

        // Add column migration
        std::fs::write(
            temp_dir.path().join("0002_add_email.sql"),
            format!(
                "ALTER TABLE \"{schema}\".\"{table}\" ADD COLUMN email VARCHAR(255);",
                schema = schema_name,
                table = table_name
            ),
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("0002_add_email.down.sql"),
            format!(
                "ALTER TABLE \"{schema}\".\"{table}\" DROP COLUMN email;",
                schema = schema_name,
                table = table_name
            ),
        )
        .unwrap();

        // Add constraint migration
        std::fs::write(
            temp_dir.path().join("0003_add_unique.sql"),
            format!(
                "ALTER TABLE \"{schema}\".\"{table}\" ADD CONSTRAINT {table}_email_unique UNIQUE (email);",
                schema = schema_name,
                table = table_name
            ),
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("0003_add_unique.down.sql"),
            format!(
                "ALTER TABLE \"{schema}\".\"{table}\" DROP CONSTRAINT {table}_email_unique;",
                schema = schema_name,
                table = table_name
            ),
        )
        .unwrap();

        // Apply all
        manager.apply_all(&pool).await.unwrap();

        // Verify the schema
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .expect("Failed to introspect");

        let users = snapshot
            .tables
            .get(&table_name)
            .expect("Users table not found");

        assert!(users.columns.contains_key("email"));
        assert!(
            users
                .constraints
                .iter()
                .any(|c| c.constraint_type == ConstraintType::Unique
                    && c.columns.contains(&"email".to_string()))
        );

        // Rollback the unique constraint
        manager.rollback_count(&pool, 1).await.unwrap();

        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        let users = snapshot.tables.get(&table_name).unwrap();

        // Unique constraint should be gone but email column remains
        assert!(users.columns.contains_key("email"));
        assert!(
            !users
                .constraints
                .iter()
                .any(|c| c.name.as_deref() == Some(&format!("{}_email_unique", table_name)))
        );

        // Rollback the email column
        manager.rollback_count(&pool, 1).await.unwrap();

        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        let users = snapshot.tables.get(&table_name).unwrap();

        // Email column should be gone
        assert!(!users.columns.contains_key("email"));

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }

    #[tokio::test]
    #[ignore] // Requires running PostgreSQL
    async fn test_migration_with_enum_type() {
        let pool = create_any_pool().await;
        let pg_pool = create_pool().await;
        let schema_name = unique_schema_name("enum_migration");
        let suffix = &schema_name[15..23];
        let temp_dir = TempDir::new().unwrap();

        setup_test_schema(&pg_pool, &schema_name).await;

        let manager = MigrationManager::new(temp_dir.path(), SchemaDialect::Postgres)
            .with_table_schema(&schema_name);

        // Create enum and table
        std::fs::write(
            temp_dir.path().join("0001_create_enum.sql"),
            format!(
                r#"
                CREATE TYPE "{schema}".order_status_{s} AS ENUM ('pending', 'processing', 'shipped', 'delivered');
                CREATE TABLE "{schema}".orders_{s} (
                    id SERIAL PRIMARY KEY,
                    status "{schema}".order_status_{s} NOT NULL DEFAULT 'pending'
                );
                "#,
                schema = schema_name,
                s = suffix
            ),
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("0001_create_enum.down.sql"),
            format!(
                r#"
                DROP TABLE "{schema}".orders_{s};
                DROP TYPE "{schema}".order_status_{s};
                "#,
                schema = schema_name,
                s = suffix
            ),
        )
        .unwrap();

        // Apply migration
        manager.apply_all(&pool).await.unwrap();

        // Verify enum and table exist
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(
            snapshot
                .enums
                .contains_key(&format!("order_status_{}", suffix))
        );
        assert!(snapshot.tables.contains_key(&format!("orders_{}", suffix)));

        // Rollback
        manager.rollback_all(&pool).await.unwrap();

        // Verify both are gone
        let snapshot = introspect_postgres_schema(&pg_pool, &schema_name)
            .await
            .unwrap();
        assert!(
            !snapshot
                .enums
                .contains_key(&format!("order_status_{}", suffix))
        );
        assert!(!snapshot.tables.contains_key(&format!("orders_{}", suffix)));

        cleanup_test_schema(&pg_pool, &schema_name).await;
    }
}
