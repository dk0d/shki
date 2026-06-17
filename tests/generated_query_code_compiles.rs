//! Fixture test: the code emitted by query codegen must actually compile when
//! placed alongside a schema-codegen `models` module.
//!
//! This exercises the real generators (no hand-written fixture): it builds a
//! `Snapshot` and a set of `DescribedQuery` values in code, renders both the
//! schema model types and the query wrappers, assembles them into a standalone
//! crate, and runs `cargo build` on it. The describe step (which needs a live
//! Shadow Database) is stubbed by constructing `DescribedQuery` directly, so the
//! test stays fast and DB-free while still compiling the genuine generator
//! output against sqlx.
//!
//! Marked `#[ignore]` because it shells out to `cargo build` and compiles sqlx
//! for the fixture crate (slow on first run; a stable target dir is reused
//! afterwards). Run explicitly with:
//!
//! ```bash
//! cargo test --test generated_query_code_compiles -- --ignored
//! ```

use std::path::PathBuf;
use std::process::Command;

use shki::codegen::CodegenConfig;
use shki::codegen::generator::CodeGenerator;
use shki::codegen::lang::rust::RustGenerator;
use shki::codegen::queries::describe::{
    DescribedQuery, ParamBinding, QueryParam, QueryResult, ResultColumn,
};
use shki::codegen::queries::generator::generate_rust_module;
use shki::codegen::queries::parse::{Cardinality, QuerySpec};
use shki::schema::{Column, DataType, DbEnum, SqlDialect, Table};
use shki::snapshots::Snapshot;

/// Build a Snapshot with a `user_status` enum and a `users` table.
fn fixture_snapshot() -> Snapshot {
    let mut snapshot = Snapshot::new(SqlDialect::Postgres);
    let schema = snapshot.catalog.ensure_schema("public");

    schema.enums.insert(
        "user_status".to_string(),
        DbEnum::with_values("user_status", vec!["active", "inactive", "banned"])
            .in_schema("public"),
    );

    let mut users = Table {
        name: "users".to_string(),
        schema: Some("public".to_string()),
        ..Default::default()
    };
    users.columns.insert(
        "id".to_string(),
        Column::new("id", DataType::Integer).primary_key(),
    );
    users.columns.insert(
        "email".to_string(),
        Column::new("email", DataType::Text).not_null(),
    );
    users.columns.insert(
        "status".to_string(),
        Column::new(
            "status",
            DataType::Enum {
                name: "user_status".to_string(),
                schema: Some("public".to_string()),
            },
        )
        .not_null(),
    );
    users
        .columns
        .insert("bio".to_string(), Column::new("bio", DataType::Text));

    schema.tables.insert("users".to_string(), users);
    snapshot
}

/// Build a described query. `sql` is the executable (positional) SQL, which is
/// the only SQL the generator reads.
fn described(
    name: &str,
    cardinality: Cardinality,
    sql: &str,
    params: Vec<QueryParam>,
    result: QueryResult,
) -> DescribedQuery {
    DescribedQuery {
        spec: QuerySpec {
            name: name.to_string(),
            cardinality,
            keyset: Vec::new(),
            sql: sql.to_string(),
            source_file: PathBuf::from("queries/users.sql"),
        },
        sql: sql.to_string(),
        params,
        result,
    }
}

fn cursor_key(key_index: usize, data_type: DataType) -> QueryParam {
    QueryParam {
        data_type,
        binding: ParamBinding::Cursor { key_index },
    }
}

fn arg(data_type: DataType) -> QueryParam {
    QueryParam {
        data_type,
        binding: ParamBinding::Arg(None),
    }
}

fn named_arg(name: &str, data_type: DataType) -> QueryParam {
    QueryParam {
        data_type,
        binding: ParamBinding::Arg(Some(name.to_string())),
    }
}

fn user_status_type() -> DataType {
    DataType::Enum {
        name: "user_status".to_string(),
        schema: Some("public".to_string()),
    }
}

fn reuse_users() -> QueryResult {
    QueryResult::Reuse {
        table_name: "users".to_string(),
    }
}

/// Representative cases: schema-struct reuse, a projected row, an `:exec`, an
/// outer-join row with nullable columns, a named argument, and a paginated
/// `:batch` (named arg + shared `Pagination`).
fn fixture_queries() -> Vec<DescribedQuery> {
    vec![
        described(
            "user_by_id",
            Cardinality::One,
            "SELECT * FROM users WHERE id = $1",
            vec![arg(DataType::Integer)],
            reuse_users(),
        ),
        described(
            "active_user_emails",
            Cardinality::Many,
            "SELECT id, email FROM users WHERE status = $1",
            vec![arg(user_status_type())],
            QueryResult::Row {
                columns: vec![
                    ResultColumn {
                        name: "id".to_string(),
                        data_type: DataType::Integer,
                        nullable: false,
                    },
                    ResultColumn {
                        name: "email".to_string(),
                        data_type: DataType::Text,
                        nullable: false,
                    },
                ],
            },
        ),
        described(
            "deactivate_user",
            Cardinality::Exec,
            "UPDATE users SET status = 'inactive' WHERE id = $1",
            vec![arg(DataType::Integer)],
            QueryResult::Exec,
        ),
        described(
            "post_with_author",
            Cardinality::Many,
            "SELECT u.id, u.email, u.bio FROM users u",
            vec![],
            QueryResult::Row {
                columns: vec![
                    ResultColumn {
                        name: "id".to_string(),
                        data_type: DataType::Integer,
                        nullable: false,
                    },
                    // Made nullable by an outer join even though the base column
                    // is NOT NULL.
                    ResultColumn {
                        name: "author_email".to_string(),
                        data_type: DataType::Text,
                        nullable: true,
                    },
                    ResultColumn {
                        name: "author_bio".to_string(),
                        data_type: DataType::Text,
                        nullable: true,
                    },
                ],
            },
        ),
        // Named argument: `:email` -> `$1`, function takes `email: String`.
        described(
            "user_by_email",
            Cardinality::One,
            "SELECT * FROM users WHERE email = $1",
            vec![named_arg("email", DataType::Text)],
            reuse_users(),
        ),
        // `:batch` limit/offset: a named data arg plus the shared `Pagination`
        // input bound to LIMIT/OFFSET; returns `Page<User>`.
        described(
            "users_by_status_page",
            Cardinality::Batch,
            "SELECT * FROM users WHERE status = $1 ORDER BY id LIMIT $2 OFFSET $3",
            vec![
                named_arg("status", user_status_type()),
                QueryParam {
                    data_type: DataType::BigInt,
                    binding: ParamBinding::PageLimit,
                },
                QueryParam {
                    data_type: DataType::BigInt,
                    binding: ParamBinding::PageOffset,
                },
            ],
            reuse_users(),
        ),
        // `:batch :keyset $1 $2`: a two-column keyset cursor (id, email) bound
        // from `CursorPagination<(i32, String)>`, plus a page-size arg.
        described(
            "users_keyset_page",
            Cardinality::Batch,
            "SELECT * FROM users WHERE (id, email) > ($1, $2) ORDER BY id, email LIMIT $3",
            vec![
                cursor_key(0, DataType::Integer),
                cursor_key(1, DataType::Text),
                arg(DataType::BigInt),
            ],
            reuse_users(),
        ),
    ]
}

/// Render the schema model types (enums first, then structs) as a single module.
fn render_models(snapshot: &Snapshot, config: &CodegenConfig) -> String {
    let generated = RustGenerator::new().generate(snapshot, config);

    let mut out = String::new();
    for rust_enum in generated.enums.values() {
        out.push_str(&rust_enum.to_string_pretty());
        out.push('\n');
    }
    for rust_struct in generated.structs.values() {
        out.push_str(&rust_struct.to_string_pretty());
        out.push('\n');
    }
    out
}

#[test]
#[ignore = "compiles a standalone crate with cargo; run with --ignored"]
fn generated_query_code_compiles() {
    let snapshot = fixture_snapshot();
    let queries = fixture_queries();
    let config = CodegenConfig::default();

    let models = render_models(&snapshot, &config);
    let queries_module = generate_rust_module(&queries, &snapshot, &config, Some("crate::models"));

    let lib_rs = format!(
        "pub mod models {{\n#![allow(dead_code, unused_imports)]\n{models}\n}}\n\n\
         pub mod queries {{\n{queries_module}\n}}\n"
    );

    let cargo_toml = r#"[package]
name = "shki-query-codegen-fixture"
version = "0.0.0"
edition = "2021"

[dependencies]
sqlx = { version = "0.9.0", features = ["runtime-tokio", "postgres", "macros"] }

[workspace]
"#;

    let crate_dir = tempfile::tempdir().expect("create temp crate dir");
    let src_dir = crate_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    std::fs::write(crate_dir.path().join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    std::fs::write(src_dir.join("lib.rs"), &lib_rs).expect("write lib.rs");

    // Reuse a stable target dir so sqlx is only compiled once across test runs.
    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/query_fixture_target");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .args(["build", "--offline", "--quiet"])
        .current_dir(crate_dir.path())
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run cargo build on generated fixture crate");

    if !output.status.success() {
        panic!(
            "generated query code failed to compile.\n\n--- src/lib.rs ---\n{lib_rs}\n\n\
             --- stdout ---\n{}\n--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
