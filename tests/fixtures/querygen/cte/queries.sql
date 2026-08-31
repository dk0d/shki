-- Both CTE output columns trace to NOT NULL base columns and stay non-optional.
-- expect-contains: pub id: i32,
-- expect-contains: pub email: String,
-- ```rust
-- pub async fn active_user_emails<'e, E>(
--     executor: E,
--     excluded: String,
-- ) -> sqlx::Result<Vec<ActiveUserEmailsRow>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     sqlx::query_as::<
--         _,
--         ActiveUserEmailsRow,
--     >("WITH active_users AS (\n    SELECT id, email\n    FROM users\n    WHERE active = true\n)\nSELECT id, email\nFROM active_users\nWHERE email <> $1")
--         .bind(excluded)
--         .fetch_all(executor)
--         .await
-- }
-- ```
-- name: active_user_emails :many
WITH active_users AS (
    SELECT id, email
    FROM users
    WHERE active = true
)
SELECT id, email
FROM active_users
WHERE email <> $excluded;
