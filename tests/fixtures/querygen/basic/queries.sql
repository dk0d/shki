-- ```rust
-- pub async fn user_by_id<'e, E>(executor: E, arg1: i32) -> sqlx::Result<Option<User>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
--         .bind(arg1)
--         .fetch_optional(executor)
--         .await
-- }
-- ```
-- name: user_by_id :one
SELECT * FROM users WHERE id = $1;

-- ```rust
-- pub async fn active_user_emails<'e, E>(
--     executor: E,
--     arg1: UserStatus,
-- ) -> sqlx::Result<Vec<ActiveUserEmailsRow>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     sqlx::query_as::<
--         _,
--         ActiveUserEmailsRow,
--     >("SELECT id, email FROM users WHERE status = $1")
--         .bind(arg1)
--         .fetch_all(executor)
--         .await
-- }
-- ```
-- name: active_user_emails :many
SELECT id, email FROM users WHERE status = $1;

-- ```rust
-- pub async fn deactivate_user(
--     transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
--     arg1: i32,
-- ) -> sqlx::Result<u64>
-- {
--     let result = sqlx::query("UPDATE users SET status = 'inactive' WHERE id = $1")
--         .bind(arg1)
--         .execute(&mut **transaction)
--         .await?;
--     Ok(result.rows_affected())
-- }
-- ```
-- name: deactivate_user :exec :tx
UPDATE users SET status = 'inactive' WHERE id = $1;

-- ```rust
-- pub async fn user_by_email<'e, E>(
--     executor: E,
--     email: String,
-- ) -> sqlx::Result<Option<User>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
--         .bind(email)
--         .fetch_optional(executor)
--         .await
-- }
-- ```
-- name: user_by_email :one
SELECT * FROM users WHERE email = $email;

-- ```rust
-- pub async fn users_by_status_page<'e, E>(
--     executor: E,
--     status: UserStatus,
--     page: &Pagination,
-- ) -> sqlx::Result<Page<User>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     let items = sqlx::query_as::<
--         _,
--         User,
--     >("SELECT * FROM users WHERE status = $1 ORDER BY id LIMIT $2 OFFSET $3")
--         .bind(status)
--         .bind(page.limit)
--         .bind(page.offset)
--         .fetch_all(executor)
--         .await?;
--     Ok(Page {
--         items,
--         pagination: Pagination {
--             limit: page.limit,
--             offset: page.offset,
--         },
--     })
-- }
-- ```
-- name: users_by_status_page :batch
SELECT * FROM users WHERE status = $status ORDER BY id LIMIT $limit OFFSET $offset;

-- ```rust
-- pub async fn users_keyset_page<'e, E>(
--     executor: E,
--     limit: i64,
--     cursor: &CursorPagination<(i32, String)>,
-- ) -> sqlx::Result<KeysetPage<User, (i32, String)>>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     let items = sqlx::query_as::<
--         _,
--         User,
--     >("SELECT * FROM users\nWHERE (id, email) > ($1, $2)\nORDER BY id, email\nLIMIT $3")
--         .bind(cursor.key.0.clone())
--         .bind(cursor.key.1.clone())
--         .bind(limit)
--         .fetch_all(executor)
--         .await?;
--     let next = items
--         .last()
--         .map(|row| CursorPagination::new((row.id.clone(), row.email.clone())));
--     Ok(KeysetPage { items, next })
-- }
-- ```
-- name: users_keyset_page :batch :keyset $cursor_id=id $cursor_email=email
SELECT * FROM users
WHERE (id, email) > ($cursor_id, $cursor_email)
ORDER BY id, email
LIMIT $limit;
