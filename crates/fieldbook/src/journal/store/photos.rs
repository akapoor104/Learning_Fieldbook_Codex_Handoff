use serde::Serialize;
use sqlx::PgPool;
use time::Date;
use uuid::Uuid;

use super::{attachment_position, lock_moment, Actor, Failure};

#[derive(Serialize, sqlx::FromRow)]
pub(in crate::journal) struct Attachment {
    id: Uuid,
    label: String,
    uploader_id: Uuid,
    uploader_name: String,
    activity_date: Option<String>,
    from_inbox: bool,
    matched_by: Option<Uuid>,
    matched_by_name: Option<String>,
    source_url: Option<String>,
    copy_asset_id: Option<Uuid>,
}

const ATTACHMENTS: &str = r#"
    SELECT a.id, a.label, a.uploader_id, uploader.display_name AS uploader_name,
           to_char(inbox.activity_date, 'YYYY-MM-DD') AS activity_date,
           (inbox.asset_id IS NOT NULL) AS from_inbox,
           link.matched_by, matcher.display_name AS matched_by_name, a.source_url, a.copy_asset_id
    FROM family_assets a
    JOIN actors uploader ON uploader.id = a.uploader_id
    LEFT JOIN family_photo_inbox inbox ON inbox.asset_id = a.id
    LEFT JOIN moment_assets link ON link.asset_id = a.id AND link.withdrawn_at IS NULL
    LEFT JOIN actors matcher ON matcher.id = link.matched_by
    WHERE a.household_id = $1
"#;

pub(in crate::journal) async fn list(
    pool: &PgPool,
    actor: &Actor,
    learner: Uuid,
) -> Result<Vec<Attachment>, Failure> {
    if actor.role == "learner" && actor.id != learner {
        return Err(Failure::Missing);
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM actors WHERE id = $1 AND household_id = $2 AND role = 'learner')",
    )
    .bind(learner)
    .bind(actor.household_id)
    .fetch_one(pool)
    .await?;
    if !exists {
        return Err(Failure::Missing);
    }
    Ok(sqlx::query_as(&format!(
        "{ATTACHMENTS} AND inbox.learner_id = $2 AND link.asset_id IS NULL
         ORDER BY inbox.added_at DESC, a.id"
    ))
    .bind(actor.household_id)
    .bind(learner)
    .fetch_all(pool)
    .await?)
}

pub(in crate::journal) async fn add(
    pool: &PgPool,
    actor: &Actor,
    learner: Uuid,
    asset: Uuid,
    date: Option<Date>,
) -> Result<(), Failure> {
    if actor.role != "guardian" {
        return Err(Failure::Missing);
    }
    let mut tx = pool.begin().await?;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT a.id FROM family_assets a JOIN actors l ON l.household_id = a.household_id
         WHERE a.id = $1 AND a.household_id = $2 AND a.uploader_id = $3 AND a.source_url IS NULL
           AND l.id = $4 AND l.role = 'learner' FOR NO KEY UPDATE OF a FOR SHARE OF l",
    )
    .bind(asset)
    .bind(actor.household_id)
    .bind(actor.id)
    .bind(learner)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    let existing: Option<(Uuid, Option<Date>)> = sqlx::query_as(
        "SELECT learner_id, activity_date FROM family_photo_inbox WHERE asset_id = $1",
    )
    .bind(asset)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(existing) = existing {
        if existing != (learner, date) {
            return Err(Failure::Conflict);
        }
    } else {
        let linked: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM moment_assets WHERE asset_id = $1)")
                .bind(asset)
                .fetch_one(&mut *tx)
                .await?;
        if linked {
            return Err(Failure::Conflict);
        }
        sqlx::query(
            "INSERT INTO family_photo_inbox (asset_id, household_id, learner_id, activity_date)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(asset)
        .bind(actor.household_id)
        .bind(learner)
        .bind(date)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(in crate::journal) async fn linked(
    pool: &PgPool,
    actor: &Actor,
    moment: Uuid,
) -> Result<Vec<Attachment>, Failure> {
    super::read(pool, actor, moment).await?;
    Ok(sqlx::query_as(&format!(
        "{ATTACHMENTS} AND link.moment_id = $2
         AND EXISTS (SELECT 1 FROM family_moments WHERE id = $2 AND household_id = $1)
         ORDER BY link.position"
    ))
    .bind(actor.household_id)
    .bind(moment)
    .fetch_all(pool)
    .await?)
}

pub(in crate::journal) async fn set_match(
    pool: &PgPool,
    actor: &Actor,
    moment: Uuid,
    asset: Uuid,
    matched: bool,
) -> Result<(), Failure> {
    let mut tx = pool.begin().await?;
    let owner = lock_moment(&mut tx, actor.household_id, moment).await?;
    if actor.role == "learner" && actor.id != owner.learner_id {
        return Err(Failure::Missing);
    }
    // Serialize edits per moment for ordering and per inbox asset for competing matches.
    sqlx::query_scalar::<_, Uuid>(
        "SELECT asset_id FROM family_photo_inbox WHERE asset_id = $1 AND household_id = $2
           AND learner_id = $3 FOR UPDATE",
    )
    .bind(asset)
    .bind(actor.household_id)
    .bind(owner.learner_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    let current: Option<Uuid> =
        sqlx::query_scalar("SELECT moment_id FROM moment_assets WHERE asset_id = $1")
            .bind(asset)
            .fetch_optional(&mut *tx)
            .await?;
    if matched && current.is_some_and(|target| target != moment) {
        return Err(Failure::Conflict);
    }
    if (current == Some(moment)) == matched {
        tx.commit().await?;
        return Ok(());
    }
    if matched {
        let position = attachment_position(&mut tx, moment).await?;
        sqlx::query(
            "INSERT INTO moment_assets (moment_id, household_id, asset_id, position, matched_by)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(moment)
        .bind(actor.household_id)
        .bind(asset)
        .bind(position)
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    } else {
        let saved =
            super::snapshot(&mut *tx, actor.household_id, moment, Some(owner.revision)).await?;
        if saved.note.is_none() && saved.asset_ids.len() == 1 {
            return Err(Failure::Validation("note_or_asset_required"));
        }
        sqlx::query("DELETE FROM moment_assets WHERE moment_id = $1 AND asset_id = $2")
            .bind(moment)
            .bind(asset)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO family_photo_match_events (household_id, asset_id, moment_id, actor_id, matched)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor.household_id)
    .bind(asset)
    .bind(moment)
    .bind(actor.id)
    .bind(matched)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
