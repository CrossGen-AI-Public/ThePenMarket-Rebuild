//! Outgoing mail for sign-in codes, reset links and lockout alerts. With `SMTP_URL` set, lettre
//! sends through that server; otherwise every message is appended to `<uploads_dir>/../admin-mail.log`
//! so the demo works without a mail account (the runbook says where to read the code).

use async_trait_shim::BoxFuture;
use std::path::PathBuf;

pub mod async_trait_shim {
    pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
}

pub struct Message {
    pub to: String,
    pub subject: String,
    pub body: String,
}

pub trait Mailer: Send + Sync {
    fn name(&self) -> &'static str;
    fn send<'a>(&'a self, msg: Message) -> BoxFuture<'a, anyhow::Result<()>>;
}

pub struct LogMailer {
    pub path: PathBuf,
}

impl Mailer for LogMailer {
    fn name(&self) -> &'static str {
        "log"
    }
    fn send<'a>(&'a self, msg: Message) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            use std::io::Write;
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
            writeln!(f, "=== {} | To: {} | Subject: {}\n{}\n", chrono::Utc::now().to_rfc3339(), msg.to, msg.subject, msg.body)?;
            tracing::info!(to = %msg.to, subject = %msg.subject, "admin mail written to log (no SMTP configured)");
            Ok(())
        })
    }
}

pub struct SmtpMailer {
    pub transport: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    pub from: String,
}

impl Mailer for SmtpMailer {
    fn name(&self) -> &'static str {
        "smtp"
    }
    fn send<'a>(&'a self, msg: Message) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            use lettre::AsyncTransport;
            let email = lettre::Message::builder()
                .from(self.from.parse()?)
                .to(msg.to.parse()?)
                .subject(msg.subject)
                .header(lettre::message::header::ContentType::TEXT_PLAIN)
                .body(msg.body)?;
            self.transport.send(email).await?;
            Ok(())
        })
    }
}

/// Pick the mailer from the environment: `SMTP_URL` (e.g. `smtps://user:pass@smtp.example.com:465`)
/// plus `MAIL_FROM`; otherwise the log.
pub fn from_env(log_path: PathBuf) -> std::sync::Arc<dyn Mailer> {
    let url = std::env::var("SMTP_URL").unwrap_or_default();
    let from = std::env::var("MAIL_FROM").unwrap_or_else(|_| "ThePenMarket.com <info@thepenmarket.com>".into());
    if !url.is_empty() {
        match lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::from_url(&url) {
            Ok(b) => return std::sync::Arc::new(SmtpMailer { transport: b.build(), from }),
            Err(e) => tracing::warn!("SMTP_URL invalid ({e}); falling back to the mail log"),
        }
    }
    std::sync::Arc::new(LogMailer { path: log_path })
}

pub fn code_message(to: &str, code: &str, device: &str) -> Message {
    Message {
        to: to.to_string(),
        subject: format!("Your ThePenMarket.com sign-in code: {code}"),
        body: format!("Hi Nathaniel,\n\nSomeone is signing in to ThePenMarket.com from a new device ({device}).\n\nYour code is: {code}\n\nIt works for {} minutes. If this wasn't you, ignore this e-mail and tell Brittany.\n", super::auth::CODE_MIN),
    }
}

pub fn reset_message(to: &str, link: &str) -> Message {
    Message {
        to: to.to_string(),
        subject: "Reset your ThePenMarket.com password".into(),
        body: format!("Hi Nathaniel,\n\nUse this link to choose a new password. It works once and expires in {} minutes:\n\n{link}\n\nIf you didn't ask for this, ignore this e-mail; your password has not changed.\n", super::auth::RESET_MIN),
    }
}

pub fn lockout_message(to: &str, email: &str, ip: &str) -> Message {
    Message {
        to: to.to_string(),
        subject: "ThePenMarket.com sign-in locked for an hour".into(),
        body: format!("The sign-in for {email} was locked for one hour after {} wrong passwords in {} minutes (from {ip}).\n\nIf that was you, wait an hour and try again, or use Forgot password. If it wasn't, nothing got in; tell Brittany.\n", super::auth::LOCK_AFTER, super::auth::LOCK_WINDOW_MIN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn log_mailer_appends() {
        let dir = std::env::temp_dir().join(format!("pm-mail-{}", std::process::id()));
        let m = LogMailer { path: dir.join("admin-mail.log") };
        m.send(code_message("n@example.com", "123456", "Mac · Safari")).await.unwrap();
        m.send(reset_message("n@example.com", "https://x/admin/reset/abc/")).await.unwrap();
        m.send(lockout_message("n@example.com", "n@example.com", "1.2.3.4")).await.unwrap();
        let t = std::fs::read_to_string(dir.join("admin-mail.log")).unwrap();
        assert!(t.contains("123456") && t.contains("/admin/reset/abc/") && t.contains("locked"));
        assert_eq!(m.name(), "log");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
