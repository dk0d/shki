use crate::models::table_id::TableId;
use crate::schema::SqlDialect;
use crate::sql::generator::SqlGenerator;
use crate::sql::utils::qualified_table_name;

pub fn delete_table(dialect: &SqlDialect, table: &TableId) -> String {
    let table_name = qualified_table_name(dialect, table);

    match dialect {
        SqlDialect::Postgres | SqlDialect::Sqlite => {
            format!("DELETE FROM {} WHERE name = $1", table_name)
        }
        SqlDialect::Mysql => {
            format!("DELETE FROM {} WHERE name = ?", table_name)
        }
    }
}

pub fn ensure_migrations(dialect: &SqlDialect, table: &TableId) -> String {
    let generator = SqlGenerator::new(dialect);
    let table_name = generator.qualified_table_name(table);

    // use text for `applied_at` for simplicity across dialects
    // allows us to use AnyPool more easily
    // checksum is VARCHAR(64) for SHA-256 hex representation
    match dialect {
        SqlDialect::Postgres => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id BIGSERIAL PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
                    checksum VARCHAR(64),
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
        SqlDialect::Mysql => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL UNIQUE,
                    checksum VARCHAR(64),
                    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
        SqlDialect::Sqlite => format!(
            r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    checksum TEXT,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            table_name
        ),
    }
}

pub fn select_migrations(dialect: &SqlDialect, table: &TableId) -> String {
    let table_name = qualified_table_name(dialect, table);
    match dialect {
        SqlDialect::Postgres => format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        ),
        SqlDialect::Mysql => format!(
            "SELECT id, name, checksum, CAST(applied_at AS CHAR) AS applied_at from {} ORDER BY id",
            table_name
        ),

        SqlDialect::Sqlite => format!(
            "SELECT id, name, checksum, applied_at from {} ORDER BY id",
            table_name
        ),
    }
}

pub fn insert_migration(dialect: &SqlDialect, table: &TableId) -> String {
    let table_name = qualified_table_name(dialect, table);
    match dialect {
        SqlDialect::Postgres => {
            format!(
                "INSERT INTO {} (name, checksum) VALUES ($1, $2)",
                table_name
            )
        }
        SqlDialect::Mysql => {
            format!("INSERT INTO {} (name, checksum) VALUES (?, ?)", table_name)
        }
        SqlDialect::Sqlite => {
            format!("INSERT INTO {} (name, checksum) VALUES (?, ?)", table_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> TableId {
        TableId::new("__shki_migrations", Some("meta".to_string()))
    }

    #[test]
    fn uses_expected_placeholders_per_dialect() {
        let table = table();

        assert!(delete_table(&SqlDialect::Postgres, &table).contains("WHERE name = $1"));
        assert!(insert_migration(&SqlDialect::Postgres, &table).contains("VALUES ($1, $2)"));

        assert!(delete_table(&SqlDialect::Sqlite, &table).contains("WHERE name = $1"));
        assert!(insert_migration(&SqlDialect::Sqlite, &table).contains("VALUES (?, ?)"));

        assert!(delete_table(&SqlDialect::Mysql, &table).contains("WHERE name = ?"));
        assert!(insert_migration(&SqlDialect::Mysql, &table).contains("VALUES (?, ?)"));
    }

    #[test]
    fn ensure_migrations_uses_dialect_specific_identifiers() {
        let postgres = ensure_migrations(&SqlDialect::Postgres, &table());
        let mysql = ensure_migrations(&SqlDialect::Mysql, &table());

        assert!(postgres.contains("\"meta\".\"__shki_migrations\""));
        assert!(postgres.contains("id BIGSERIAL PRIMARY KEY"));
        assert!(mysql.contains("`meta`.`__shki_migrations`"));
        assert!(mysql.contains("id INT AUTO_INCREMENT PRIMARY KEY"));
    }
}
