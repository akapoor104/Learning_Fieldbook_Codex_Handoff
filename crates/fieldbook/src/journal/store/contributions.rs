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
        sqlx::query(
            "UPDATE relative_shares SET revoked_at = clock_timestamp()
             WHERE moment_id = $1 AND revision = $2 AND include_text AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
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
pub(crate) struct Comment {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub body: String,
    pub kind: String,
    pub created_at: String,
    pub withdrawn: bool,
    pub share_id: Option<Uuid>,
    pub feedback_writer: Option<String>,
    pub feedback_date: Option<String>,
    pub feedback_asset_id: Option<Uuid>,
    pub feedback_uploader_name: Option<String>,
}

const COMMENTS: &str = r#"
    SELECT c.id, c.author_id, a.display_name AS author_name, c.body, c.kind,
           to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at,
           c.withdrawn_at IS NOT NULL AS withdrawn, c.share_id, c.feedback_writer,
           to_char(c.feedback_date, 'YYYY-MM-DD') AS feedback_date, c.feedback_asset_id,
           uploader.display_name AS feedback_uploader_name
    FROM moment_comments c
    JOIN actors a ON a.id = c.author_id
    LEFT JOIN family_assets asset ON asset.id = c.feedback_asset_id AND asset.household_id = c.household_id
    LEFT JOIN actors uploader ON uploader.id = asset.uploader_id
    JOIN family_moments m ON m.id = c.moment_id AND m.household_id = c.household_id
    WHERE c.household_id = $1 AND c.moment_id = $2
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
    let saved = add_scoped_comment(&mut tx, actor, id, None, input).await?;
    tx.commit().await?;
    Ok(saved)
}

pub(crate) async fn scoped_comments(
    executor: impl PgExecutor<'_>,
    actor: &Actor,
    id: Uuid,
    share: Uuid,
) -> Result<Vec<Comment>, Failure> {
    Ok(sqlx::query_as(&format!(
        "{COMMENTS} AND c.share_id = $4 ORDER BY c.created_at, c.id"
    ))
    .bind(actor.household_id)
    .bind(id)
    .bind(actor.id)
    .bind(share)
    .fetch_all(executor)
    .await?)
}

pub(crate) async fn add_scoped_comment(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    share: Option<Uuid>,
    input: &AddComment,
) -> Result<Comment, Failure> {
    let feedback = input.feedback.as_ref();
    let writer = feedback.map(|item| item.writer.trim());
    let date = feedback.and_then(|item| item.date.as_deref());
    let asset = feedback.and_then(|item| item.asset_id);
    if input.body.len() > 4000 || input.body.contains('\0') || input.body.trim().is_empty() {
        return Err(Failure::Validation("invalid_comment"));
    }
    if !matches!(input.kind.as_str(), "comment" | "suggestion") {
        return Err(Failure::Validation("invalid_comment_kind"));
    }
    if (share.is_some() && (input.kind != "comment" || feedback.is_some()))
        || (feedback.is_some() && (input.kind != "comment" || actor.role != "guardian"))
        || writer.is_some_and(|name| name.is_empty() || name.len() > 200 || name.contains('\0'))
        || date.is_some_and(|value| super::activity_date(Some(value)).is_err())
    {
        return Err(Failure::Validation("invalid_feedback"));
    }
    if let Some(asset) = asset {
        let available: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM moment_assets l JOIN family_assets a ON a.id = l.asset_id AND a.household_id = l.household_id
             WHERE l.moment_id = $1 AND l.asset_id = $2 AND l.household_id = $3
               AND l.withdrawn_at IS NULL AND a.source_url IS NULL AND a.media_type = 'application/pdf')",
        )
        .bind(id).bind(asset).bind(actor.household_id).fetch_one(&mut **tx).await?;
        if !available {
            return Err(Failure::Missing);
        }
    }
    let inserted = sqlx::query_scalar(
        "INSERT INTO moment_comments (household_id, moment_id, author_id, submission_key, kind, body, share_id,
                                      feedback_writer, feedback_date, feedback_asset_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::text::date, $10)
         ON CONFLICT (author_id, submission_key) DO NOTHING
         RETURNING id",
    )
    .bind(actor.household_id)
    .bind(id)
    .bind(actor.id)
    .bind(input.submission_key)
    .bind(&input.kind)
    .bind(&input.body)
    .bind(share)
    .bind(writer)
    .bind(date)
    .bind(asset)
    .fetch_optional(&mut **tx)
    .await?;
    let comment_id = match inserted {
        Some(id) => id,
        None => {
            let (prior_id, prior_moment): (Uuid, Uuid) = sqlx::query_as(
                "SELECT id, moment_id FROM moment_comments
                 WHERE household_id = $1 AND author_id = $2 AND submission_key = $3",
            )
            .bind(actor.household_id)
            .bind(actor.id)
            .bind(input.submission_key)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Failure::Missing)?;
            if prior_moment != id {
                return Err(Failure::Conflict);
            }
            prior_id
        }
    };
    let saved = comment(&mut **tx, actor, id, comment_id).await?;
    if saved.share_id != share
        || saved.kind != input.kind
        || saved.body != input.body
        || saved.feedback_writer.as_deref() != writer
        || saved.feedback_date.as_deref() != date
        || saved.feedback_asset_id != asset
    {
        return Err(Failure::Conflict);
    }
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
    let updated = set_scoped_withdrawal(&mut tx, actor, id, None, comment_id, withdraw).await?;
    tx.commit().await?;
    Ok(updated)
}

pub(crate) async fn set_scoped_withdrawal(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    share: Option<Uuid>,
    comment_id: Uuid,
    withdraw: bool,
) -> Result<Comment, Failure> {
    let result = sqlx::query(
        "UPDATE moment_comments SET withdrawn_at = CASE WHEN $1 THEN coalesce(withdrawn_at, now()) ELSE NULL END
         WHERE id = $2 AND household_id = $3 AND moment_id = $4 AND author_id = $5 AND share_id IS NOT DISTINCT FROM $6",
    )
    .bind(withdraw)
    .bind(comment_id)
    .bind(actor.household_id)
    .bind(id)
    .bind(actor.id)
    .bind(share)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(Failure::Missing);
    }
    comment(&mut **tx, actor, id, comment_id).await
}
