//! Checks a running server end to end: asks how to log in, logs in with a
//! password, asks who we are, lists the projects, and logs out.
//!
//! ```sh
//! AEGIS_URL=https://aegis.example.com AEGIS_USER=admin AEGIS_PASSWORD='...' \
//!   cargo run -p aegis-client --example smoke
//! ```
//!
//! `AEGIS_TRANSPORT` is `auto` (default), `grpc` or `grpc-web`. The password
//! is read from the environment so it never shows in the shell history as an
//! argument. Exits with status 1 and the reason on the first failure.

use std::process::ExitCode;
use std::sync::Arc;

use aegis_client::connection::{Connection, LoginMethod, PasswordLogin, Public};
use aegis_client::transport::TransportKind;

fn var(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("set {name}"))
}

async fn run() -> Result<(), String> {
    let url = var("AEGIS_URL")?;
    let user = var("AEGIS_USER")?;
    let password = var("AEGIS_PASSWORD")?;
    let kind = TransportKind::parse(&std::env::var("AEGIS_TRANSPORT").unwrap_or_default(), &url)?;
    println!("server     {url} ({})", kind.as_str());

    let public = Public::connect(&url, kind)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let options = public
        .login_options()
        .await
        .map_err(|e| format!("login options: {e}"))?;
    println!("name       {}", options.server_name);
    if !matches!(options.method, LoginMethod::Password) {
        return Err(format!(
            "the server does not use password login: {:?}",
            options.method
        ));
    }

    let session = public
        .log_in(&user, &password, "aegis smoke test")
        .await
        .map_err(|e| format!("log in: {e}"))?;
    println!(
        "logged in  {} (admin: {}), session {} min",
        session.user.username,
        session.user.admin,
        session.expires_in.as_secs() / 60
    );

    let login = Arc::new(PasswordLogin::new());
    login
        .set(&url, Some(session.token))
        .map_err(|e| format!("bind login: {e}"))?;
    let conn = Connection::connect_with(&url, kind, Arc::clone(&login))
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let me = conn
        .who_am_i()
        .await
        .map_err(|e| format!("who am i: {e}"))?;
    println!("who am i   {}", me.subject);
    let info = conn.info().await.map_err(|e| format!("server info: {e}"))?;
    println!("engine     {}", info.engine_version);
    let projects = conn
        .list_projects()
        .await
        .map_err(|e| format!("list projects: {e}"))?;
    println!("projects   {}", projects.len());
    conn.log_out().await.map_err(|e| format!("log out: {e}"))?;
    println!("logged out");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => {
            println!("ok");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("smoke test failed: {message}");
            ExitCode::FAILURE
        }
    }
}
