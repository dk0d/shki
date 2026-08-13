-- ```rust
-- pub async fn users_with_optional_post<'e, E>(
--     executor: E,
--     arg1: i32,
-- ) -> sqlx::Result<Vec<UsersWithOptionalPostRow>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     sqlx::query_as::<
--         _,
--         UsersWithOptionalPostRow,
--     >("SELECT\n    u.id AS user_id,\n    u.email,\n    p.id AS post_id,\n    p.title AS post_title\nFROM users u\nLEFT JOIN posts p ON p.user_id = u.id\nWHERE u.id >= $1\nORDER BY u.id, p.id")
--         .bind(arg1)
--         .fetch_all(executor)
--         .await
-- }
-- ```
-- name: users_with_optional_post :many
SELECT
    u.id AS user_id,
    u.email,
    p.id AS post_id,
    p.title AS post_title
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
WHERE u.id >= $1
ORDER BY u.id, p.id;
