use std::{io, time::Duration};

use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::files::{matches_type, original_key, PrivateMedia, Verification};
use crate::identity::{Actor, Failure};

pub(super) const PART_BYTES: usize = 8 * 1024 * 1024;
const MAX_UPLOAD_BYTES: i64 = 8 * 1024 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    submission_key: Uuid,
    label: String,
    byte_count: i64,
    sha256: String,
    media_type: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(super) struct Upload {
    pub(super) id: Uuid,
    label: String,
    byte_count: i64,
    received: i64,
    asset_id: Option<Uuid>,
    media_type: String,
    #[serde(skip)]
    sha256: Vec<u8>,
}

const UPLOAD: &str = "id, label, byte_count, received, asset_id, media_type, sha256";
const LIVE_OWNER: &str = "id = $1 AND household_id = $2 AND uploader_id = $3
    AND (asset_id IS NOT NULL OR staging_expires_at > clock_timestamp())";

pub(super) async fn begin(
    pool: &PgPool,
    actor: &Actor,
    manifest: Manifest,
) -> Result<Upload, Failure> {
    let label = manifest.label.trim();
    if label.is_empty()
        || label.len() > 255
        || label.chars().any(char::is_control)
        || !(1..=MAX_UPLOAD_BYTES).contains(&manifest.byte_count)
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Failure::Validation("invalid_upload_manifest"));
    }
    if matches_type(&manifest.media_type, &[]).is_none() {
        return Err(Failure::Validation("unsupported_upload_type"));
    }
    let hash: Vec<_> = (0..64)
        .step_by(2)
        .map(|index| u8::from_str_radix(&manifest.sha256[index..index + 2], 16).unwrap())
        .collect();
    sqlx::query_as(&format!(
        "INSERT INTO private_uploads AS u
            (household_id, uploader_id, submission_key, label, byte_count, sha256, media_type)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (uploader_id, submission_key) DO UPDATE SET label = u.label
         WHERE u.household_id = EXCLUDED.household_id AND u.label = EXCLUDED.label
           AND u.byte_count = EXCLUDED.byte_count AND u.sha256 = EXCLUDED.sha256
           AND u.media_type = EXCLUDED.media_type
           AND (u.asset_id IS NOT NULL OR u.staging_expires_at > clock_timestamp())
         RETURNING {UPLOAD}"
    ))
    .bind(actor.household_id)
    .bind(actor.id)
    .bind(manifest.submission_key)
    .bind(label)
    .bind(manifest.byte_count)
    .bind(hash)
    .bind(manifest.media_type)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Conflict)
}

pub(super) async fn status(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<Upload, Failure> {
    sqlx::query_as(&format!(
        "SELECT {UPLOAD} FROM private_uploads WHERE {LIVE_OWNER}"
    ))
    .bind(id)
    .bind(actor.household_id)
    .bind(actor.id)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Missing)
}

async fn lock(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<(Transaction<'static, Postgres>, Upload), Failure> {
    let mut tx = pool.begin().await?;
    let upload = sqlx::query_as(&format!(
        "SELECT {UPLOAD} FROM private_uploads WHERE {LIVE_OWNER} FOR UPDATE"
    ))
    .bind(id)
    .bind(actor.household_id)
    .bind(actor.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    Ok((tx, upload))
}

pub(super) async fn append(
    pool: &PgPool,
    actor: &Actor,
    media: PrivateMedia,
    id: Uuid,
    offset: i64,
    bytes: Bytes,
) -> Result<Upload, Failure> {
    let (mut tx, mut upload) = lock(pool, actor, id).await?;
    if bytes.is_empty() || bytes.len() > PART_BYTES {
        return Err(Failure::Validation("invalid_upload_part"));
    }
    let end = offset
        .checked_add(bytes.len() as i64)
        .filter(|end| offset >= 0 && *end <= upload.byte_count)
        .ok_or(Failure::Conflict)?;
    if end <= upload.received {
        let ready = upload.asset_id.is_some();
        if !disk(move || media.matches(id, ready, offset as u64, &bytes)).await? {
            return Err(Failure::Conflict);
        }
        return Ok(upload);
    }
    if offset != upload.received {
        return Err(Failure::Conflict);
    }
    disk(move || media.append(id, offset as u64, &bytes)).await?;
    sqlx::query(
        "UPDATE private_uploads SET received = $2,
            staging_expires_at = clock_timestamp() + interval '24 hours' WHERE id = $1",
    )
    .bind(id)
    .bind(end)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    upload.received = end;
    Ok(upload)
}

pub(super) async fn finish(
    pool: &PgPool,
    actor: &Actor,
    media: PrivateMedia,
    id: Uuid,
) -> Result<Upload, Failure> {
    let (mut tx, mut upload) = lock(pool, actor, id).await?;
    if upload.asset_id.is_some() {
        return Ok(upload);
    }
    if upload.received != upload.byte_count {
        return Err(Failure::Conflict);
    }
    let count = upload.byte_count;
    let hash = upload.sha256.clone();
    let media_type = upload.media_type.clone();
    match disk(move || media.publish(id, count as u64, &hash, &media_type)).await? {
        Verification::Valid => {}
        Verification::ChecksumMismatch => {
            return Err(Failure::Validation("upload_checksum_mismatch"));
        }
        Verification::TypeMismatch => {
            return Err(Failure::Validation("upload_type_mismatch"));
        }
    }
    sqlx::query(
        "INSERT INTO assets (id, household_id, uploader_id, label, storage_key, byte_count, sha256, media_type)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(actor.household_id)
    .bind(actor.id)
    .bind(&upload.label)
    .bind(original_key(id))
    .bind(count)
    .bind(&upload.sha256)
    .bind(&upload.media_type)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE private_uploads SET asset_id = id, staging_expires_at = clock_timestamp() WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    upload.asset_id = Some(id);
    Ok(upload)
}

pub(super) async fn clean_one(pool: &PgPool, media: PrivateMedia) -> Result<bool, Failure> {
    let mut tx = pool.begin().await?;
    let expired: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, asset_id IS NOT NULL FROM private_uploads
         WHERE staging_expires_at <= statement_timestamp()
         ORDER BY staging_expires_at LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, ready)) = expired else {
        return Ok(false);
    };
    disk(move || media.clean(id, ready)).await?;
    let update = if ready {
        "UPDATE private_uploads SET staging_expires_at = NULL WHERE id = $1"
    } else {
        "DELETE FROM private_uploads WHERE id = $1"
    };
    sqlx::query(update).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn sweep(pool: PgPool, media: PrivateMedia) {
    loop {
        for _ in 0..32 {
            match clean_one(&pool, media.clone()).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    eprintln!("Could not clean expired private upload staging");
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn disk<T: Send + 'static>(
    action: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(action)
        .await
        .map_err(|_| Failure::Unavailable)?
        .map_err(|_| Failure::Unavailable)
}
