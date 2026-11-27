use std::{env, io, net::Ipv4Addr};

use fieldbook::{identity, storage};
use sqlx::{postgres::PgConnectOptions, PgPool};
use tokio::net::TcpListener;

mod media;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let mut args = env::args_os().skip(1).peekable();
    let command = args.next();
    if command.as_deref().and_then(|arg| arg.to_str()) == Some("account") {
        return account(args.collect()).await;
    }
    if args.peek().is_none() {
        match command.as_deref().and_then(|arg| arg.to_str()) {
            Some("serve") => return serve().await,
            Some("migrate") => return migrate().await,
            _ => {}
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: fieldbook serve | migrate | account",
    ))
}

async fn serve() -> io::Result<()> {
    validate_hosted_settings()?;
    let media = media::configured_media()?;
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "set PORT to a TCP port"))?;

    let pool = database_pool()?;
    let mut router = fieldbook::router(pool.clone());
    let origin = env::var("FIELD_PUBLIC_ORIGIN").unwrap_or_default();
    let rp = env::var("FIELD_RP_ID").unwrap_or_default();
    let secret = env::var("FIELD_SESSION_SECRET").unwrap_or_default();
    if env::var("FIELD_ENV").as_deref() == Ok("production")
        || [&origin, &rp, &secret]
            .iter()
            .any(|value| !value.is_empty())
    {
        let pool = pool.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "set DATABASE_URL to serve the notebook",
            )
        })?;
        let web_root = env::var_os("FIELD_WEB_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "web/dist".into());
        router = router.merge(identity::router_with_media(
            pool.clone(),
            &origin,
            &rp,
            &secret,
            web_root,
            media.clone(),
        )?);
        if let Some(media) = media {
            tokio::spawn(storage::sweep_uploads(pool, media));
        }
    }
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    eprintln!("Fieldbook listening on {}", listener.local_addr()?);
    axum::serve(listener, router).await
}

async fn account(args: Vec<std::ffi::OsString>) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: fieldbook account household LABEL | add HOUSEHOLD_ID USERNAME DISPLAY_NAME ROLE | enroll ACTOR_ID | recover ACTOR_ID | revoke ACTOR_ID",
    )
    };
    let args: Vec<&str> = args
        .iter()
        .map(|arg| arg.to_str().ok_or_else(invalid))
        .collect::<io::Result<_>>()?;
    let pool = database_pool()?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "set DATABASE_URL to manage accounts",
        )
    })?;
    let database_error = |_| io::Error::other("account operation failed");
    match args.as_slice() {
        ["household", label] => {
            let id: uuid::Uuid =
                sqlx::query_scalar("INSERT INTO households (label) VALUES ($1) RETURNING id")
                    .bind(label)
                    .fetch_one(&pool)
                    .await
                    .map_err(database_error)?;
            println!("{id}");
        }
        ["add", household, username, name, role] => {
            let id = identity::provision_actor(
                &pool,
                household.parse().map_err(|_| invalid())?,
                username,
                name,
                role,
            )
            .await
            .map_err(database_error)?;
            println!("{id}");
        }
        [action @ ("enroll" | "recover"), actor] => {
            let grant = identity::issue_enrollment(
                &pool,
                actor.parse().map_err(|_| invalid())?,
                *action == "recover",
            )
            .await
            .map_err(database_error)?
            .ok_or_else(|| io::Error::other("account missing or already enrolled"))?;
            // This invitation is shown once on the operator's terminal.
            println!("{grant}");
        }
        ["revoke", actor] => {
            if !identity::revoke_credentials(&pool, actor.parse().map_err(|_| invalid())?)
                .await
                .map_err(database_error)?
            {
                return Err(io::Error::other("account missing"));
            }
        }
        _ => return Err(invalid()),
    }
    Ok(())
}

async fn migrate() -> io::Result<()> {
    let pool = database_pool()?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "set DATABASE_URL to migrate")
    })?;
    storage::migrate(&pool)
        .await
        .map_err(|_| io::Error::other("database migration failed"))?;
    eprintln!("Fieldbook migrations applied");
    Ok(())
}

fn database_pool() -> io::Result<Option<PgPool>> {
    let options = match env::var("DATABASE_URL") {
        Ok(url) => url.parse::<PgConnectOptions>().ok(),
        Err(env::VarError::NotPresent) => return Ok(None),
        Err(env::VarError::NotUnicode(_)) => None,
    };
    options
        .map(storage::connect_lazy)
        .map(Some)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid DATABASE_URL"))
}

fn validate_hosted_settings() -> io::Result<()> {
    if env::var("FIELD_ENV").as_deref() != Ok("production") {
        return Ok(());
    }

    if !env::var("FIELD_DIARY_KEYRING").is_ok_and(|value| !value.trim().is_empty()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "set FIELD_DIARY_KEYRING for production",
        ));
    }

    if !env::var("FIELD_PUBLIC_ORIGIN").is_ok_and(|value| {
        url::Url::parse(&value)
            .is_ok_and(|url| url.scheme() == "https" && url.origin().ascii_serialization() == value)
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "set FIELD_PUBLIC_ORIGIN to the canonical HTTPS origin",
        ));
    }

    Ok(())
}
