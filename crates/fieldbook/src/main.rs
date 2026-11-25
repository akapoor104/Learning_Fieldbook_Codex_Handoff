use std::{env, io, net::Ipv4Addr};
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    if !env::args_os().skip(1).eq(["serve"]) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: fieldbook serve",
        ));
    }

    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "set PORT to a TCP port"))?;

    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    eprintln!("Fieldbook listening on {}", listener.local_addr()?);
    axum::serve(listener, fieldbook::router()).await
}
