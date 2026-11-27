use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use super::{attachment_position, lock_moment, snapshot, Actor, Failure};

pub(in crate::journal) async fn create_link(
    pool: &PgPool,
    actor: &Actor,
    submission: Uuid,
    label: &str,
    source: &str,
) -> Result<Uuid, Failure> {
    let label = label.trim();
    if label.is_empty() || label.len() > 255 || label.chars().any(char::is_control) {
        return Err(Failure::Validation("invalid_link_label"));
    }
    let source = source.trim();
    if source.len() > 4096
        || source
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || !source
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return Err(Failure::Validation("invalid_link_url"));
    }
    let url = Url::parse(source).map_err(|_| Failure::Validation("invalid_link_url"))?;
    if !url.has_host() || !url.username().is_empty() || url.password().is_some() {
        return Err(Failure::Validation("invalid_link_url"));
    }
    sqlx::query_scalar(
        "INSERT INTO assets AS a (household_id, uploader_id, label, source_url, link_submission_key)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (uploader_id, link_submission_key) DO UPDATE SET link_submission_key = EXCLUDED.link_submission_key
         WHERE a.household_id = EXCLUDED.household_id AND a.label = EXCLUDED.label
           AND a.source_url = EXCLUDED.source_url
         RETURNING a.id",
    )
    .bind(actor.household_id)
    .bind(actor.id)
    .bind(label)
    .bind(url.as_str())
    .bind(submission)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Conflict)
}

pub(in crate::journal) async fn set_link(
    pool: &PgPool,
    actor: &Actor,
    moment: Uuid,
    asset: Uuid,
    attached: bool,
) -> Result<(), Failure> {
    let mut tx = pool.begin().await?;
    let owner = lock_moment(&mut tx, actor.household_id, moment).await?;
    if actor.role == "learner" && actor.id != owner.learner_id {
        return Err(Failure::Missing);
    }
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM family_assets WHERE id = $1 AND household_id = $2 AND uploader_id = $3
           AND source_url IS NOT NULL",
    )
    .bind(asset)
    .bind(actor.household_id)
    .bind(actor.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    let active: Option<bool> = sqlx::query_scalar(
        "SELECT withdrawn_at IS NULL FROM moment_assets WHERE moment_id = $1 AND asset_id = $2",
    )
    .bind(moment)
    .bind(asset)
    .fetch_optional(&mut *tx)
    .await?;
    if active != Some(true) && attached {
        let position = attachment_position(&mut tx, moment).await?;
        sqlx::query(
            "INSERT INTO moment_assets (moment_id, household_id, asset_id, position, matched_by)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (moment_id, asset_id) DO UPDATE SET withdrawn_at = NULL",
        )
        .bind(moment)
        .bind(actor.household_id)
        .bind(asset)
        .bind(position)
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    } else if active == Some(true) && !attached {
        let saved = snapshot(&mut *tx, actor.household_id, moment, Some(owner.revision)).await?;
        if saved.note.is_none() && saved.asset_ids.len() == 1 {
            return Err(Failure::Validation("note_or_asset_required"));
        }
        sqlx::query(
            "UPDATE moment_assets SET withdrawn_at = clock_timestamp()
             WHERE moment_id = $1 AND asset_id = $2",
        )
        .bind(moment)
        .bind(asset)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(in crate::journal) async fn set_copy(
    pool: &PgPool,
    actor: &Actor,
    moment: Uuid,
    asset: Uuid,
    original: Uuid,
    kept: bool,
) -> Result<(), Failure> {
    let mut tx = pool.begin().await?;
    let owner = lock_moment(&mut tx, actor.household_id, moment).await?;
    if actor.role == "learner" && actor.id != owner.learner_id {
        return Err(Failure::Missing);
    }
    let current: Option<Uuid> = sqlx::query_scalar(
        "SELECT a.copy_asset_id FROM family_assets a
         JOIN moment_assets link ON link.asset_id = a.id AND link.household_id = a.household_id
         WHERE a.id = $1 AND a.household_id = $2 AND a.uploader_id = $3
           AND a.source_url IS NOT NULL AND link.moment_id = $4 AND link.withdrawn_at IS NULL
         FOR NO KEY UPDATE OF a",
    )
    .bind(asset)
    .bind(actor.household_id)
    .bind(actor.id)
    .bind(moment)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    // A backing file is an immutable, verified upload by the current actor.
    // An unfinished upload has no asset_id and cannot become visible through the link.
    sqlx::query_scalar::<_, Uuid>(
        "SELECT asset_id FROM private_uploads WHERE asset_id = $1 AND household_id = $2 AND uploader_id = $3",
    )
    .bind(original)
    .bind(actor.household_id)
    .bind(actor.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    if current.is_some_and(|id| id != original) {
        return Err(Failure::Conflict);
    }
    let next = kept.then_some(original);
    if current != next {
        sqlx::query("UPDATE assets SET copy_asset_id = $1 WHERE id = $2 AND household_id = $3")
            .bind(next)
            .bind(asset)
            .bind(actor.household_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
