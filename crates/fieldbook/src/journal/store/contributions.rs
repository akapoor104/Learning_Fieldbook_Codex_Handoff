use serde::Serialize;
use serde_json::json;
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::{lock_moment, read, snapshot, Actor, Failure, Moment};
use crate::journal::AddComment;

pub(in crate::journal) async fn set_confirmation(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    expected_revision: i32,
    confirm: bool,
) -> Result<Moment, Failure> {
    let mut tx = pool.begin().await?;
    let moment = lock_moment(&mut tx, actor.household_id, id).await?;
    if actor.role != "learner" || moment.learner_id != actor.id || moment.voice != "scribed" {
        return Err(Failure::Missing);
    }
    let revision = moment.revision;
    if revision != expected_revision {
        let saved = snapshot(&mut *tx, actor.household_id, id, Some(revision)).await?;
        return Err(Failure::Stale {
            saved: json!(saved),
            submitted: json!({ "expected_revision": expected_revision }),
        });
    }
    if confirm {
        sqlx::query(
            "INSERT INTO moment_confirmations (moment_id, household_id, revision, learner_id)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (moment_id, revision)
             DO UPDATE SET withdrawn_at = NULL, confirmed_at = now()
             WHERE moment_confirmations.withdrawn_at IS NOT NULL",
        )
        .bind(id)
        .bind(actor.household_id)
        .bind(revision)
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    } else {
        withdraw_confirmation(&mut tx, id, revision).await?;
    }
    let current = snapshot(&mut *tx, actor.household_id, id, Some(revision)).await?;
    tx.commit().await?;
    Ok(current)
}

pub(super) async fn withdraw_confirmation(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    revision: i32,
) -> Result<(), Failure> {
    sqlx::query(
        "UPDATE moment_confirmations SET withdrawn_at = now()
         WHERE moment_id = $1 AND revision = $2 AND withdrawn_at IS NULL",
    )
    .bind(id)
    .bind(revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Serialize, sqlx::FromRow)]
pub(in crate::journal) struct Comment {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub body: String,
    pub kind: String,
    pub created_at: String,
    pub withdrawn: bool,
}

const COMMENTS: &str = r#"
    SELECT c.id, c.author_id, a.display_name AS author_name, c.body, c.kind,
           to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at,
           c.withdrawn_at IS NOT NULL AS withdrawn
    FROM moment_comments c
    JOIN actors a ON a.id = c.author_id
    JOIN moments m ON m.id = c.moment_id AND m.household_id = c.household_id
    WHERE c.household_id = $1 AND c.moment_id = $2 AND m.visibility = 'family'
      AND (c.withdrawn_at IS NULL OR c.author_id = $3)
"#;

pub(in crate::journal) async fn list_comments(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<Vec<Comment>, Failure> {
    read(pool, actor, id).await?;
    Ok(
        sqlx::query_as(&format!("{COMMENTS} ORDER BY c.created_at, c.id"))
            .bind(actor.household_id)
            .bind(id)
            .bind(actor.id)
            .fetch_all(pool)
            .await?,
    )
}

pub(in crate::journal) async fn comment<'e>(
    executor: impl PgExecutor<'e>,
    actor: &Actor,
    id: Uuid,
    comment: Uuid,
) -> Result<Comment, Failure> {
    sqlx::query_as(&format!("{COMMENTS} AND c.id = $4"))
        .bind(actor.household_id)
        .bind(id)
        .bind(actor.id)
        .bind(comment)
        .fetch_optional(executor)
        .await?
        .ok_or(Failure::Missing)
}

pub(in crate::journal) async fn add_comment(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    input: &AddComment,
) -> Result<Comment, Failure> {
    let mut tx = pool.begin().await?;
    lock_moment(&mut tx, actor.household_id, id).await?;
    if input.body.trim().is_empty() || input.body.len() > 4000 || input.body.contains('\0') {
        return Err(Failure::Validation("invalid_comment"));
    }
    if !matches!(input.kind.as_str(), "comment" | "suggestion") {
        return Err(Failure::Validation("invalid_comment_kind"));
    }
    let inserted = sqlx::query_scalar(
        "INSERT INTO moment_comments (household_id, moment_id, author_id, submission_key, kind, body)
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (author_id, submission_key) DO NOTHING
         RETURNING id",
    )
    .bind(actor.household_id)
    .bind(id)
    .bind(actor.id)
    .bind(input.submission_key)
    .bind(&input.kind)
    .bind(&input.body)
    .fetch_optional(&mut *tx)
    .await?;
    let comment_id = match inserted {
        Some(id) => id,
        None => {
            let (prior_id, prior_moment, kind, body): (Uuid, Uuid, String, String) =
                sqlx::query_as(
                    "SELECT id, moment_id, kind, body FROM moment_comments
                 WHERE household_id = $1 AND author_id = $2 AND submission_key = $3",
                )
                .bind(actor.household_id)
                .bind(actor.id)
                .bind(input.submission_key)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Failure::Missing)?;
            if prior_moment != id || kind != input.kind || body != input.body {
                return Err(Failure::Conflict);
            }
            prior_id
        }
    };
    let saved = comment(&mut *tx, actor, id, comment_id).await?;
    tx.commit().await?;
    Ok(saved)
}

pub(in crate::journal) async fn set_withdrawal(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    comment_id: Uuid,
    withdraw: bool,
) -> Result<Comment, Failure> {
    let mut tx = pool.begin().await?;
    lock_moment(&mut tx, actor.household_id, id).await?;
    let result = sqlx::query(
        "UPDATE moment_comments SET withdrawn_at = CASE WHEN $1 THEN coalesce(withdrawn_at, now()) ELSE NULL END
         WHERE id = $2 AND household_id = $3 AND moment_id = $4 AND author_id = $5",
    )
    .bind(withdraw)
    .bind(comment_id)
    .bind(actor.household_id)
    .bind(id)
    .bind(actor.id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(Failure::Missing);
    }
    let updated = comment(&mut *tx, actor, id, comment_id).await?;
    tx.commit().await?;
    Ok(updated)
}
