//! Compile and execute the generated Serde adapters against real consumer dependencies.
use std::process::Command;

use indexmap::IndexMap;
use shki::codegen::lang::rust::{RustField, RustGenerator, RustStruct};
use shki::codegen::{CodegenConfig, DateTimeCrate};
use shki::schema::DataType;

#[test]
#[ignore = "builds a standalone Jiff consumer crate; run with --ignored"]
fn jiff_sqlx_models_compile_and_round_trip_serde() {
    let config = CodegenConfig {
        date_time_crate: Some(DateTimeCrate::JiffSqlx),
        serde: Some(true),
        ..Default::default()
    };
    let types = [
        ("date", DataType::Date, "2026-09-22"),
        (
            "time",
            DataType::Time {
                precision: None,
                with_timezone: false,
            },
            "12:34:56",
        ),
        (
            "datetime",
            DataType::Timestamp {
                precision: None,
                with_timezone: false,
            },
            "2026-09-22T12:34:56",
        ),
        (
            "timestamp",
            DataType::Timestamp {
                precision: None,
                with_timezone: true,
            },
            "2026-09-22T12:34:56Z",
        ),
    ];
    let mut fields = Vec::new();
    let mut json = serde_json::Map::new();
    for (name, ty, value) in types {
        for (suffix, ty, nullable, value) in [
            ("", ty.clone(), false, serde_json::json!(value)),
            ("_optional", ty.clone(), true, serde_json::json!(value)),
            (
                "_array",
                DataType::Array {
                    element_type: Box::new(ty.clone()),
                },
                false,
                serde_json::json!([value]),
            ),
            (
                "_optional_array",
                DataType::Array {
                    element_type: Box::new(ty),
                },
                true,
                serde_json::json!([value]),
            ),
        ] {
            let name = format!("{name}{suffix}");
            json.insert(name.clone(), value);
            fields.push(RustField {
                name: name.clone(),
                db_name: name,
                rust_type: RustGenerator.sql_type_to_rust(
                    &ty,
                    nullable,
                    &IndexMap::new(),
                    &IndexMap::new(),
                    &config,
                ),
                nullable,
                primary_key: false,
                unique: false,
                comment: None,
            });
        }
    }
    let model = RustStruct {
        name: "Event".into(),
        table_name: "events".into(),
        fields,
        derives: config.struct_derives.clone(),
        attributes: vec![],
        serde: true,
        sqlx: true,
        comment: None,
    };
    let generated = model.to_string_pretty();
    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        r#"
[package]
name = "shki-jiff-fixture"
version = "0.0.0"
edition = "2024"

[[bin]]
name = "fixture"
path = "main.rs"

[dependencies]
jiff = { version = "0.2", features = ["serde"] }
jiff-sqlx = { version = "0.2", features = ["postgres"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_with = { version = "3", default-features = false, features = ["std"] }
sqlx = { version = "0.9", default-features = false, features = ["postgres", "derive"] }

[workspace]
"#,
    )
    .unwrap();
    let json = serde_json::Value::Object(json).to_string();
    let checks = r#"
fn assert_from_row<T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>>() {}
fn assert_sqlx<T: sqlx::Type<sqlx::Postgres> + for<'r> sqlx::Decode<'r, sqlx::Postgres> + for<'q> sqlx::Encode<'q, sqlx::Postgres>>() {}
fn main() {
    assert_from_row::<Event>();
    assert_sqlx::<jiff_sqlx::Date>();
    assert_sqlx::<jiff_sqlx::Time>();
    assert_sqlx::<jiff_sqlx::DateTime>();
    assert_sqlx::<jiff_sqlx::Timestamp>();
    let original: serde_json::Value = serde_json::from_str(JSON).unwrap();
    let event: Event = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(event).unwrap(), original);
    let mut nulls = original.clone();
    for (name, value) in nulls.as_object_mut().unwrap() {
        if name.contains("_optional") { *value = serde_json::Value::Null; }
    }
    let event: Event = serde_json::from_value(nulls.clone()).unwrap();
    assert_eq!(serde_json::to_value(event).unwrap(), nulls);
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().retain(|name, _| !name.contains("_optional"));
    let event: Event = serde_json::from_value(missing).unwrap();
    assert_eq!(serde_json::to_value(event).unwrap(), nulls);
    let mut invalid = original;
    invalid["date"] = serde_json::json!("not a date");
    assert!(serde_json::from_value::<Event>(invalid).is_err());
}
"#;
    std::fs::write(
        fixture.path().join("main.rs"),
        format!("{generated}\nconst JSON: &str = {json:?};\n{checks}"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(fixture.path())
        .env(
            "CARGO_TARGET_DIR",
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/jiff_fixture_target"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
