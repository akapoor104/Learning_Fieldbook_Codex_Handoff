use super::{diary, hash_token, random_token, Actor, Failure, IDLE_SECONDS};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub async fn provision_actor(
    pool: &PgPool,
    household_id: Uuid,
    username: &str,
    display_name: &str,
    role: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO actors (household_id, username, display_name, role)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(household_id)
    .bind(username)
    .bind(display_name)
    .bind(role)
    .fetch_one(pool)
    .await
}

pub async fn issue_enrollment(
    pool: &PgPool,
    actor: Uuid,
    replace_credential: bool,
) -> Result<Option<String>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if !lock_actor(&mut tx, actor).await? {
        return Ok(None);
    }
    if !replace_credential
        && sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM passkeys WHERE actor_id = $1)",
        )
        .bind(actor)
        .fetch_one(&mut *tx)
        .await?
    {
        return Ok(None);
    }
    clear_credentials(&mut tx, actor).await?;
    let raw = random_token();
    sqlx::query(
        "INSERT INTO enrollment_grants (actor_id, token_hash, expires_at)
         VALUES ($1, $2, now() + interval '15 minutes')",
    )
    .bind(actor)
    .bind(hash_token(&raw))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(raw))
}

pub async fn revoke_credentials(pool: &PgPool, actor: Uuid) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if !lock_actor(&mut tx, actor).await? {
        return Ok(false);
    }
    clear_credentials(&mut tx, actor).await?;
    tx.commit().await?;
    Ok(true)
}

async fn lock_actor(tx: &mut Transaction<'_, Postgres>, actor: Uuid) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM actors WHERE id = $1 FOR UPDATE")
            .bind(actor)
            .fetch_optional(&mut **tx)
            .await?
            .is_some(),
    )
}

async fn clear_credentials(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE actors SET credential_generation = credential_generation + 1 WHERE id = $1",
    )
    .bind(actor)
    .execute(&mut **tx)
    .await?;
    for statement in [
        "DELETE FROM sessions WHERE actor_id = $1",
        "DELETE FROM passkeys WHERE actor_id = $1",
        "DELETE FROM enrollment_grants WHERE actor_id = $1",
        "DELETE FROM webauthn_ceremonies WHERE actor_id = $1",
    ] {
        sqlx::query(statement)
            .bind(actor)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub(super) async fn enrollment_actor(pool: &PgPool, token_hash: &[u8]) -> Result<Actor, Failure> {
    sqlx::query_as(
        "SELECT a.* FROM enrollment_grants g JOIN actors a ON a.id = g.actor_id
         WHERE g.token_hash = $1 AND g.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Unauthorized)
}

pub(super) async fn login_actor(
    pool: &PgPool,
    username: &str,
) -> Result<(Actor, Passkey), Failure> {
    let LoginRow { actor, credential } = sqlx::query_as(
        "SELECT a.*, p.credential FROM actors a JOIN passkeys p ON p.actor_id = a.id
         WHERE a.username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Unauthorized)?;
    Ok((actor, credential.0))
}

#[derive(sqlx::FromRow)]
struct LoginRow {
    #[sqlx(flatten)]
    actor: Actor,
    credential: sqlx::types::Json<Passkey>,
}

#[derive(Clone, Copy)]
pub(super) enum Purpose {
    Enroll,
    SignIn,
    StepUp,
    Diary,
}

impl Purpose {
    fn name(self) -> &'static str {
        match self {
            Self::Enroll => "enroll",
            Self::SignIn => "sign-in",
            Self::StepUp => "step-up",
            Self::Diary => "diary",
        }
    }
}

pub(super) async fn start_ceremony(
    pool: &PgPool,
    actor: &Actor,
    purpose: Purpose,
    session_hash: Option<&[u8]>,
    state: Value,
) -> Result<String, Failure> {
    sqlx::query("DELETE FROM webauthn_ceremonies WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    let raw = random_token();
    sqlx::query(
        "INSERT INTO webauthn_ceremonies
          (token_hash, actor_id, generation, purpose, session_hash, challenge_state, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, now() + interval '2 minutes')",
    )
    .bind(hash_token(&raw))
    .bind(actor.id)
    .bind(actor.credential_generation)
    .bind(purpose.name())
    .bind(session_hash)
    .bind(state)
    .execute(pool)
    .await?;
    Ok(raw)
}

#[derive(sqlx::FromRow)]
pub(super) struct Ceremony {
    pub actor_id: Uuid,
    pub generation: i64,
    pub challenge_state: Value,
}

pub(super) async fn take_ceremony(
    pool: &PgPool,
    raw: &str,
    purpose: Purpose,
    session_hash: Option<&[u8]>,
) -> Result<Ceremony, Failure> {
    sqlx::query_as(
        "DELETE FROM webauthn_ceremonies WHERE token_hash = $1 AND purpose = $2
           AND session_hash IS NOT DISTINCT FROM $3 AND expires_at > now()
         RETURNING actor_id, generation, challenge_state",
    )
    .bind(hash_token(raw))
    .bind(purpose.name())
    .bind(session_hash)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Unauthorized)
}

pub(super) async fn current_actor(
    tx: &mut Transaction<'_, Postgres>,
    ceremony: &Ceremony,
) -> Result<Actor, Failure> {
    sqlx::query_as("SELECT * FROM actors WHERE id = $1 AND credential_generation = $2 FOR UPDATE")
        .bind(ceremony.actor_id)
        .bind(ceremony.generation)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Failure::Unauthorized)
}

pub(super) async fn save_enrollment(
    pool: &PgPool,
    ceremony: &Ceremony,
    passkey: &Passkey,
) -> Result<(), Failure> {
    let mut tx = pool.begin().await?;
    let actor = current_actor(&mut tx, ceremony).await?;
    let used =
        sqlx::query("DELETE FROM enrollment_grants WHERE actor_id = $1 AND expires_at > now()")
            .bind(actor.id)
            .execute(&mut *tx)
            .await?;
    if used.rows_affected() != 1 {
        return Err(Failure::Unauthorized);
    }
    sqlx::query("INSERT INTO passkeys (actor_id, credential_id, credential) VALUES ($1, $2, $3)")
        .bind(actor.id)
        .bind(passkey.cred_id().as_slice())
        .bind(sqlx::types::Json(passkey))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM webauthn_ceremonies WHERE actor_id = $1 AND purpose = 'enroll'")
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn passkey(pool: &PgPool, actor: Uuid) -> Result<Passkey, Failure> {
    fetch_passkey(pool, actor).await
}

async fn fetch_passkey<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    actor: Uuid,
) -> Result<Passkey, Failure> {
    sqlx::query_scalar::<_, sqlx::types::Json<Passkey>>(
        "SELECT credential FROM passkeys WHERE actor_id = $1",
    )
    .bind(actor)
    .fetch_optional(executor)
    .await?
    .map(|credential| credential.0)
    .ok_or(Failure::Unauthorized)
}

pub(super) async fn passkey_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
) -> Result<Passkey, Failure> {
    fetch_passkey(&mut **tx, actor).await
}

pub(super) async fn save_assertion(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    counter: u32,
    updated: &Passkey,
) -> Result<(), Failure> {
    let result = sqlx::query(
        "UPDATE passkeys SET credential = $1, signature_counter = $3 WHERE actor_id = $2
         AND ((signature_counter = 0 AND $3 = 0) OR signature_counter < $3)",
    )
    .bind(sqlx::types::Json(updated))
    .bind(actor)
    .bind(i64::from(counter))
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(Failure::Unauthorized);
    }
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub(crate) struct Session {
    #[sqlx(flatten)]
    pub actor: Actor,
    pub idle_seconds: i32,
    pub elevated_seconds: i32,
    pub diary_learner_id: Option<Uuid>,
    pub diary_seconds: i32,
}

const SESSION_QUERY: &str = "SELECT a.*,
        ceil(extract(epoch FROM least(s.idle_until, s.expires_at) - statement_timestamp()))::int AS idle_seconds,
        greatest(0, ceil(extract(epoch FROM s.elevated_until - statement_timestamp())))::int AS elevated_seconds,
        d.learner_id AS diary_learner_id,
        greatest(0, ceil(extract(epoch FROM d.expires_at - statement_timestamp())))::int AS diary_seconds
     FROM sessions s JOIN actors a ON a.id = s.actor_id AND a.household_id = s.household_id
     LEFT JOIN active_diary_sessions d ON d.token_hash = s.token_hash
     WHERE s.token_hash = $1 AND s.idle_until > statement_timestamp() AND s.expires_at > statement_timestamp()";

pub(super) async fn session(pool: &PgPool, hash: &[u8]) -> Result<Option<Session>, Failure> {
    Ok(sqlx::query_as(SESSION_QUERY)
        .bind(hash)
        .fetch_optional(pool)
        .await?)
}

pub(super) async fn new_session(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    hash: &[u8],
    previous: Option<&[u8]>,
    diary: Option<diary::AccessRequest>,
) -> Result<Session, Failure> {
    let (expires_at, event, step_up) = if let Some(previous) = previous {
        if actor.role != "guardian" {
            return Err(Failure::Forbidden);
        }
        let expires_at: OffsetDateTime = sqlx::query_scalar(
            "DELETE FROM sessions
             WHERE token_hash = $1 AND actor_id = $2 AND household_id = $3
               AND idle_until > statement_timestamp() AND expires_at > statement_timestamp()
             RETURNING expires_at",
        )
        .bind(previous)
        .bind(actor.id)
        .bind(actor.household_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Failure::Unauthorized)?;
        let event = match diary {
            Some(request) => Some(diary::append_event(tx, actor, request, expires_at).await?),
            None => None,
        };
        (Some(expires_at), event, event.is_none())
    } else {
        (None, None, false)
    };
    sqlx::query(
        "INSERT INTO sessions (token_hash, actor_id, household_id, idle_until, expires_at,
                          diary_event_id, diary_previous_hash, elevated_until)
         VALUES ($1, $2, $3, statement_timestamp() + make_interval(secs => $4),
                 coalesce($5, statement_timestamp() + interval '8 hours'), $6,
                 $7, CASE WHEN $8 THEN statement_timestamp() + interval '5 minutes' END)",
    )
    .bind(hash)
    .bind(actor.id)
    .bind(actor.household_id)
    .bind(IDLE_SECONDS)
    .bind(expires_at)
    .bind(event)
    .bind(previous.filter(|_| event.is_some()))
    .bind(step_up)
    .execute(&mut **tx)
    .await?;
    let session: Session = sqlx::query_as(SESSION_QUERY)
        .bind(hash)
        .fetch_one(&mut **tx)
        .await?;
    if event.is_some() && session.diary_learner_id.is_none() {
        return Err(Failure::Unauthorized);
    }
    Ok(session)
}

pub(super) async fn refresh(pool: &PgPool, hash: &[u8]) -> Result<(), Failure> {
    let updated = sqlx::query(
        "UPDATE sessions SET idle_until = now() + make_interval(secs => $2)
         WHERE token_hash = $1 AND idle_until > now() AND expires_at > now()",
    )
    .bind(hash)
    .bind(IDLE_SECONDS)
    .execute(pool)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(Failure::Unauthorized);
    }
    Ok(())
}

pub(super) async fn has_diary_successor(pool: &PgPool, hash: &[u8]) -> Result<bool, Failure> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sessions WHERE diary_previous_hash = $1)")
            .bind(hash)
            .fetch_one(pool)
            .await?,
    )
}

pub(super) async fn delete_session(pool: &PgPool, hash: &[u8]) -> Result<(), Failure> {
    let mut tx = pool.begin().await?;
    // This DELETE serializes with rotation on the previous row. The second
    // statement gets a fresh snapshot and can revoke a just-committed diary successor.
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE diary_previous_hash = $1")
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
