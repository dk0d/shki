-- Schema nullability holds for RETURNING columns, where sqlx's describe-time
-- inference is "unknown": the NOT NULL id stays non-optional, bio stays
-- Option.
-- expect-contains: pub id: i32,
-- expect-contains: pub bio: Option<String>,
-- name: create_user :one
INSERT INTO users (id, email) VALUES ($id, $email) RETURNING id, bio;

-- UNION output columns lose their table origin, so inference cannot prove
-- them; an sqlx-style alias marker (`AS "name!"` / `AS "name?"`) forces it.
-- expect-contains: pub union_id: i32,
-- name: all_user_ids :many
SELECT id AS "union_id!" FROM users UNION ALL SELECT id FROM users;

-- expect-contains: pub maybe_email: Option<String>,
-- name: emails_forced_nullable :many
SELECT email AS "maybe_email?" FROM users;

-- A parameter written whole into a nullable column (INSERT VALUES / UPDATE
-- SET) is inferred nullable without an explicit `?name` marker.
-- expect:
-- pub async fn upsert_bio<'e, E>(
--     executor: E,
--     id: i32,
--     email: String,
--     bio: Option<String>,
-- ) -> sqlx::Result<u64>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     let result = sqlx::query(
--             r#"INSERT INTO users (id, email, bio) VALUES ($1, $2, $3) ON CONFLICT (id) DO UPDATE SET bio = EXCLUDED.bio"#,
--         )
--         .bind(id)
--         .bind(email)
--         .bind(bio)
--         .execute(executor)
--         .await?;
--     Ok(result.rows_affected())
-- }
-- end expect
-- name: upsert_bio :exec
INSERT INTO users (id, email, bio) VALUES ($id, $email, $bio) ON CONFLICT (id) DO UPDATE SET bio = EXCLUDED.bio;

-- Inference covers every nullable column type: enum, jsonb, array,
-- timestamptz, uuid.
-- expect:
-- pub async fn upsert_attribute<'e, E>(
--     executor: E,
--     id: i64,
--     name: String,
--     annotation: Option<String>,
--     tag: Option<Marker>,
--     meta: Option<serde_json::Value>,
--     scores: Option<Vec<i64>>,
--     verified_at: Option<chrono::DateTime<chrono::Utc>>,
--     ref_id: Option<uuid::Uuid>,
-- ) -> sqlx::Result<u64>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     let result = sqlx::query(
--             r#"INSERT INTO attributes (id, name, annotation, tag, meta, scores, verified_at, ref_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (id) DO UPDATE SET annotation = EXCLUDED.annotation"#,
--         )
--         .bind(id)
--         .bind(name)
--         .bind(annotation)
--         .bind(tag)
--         .bind(meta)
--         .bind(scores)
--         .bind(verified_at)
--         .bind(ref_id)
--         .execute(executor)
--         .await?;
--     Ok(result.rows_affected())
-- }
-- end expect
-- name: upsert_attribute :exec
INSERT INTO attributes (id, name, annotation, tag, meta, scores, verified_at, ref_id) VALUES ($id, $name, $annotation, $tag, $meta, $scores, $verified_at, $ref_id) ON CONFLICT (id) DO UPDATE SET annotation = EXCLUDED.annotation;

-- expect:
-- pub async fn set_annotation<'e, E>(
--     executor: E,
--     annotation: Option<String>,
--     id: i64,
-- ) -> sqlx::Result<u64>
-- where
--     E: sqlx::PgExecutor<'e>,
-- {
--     let result = sqlx::query(
--             r#"UPDATE public.attributes SET annotation = $1 WHERE id = $2"#,
--         )
--         .bind(annotation)
--         .bind(id)
--         .execute(executor)
--         .await?;
--     Ok(result.rows_affected())
-- }
-- end expect
-- name: set_annotation :exec
UPDATE public.attributes SET annotation = $annotation WHERE id = $id;
