//! `cargo run --bin admin -- <command>`: the admin account is created here, never by a sign-up form.
//!
//!   admin create <email> [--name "Nathaniel"]   password read from ADMIN_PASSWORD or the terminal
//!   admin set-password <email>                  same, for an existing account
//!   admin unlock <email>                        clear a lockout
//!   admin forget-devices <email>                sign out everywhere and forget remembered devices
//!
//! The password is checked against the length policy and, when the network allows, against the
//! Have I Been Pwned range API (only five characters of a hash leave this machine). Pass
//! --allow-breached to skip that check offline (not recommended).

use std::io::{BufRead, Write};
use thepenmarket::admin_core::{auth, store};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let email = args.get(1).cloned().unwrap_or_default();
    if cmd.is_empty() || (cmd != "help" && email.is_empty()) {
        eprintln!("usage: admin <create|set-password|unlock|forget-devices> <email> [--name NAME] [--allow-breached]");
        std::process::exit(2);
    }
    let cfg = thepenmarket::config::load()?;
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&cfg.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let name = args.iter().position(|a| a == "--name").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "Nathaniel".into());
    let allow_breached = args.iter().any(|a| a == "--allow-breached");
    match cmd {
        "create" | "set-password" => {
            let pw = read_password()?;
            if let Err(e) = auth::password_policy(&pw) {
                anyhow::bail!("{e}");
            }
            let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?;
            match auth::breached_count(&http, &pw).await {
                Some(0) => println!("breach check: not found in known breaches."),
                Some(n) => anyhow::bail!("that password appears in known breaches ({n} times). Choose another."),
                None if allow_breached => println!("breach check skipped (offline)."),
                None => anyhow::bail!("could not reach the breach-check service; retry with a connection or pass --allow-breached"),
            }
            let hash = auth::hash_password(&pw)?;
            if cmd == "create" {
                let id = store::upsert_account(&pool, &email, &name, &hash).await?;
                println!("account #{id} ready for {}", email.to_lowercase());
            } else {
                let acc = store::account_by_email(&pool, &email).await?.ok_or_else(|| anyhow::anyhow!("no account for {email}"))?;
                store::set_password(&pool, acc.id, &hash).await?;
                store::revoke_all_sessions(&pool, acc.id).await?;
                println!("password updated; all sessions signed out.");
            }
        }
        "unlock" => {
            let acc = store::account_by_email(&pool, &email).await?.ok_or_else(|| anyhow::anyhow!("no account for {email}"))?;
            store::unlock(&pool, acc.id).await?;
            println!("unlocked.");
        }
        "forget-devices" => {
            let acc = store::account_by_email(&pool, &email).await?.ok_or_else(|| anyhow::anyhow!("no account for {email}"))?;
            for d in store::devices(&pool, acc.id).await? {
                store::revoke_device(&pool, acc.id, d.id).await?;
            }
            store::revoke_all_sessions(&pool, acc.id).await?;
            println!("devices forgotten and sessions signed out.");
        }
        _ => anyhow::bail!("unknown command {cmd}"),
    }
    Ok(())
}

fn read_password() -> anyhow::Result<String> {
    if let Ok(pw) = std::env::var("ADMIN_PASSWORD") {
        return Ok(pw);
    }
    print!("New password (at least {} characters; it will not be echoed on some terminals): ", auth::PASSWORD_MIN);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}
