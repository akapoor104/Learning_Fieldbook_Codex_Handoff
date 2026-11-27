use std::{env, io, net::Ipv4Addr};

use fieldbook::storage;
use sqlx::{postgres::PgConnectOptions, PgPool};
use tokio::net::TcpListener;

mod media;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let mut args = env::args_os().skip(1);
    let command = args.next();
    if args.next().is_none() {
        match command.as_deref().and_then(|arg| arg.to_str()) {
            Some("serve") => return serve().await,
            Some("migrate") => return migrate().await,
            _ => {}
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: fieldbook serve | migrate",
    ))
}

async fn serve() -> io::Result<()> {
    validate_hosted_settings()?;
    media::validate()?;
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "set PORT to a TCP port"))?;

    let router = fieldbook::router(database_pool()?);
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    eprintln!("Fieldbook listening on {}", listener.local_addr()?);
    axum::serve(listener, router).await
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
