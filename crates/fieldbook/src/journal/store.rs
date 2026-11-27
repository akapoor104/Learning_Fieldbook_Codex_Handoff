use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};
use time::Date;
use uuid::Uuid;

use super::{activity_date, normalized_note, Actor, CreateMoment, EditMoment, Failure};

pub(super) mod contributions;
pub(super) mod photos;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(super) struct Moment {
    pub id: Uuid,
    pub learner_id: Uuid,
    pub learner_name: String,
    pub recorder_id: Uuid,
    pub recorder_name: String,
    pub voice: String,
    pub visibility: String,
    pub created_at: String,
    pub revision: i32,
    pub editor_id: Uuid,
    pub editor_name: String,
    pub updated_at: String,
    pub note: Option<String>,
    pub activity_date: Option<String>,
    pub asset_ids: Vec<Uuid>,
    pub confirmed: bool,
}

const MOMENT: &str = r#"
    SELECT m.id, m.learner_id, learner.display_name AS learner_name,
           m.recorder_id, recorder.display_name AS recorder_name, m.voice, m.visibility,
           to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at,
           r.revision, r.editor_id, editor.display_name AS editor_name,
           to_char(r.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS updated_at,
           r.note, to_char(r.activity_date, 'YYYY-MM-DD') AS activity_date,
           ARRAY(SELECT asset_id FROM moment_assets WHERE moment_id = m.id ORDER BY position)
               AS asset_ids,
           (confirmation.confirmed_at IS NOT NULL AND confirmation.withdrawn_at IS NULL) AS confirmed
    FROM moments m
    JOIN moment_revisions r ON r.moment_id = m.id
    JOIN actors learner ON learner.id = m.learner_id
    JOIN actors recorder ON recorder.id = m.recorder_id
    JOIN actors editor ON editor.id = r.editor_id
    LEFT JOIN moment_confirmations confirmation
      ON confirmation.moment_id = m.id AND confirmation.revision = r.revision
    WHERE m.household_id = $1 AND m.visibility = 'family'
"#;

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct Learner {
    id: Uuid,
    display_name: String,
}

pub(super) async fn learners(pool: &PgPool, actor: &Actor) -> Result<Vec<Learner>, Failure> {
    Ok(sqlx::query_as(
        "SELECT id, display_name FROM actors WHERE household_id = $1 AND role = 'learner'
           AND ($2 OR id = $3) ORDER BY display_name, id",
    )
    .bind(actor.household_id)
    .bind(actor.role == "guardian")
    .bind(actor.id)
    .fetch_all(pool)
    .await?)
}

pub(super) async fn list(pool: &PgPool, actor: &Actor) -> Result<Vec<Moment>, Failure> {
    Ok(sqlx::query_as(&format!(
        "{MOMENT} AND r.revision = m.revision
         ORDER BY coalesce(r.activity_date, (m.created_at AT TIME ZONE 'America/Los_Angeles')::date)
         DESC, m.created_at DESC, m.id DESC"
    ))
    .bind(actor.household_id)
    .fetch_all(pool)
    .await?)
}

pub(super) async fn read(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<Moment, Failure> {
    snapshot(pool, actor.household_id, id, None).await
}

async fn snapshot<'e>(
    executor: impl PgExecutor<'e>,
    household: Uuid,
    id: Uuid,
    revision: Option<i32>,
) -> Result<Moment, Failure> {
    sqlx::query_as(&format!(
        "{MOMENT} AND m.id = $2 AND r.revision = coalesce($3, m.revision)"
    ))
    .bind(household)
    .bind(id)
    .bind(revision)
    .fetch_optional(executor)
    .await?
    .ok_or(Failure::Missing)
}

pub(super) async fn history(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<Vec<Moment>, Failure> {
    let rows = sqlx::query_as(&format!("{MOMENT} AND m.id = $2 ORDER BY r.revision DESC"))
        .bind(actor.household_id)
        .bind(id)
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Err(Failure::Missing);
    }
    Ok(rows)
}

pub(super) async fn create(
    pool: &PgPool,
    actor: &Actor,
    input: &CreateMoment,
) -> Result<Moment, Failure> {
    let note = normalized_note(input.note.as_deref())?;
    let date = activity_date(input.activity_date.as_deref())?;
    if note.is_none() && (input.asset_ids.is_empty() || input.scribed) {
        return Err(Failure::Validation("note_or_asset_required"));
    }
    if input.asset_ids.len() > 20 {
        return Err(Failure::Validation("too_many_assets"));
    }

    let mut tx = pool.begin().await?;
    let (learner, voice) = capture_identity(&mut tx, actor, input).await?;
    let submission = Sha256::digest(
        json!({
            "learner_id": learner,
            "voice": voice,
            "note": note,
            "activity_date": input.activity_date,
            "asset_ids": input.asset_ids
        })
        .to_string()
        .as_bytes(),
    );
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO moments (household_id, learner_id, recorder_id, voice, submission_key, submission_hash)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (recorder_id, submission_key) DO NOTHING RETURNING id",
    )
    .bind(actor.household_id)
    .bind(learner)
    .bind(actor.id)
    .bind(voice)
    .bind(input.submission_key)
    .bind(submission.as_slice())
    .fetch_optional(&mut *tx)
    .await?;

    let id = if let Some(id) = inserted {
        insert_revision(&mut tx, actor, id, 1, note, date).await?;
        for (position, asset) in input.asset_ids.iter().enumerate() {
            link_asset(&mut tx, actor, id, *asset, position as i32).await?;
        }
        id
    } else {
        let (id, original): (Uuid, Vec<u8>) = sqlx::query_as(
            "SELECT id, submission_hash FROM moments WHERE household_id = $1 AND recorder_id = $2
               AND submission_key = $3 AND visibility = 'family'",
        )
        .bind(actor.household_id)
        .bind(actor.id)
        .bind(input.submission_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Failure::Missing)?;
        if original != submission.as_slice() {
            return Err(Failure::Conflict);
        }
        id
    };
    let mut moment = snapshot(&mut *tx, actor.household_id, id, Some(1)).await?;
    moment.asset_ids.clone_from(&input.asset_ids);
    tx.commit().await?;
    Ok(moment)
}

async fn capture_identity(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    input: &CreateMoment,
) -> Result<(Uuid, &'static str), Failure> {
    if actor.role == "learner" {
        if input.learner_id.is_some_and(|id| id != actor.id) || input.scribed {
            return Err(Failure::Missing);
        }
        return Ok((actor.id, "student"));
    }
    let learner = input
        .learner_id
        .ok_or(Failure::Validation("choose_learner"))?;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM actors WHERE id = $1 AND household_id = $2 AND role = 'learner' FOR SHARE",
    )
    .bind(learner)
    .bind(actor.household_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Failure::Missing)?;
    let voice = if input.scribed { "scribed" } else { "guardian" };
    Ok((learner, voice))
}

async fn link_asset(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    asset: Uuid,
    position: i32,
) -> Result<(), Failure> {
    let allowed = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM assets WHERE id = $1 AND household_id = $2 AND uploader_id = $3 FOR SHARE",
    )
    .bind(asset)
    .bind(actor.household_id)
    .bind(actor.id)
    .fetch_optional(&mut **tx)
    .await?;
    if allowed.is_none() {
        return Err(Failure::Missing);
    }
    let in_inbox: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM family_photo_inbox WHERE asset_id = $1)")
            .bind(asset)
            .fetch_one(&mut **tx)
            .await?;
    if in_inbox {
        return Err(Failure::Missing);
    }
    let linked = sqlx::query(
        "INSERT INTO moment_assets (moment_id, household_id, asset_id, position, matched_by)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (moment_id, asset_id) DO NOTHING",
    )
    .bind(id)
    .bind(actor.household_id)
    .bind(asset)
    .bind(position)
    .bind(actor.id)
    .execute(&mut **tx)
    .await?;
    if linked.rows_affected() != 1 {
        return Err(Failure::Validation("duplicate_asset"));
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct MomentOwner {
    recorder_id: Uuid,
    learner_id: Uuid,
    voice: String,
    revision: i32,
}

impl MomentOwner {
    fn writer(&self) -> Uuid {
        if self.voice == "guardian" {
            self.recorder_id
        } else {
            self.learner_id
        }
    }
}

async fn lock_moment(
    tx: &mut Transaction<'_, Postgres>,
    household: Uuid,
    id: Uuid,
) -> Result<MomentOwner, Failure> {
    sqlx::query_as(
        "SELECT recorder_id, learner_id, voice, revision FROM moments
         WHERE id = $1 AND household_id = $2 AND visibility = 'family' FOR NO KEY UPDATE",
    )
    .bind(id)
    .bind(household)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Failure::Missing)
}

pub(super) async fn edit(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    input: &EditMoment,
) -> Result<Moment, Failure> {
    let mut tx = pool.begin().await?;
    let moment = lock_moment(&mut tx, actor.household_id, id).await?;
    if moment.writer() != actor.id {
        return Err(Failure::Missing);
    }
    let saved = snapshot(&mut *tx, actor.household_id, id, Some(moment.revision)).await?;
    if input.expected_revision != moment.revision {
        return Err(Failure::Stale {
            saved: json!(saved),
            submitted: json!(input),
        });
    }
    let note = normalized_note(input.note.as_deref())?;
    let date = activity_date(input.activity_date.as_deref())?;
    if note.is_none() && (saved.asset_ids.is_empty() || moment.voice == "scribed") {
        return Err(Failure::Validation("note_or_asset_required"));
    }
    if saved.note.as_deref() == note && saved.activity_date == input.activity_date {
        tx.commit().await?;
        return Ok(saved);
    }
    if moment.voice == "scribed" {
        contributions::withdraw_confirmation(&mut tx, id, moment.revision).await?;
    }
    let revision = moment.revision.checked_add(1).ok_or(Failure::Conflict)?;
    insert_revision(&mut tx, actor, id, revision, note, date).await?;
    sqlx::query("UPDATE moments SET revision = $1 WHERE id = $2 AND household_id = $3")
        .bind(revision)
        .bind(id)
        .bind(actor.household_id)
        .execute(&mut *tx)
        .await?;
    let updated = snapshot(&mut *tx, actor.household_id, id, Some(revision)).await?;
    tx.commit().await?;
    Ok(updated)
}

async fn insert_revision(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    moment: Uuid,
    revision: i32,
    note: Option<&str>,
    date: Option<Date>,
) -> Result<(), Failure> {
    sqlx::query(
        "INSERT INTO moment_revisions (moment_id, household_id, revision, editor_id, note, activity_date)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(moment)
    .bind(actor.household_id)
    .bind(revision)
    .bind(actor.id)
    .bind(note)
    .bind(date)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
