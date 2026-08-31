#![cfg(feature = "querygen")]

//! File-backed Querygen fixture. Add a directory under `tests/fixtures/querygen/`
//! with `schema.sql` and `queries.sql`. Annotate each query, directly above its
//! `-- name:` marker, with an `-- expect:` block declaring its exact generated
//! function and/or `-- expect-contains: <text>` lines for fragments (e.g. row
//! struct fields); this test generates each fixture into a standalone crate and
//! verifies those contracts.

use std::path::{Path, PathBuf};
use std::process::Command;

use shki::codegen::queries::{QueriesConfig, cmd_query_codegen};
use shki::codegen::{CodegenConfig, cmd_codegen};
use shki::config::Config;
use shki::schema::SqlDialect;
use shki::{CommonArgs, ShadowArgs};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/querygen");

fn fixture_dirs() -> Vec<PathBuf> {
    let mut fixtures: Vec<_> = std::fs::read_dir(FIXTURES)
        .expect("read Querygen fixtures")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    fixtures.sort();
    fixtures
}

fn copy_fixture(fixture: &Path, root: &Path, name: &str) {
    std::fs::copy(fixture.join(name), root.join(name)).expect("copy Querygen fixture input");
}

fn expected_functions(queries: &str) -> Vec<(String, String)> {
    let mut expected = Vec::new();
    let mut lines = queries.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "-- expect:" {
            continue;
        }
        let mut body = String::new();
        for line in lines.by_ref() {
            if line.trim() == "-- end expect" {
                break;
            }
            body.push_str(
                line.strip_prefix("-- ")
                    .expect("expected line must start with '-- '"),
            );
            body.push('\n');
        }
        let name = body
            .split_whitespace()
            .nth(3)
            .and_then(|name| name.split('<').next())
            .expect("expected function must start with `pub async fn <name>`")
            .to_string();
        expected.push((name, body));
    }
    expected
}

fn generated_function(generated: &str, name: &str) -> String {
    let start = generated
        .find(&format!("pub async fn {name}"))
        .expect("expected generated function");
    let body_start = generated[start..]
        .find('{')
        .expect("generated function body")
        + start;
    let mut depth = 0;
    for (offset, character) in generated[body_start..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return generated[start..=body_start + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unterminated generated function {name}");
}

fn assert_expected(generated: &str, fixture: &Path) {
    let queries = std::fs::read_to_string(fixture.join("queries.sql"))
        .expect("read Querygen fixture queries");
    // `-- expect-contains: <text>` asserts the generated module contains <text>
    // verbatim (e.g. a row struct field, which `-- expect:` cannot cover).
    for line in queries.lines() {
        if let Some(expected) = line.trim().strip_prefix("-- expect-contains: ") {
            assert!(
                generated.contains(expected),
                "{} generated output missing `{expected}`:\n{generated}",
                fixture.display(),
            );
        }
    }
    for (name, expected) in expected_functions(&queries) {
        let actual = generated_function(generated, &name);
        assert!(
            actual == expected.trim_end(),
            "{} generated `{name}` differently:\n--- expected ---\n{}\n--- actual ---\n{}",
            fixture.display(),
            expected.trim_end(),
            actual,
        );
    }
}

async fn generate_fixture(fixture: &Path) -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().expect("create fixture crate");
    copy_fixture(fixture, root.path(), "schema.sql");
    std::fs::create_dir(root.path().join("queries")).expect("create fixture query dir");
    copy_fixture(fixture, &root.path().join("queries"), "queries.sql");

    let models = root.path().join("src");
    let queries = root.path().join("src/queries.rs");
    let config = Config {
        root: root.path().to_path_buf(),
        common: CommonArgs {
            dialect: Some(SqlDialect::Postgres),
            ..Default::default()
        },
        schema: "schema.sql".into(),
        codegen: CodegenConfig {
            output: Some(models),
            ..Default::default()
        },
        queries: QueriesConfig {
            output: Some(queries.clone()),
            models: Some("crate::models".to_string()),
            ..Default::default()
        },
        shadow: ShadowArgs::default(),
        ..Default::default()
    };

    std::fs::create_dir(root.path().join("src")).expect("create fixture source dir");
    cmd_codegen(&config, None, shki::CodegenLanguage::Rust, false)
        .await
        .expect("generate fixture models");
    cmd_query_codegen(&config, false)
        .await
        .expect("generate fixture queries");

    let generated = std::fs::read_to_string(queries).expect("read generated queries");
    (root, generated)
}

#[tokio::test]
async fn generated_query_code_is_valid_rust() {
    let mut fixtures = tokio::task::JoinSet::new();
    for fixture in fixture_dirs() {
        fixtures.spawn(async move {
            let (_, generated) = generate_fixture(&fixture).await;
            assert_expected(&generated, &fixture);
            syn::parse_file(&generated).unwrap_or_else(|error| {
                panic!(
                    "{} generated invalid Rust: {error}\n\n--- generated ---\n{generated}",
                    fixture.display()
                )
            });
        });
    }
    while let Some(result) = fixtures.join_next().await {
        result.expect("Querygen fixture task failed");
    }
}

#[tokio::test]
#[ignore = "builds standalone Querygen fixture crates; run with --ignored"]
async fn generated_query_code_compiles() {
    let mut fixtures = tokio::task::JoinSet::new();
    for fixture in fixture_dirs() {
        fixtures.spawn(async move {
            let (crate_dir, generated) = generate_fixture(&fixture).await;
            assert_expected(&generated, &fixture);
            std::fs::write(
                crate_dir.path().join("src/lib.rs"),
                "pub mod models;\npub mod queries;\n",
            )
            .expect("write fixture lib");
            std::fs::write(
                crate_dir.path().join("Cargo.toml"),
                "[package]\nname = \"shki-query-codegen-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nsqlx = { version = \"0.9.0\", features = [\"runtime-tokio\", \"postgres\", \"macros\"] }\n\n[workspace]\n",
            )
            .expect("write fixture manifest");

            let target_dir =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/query_fixture_target");
            let output = Command::new(
                std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()),
            )
            .args(["build", "--offline", "--quiet"])
            .current_dir(crate_dir.path())
            .env("CARGO_TARGET_DIR", target_dir)
            .output()
            .expect("build Querygen fixture");
            assert!(
                output.status.success(),
                "{} failed to compile:\n{}",
                fixture.display(),
                String::from_utf8_lossy(&output.stderr),
            );
        });
    }
    while let Some(result) = fixtures.join_next().await {
        result.expect("Querygen fixture task failed");
    }
}
