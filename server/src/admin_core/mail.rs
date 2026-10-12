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
    /// Set on form notifications so "Reply" in his mail program answers the customer.
    pub reply_to: Option<String>,
    /// An HTML version sent alongside the plain text (the buyer's receipt, which shows the pen's photo).
    pub html: Option<String>,
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
            if let Some(h) = &msg.html {
                writeln!(f, "--- HTML version ---\n{h}\n")?;
            }
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
            let mut builder = lettre::Message::builder().from(self.from.parse()?).to(msg.to.parse()?);
            if let Some(r) = msg.reply_to.as_deref().and_then(|r| r.parse().ok()) {
                builder = builder.reply_to(r);
            }
            let builder = builder.subject(msg.subject);
            let email = match msg.html {
                Some(h) => builder.multipart(lettre::message::MultiPart::alternative_plain_html(msg.body, h))?,
                None => builder.header(lettre::message::header::ContentType::TEXT_PLAIN).body(msg.body)?,
            };
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
        reply_to: None,
        body: format!("Hi Nathaniel,\n\nSomeone is signing in to ThePenMarket.com from a new device ({device}).\n\nYour code is: {code}\n\nIt works for {} minutes. If this wasn't you, ignore this e-mail and tell Brittany.\n", super::auth::CODE_MIN),
        html: None,
    }
}

pub fn reset_message(to: &str, link: &str) -> Message {
    Message {
        to: to.to_string(),
        subject: "Reset your ThePenMarket.com password".into(),
        reply_to: None,
        body: format!("Hi Nathaniel,\n\nUse this link to choose a new password. It works once and expires in {} minutes:\n\n{link}\n\nIf you didn't ask for this, ignore this e-mail; your password has not changed.\n", super::auth::RESET_MIN),
        html: None,
    }
}

pub fn lockout_message(to: &str, email: &str, ip: &str) -> Message {
    Message {
        to: to.to_string(),
        subject: "ThePenMarket.com sign-in locked for an hour".into(),
        reply_to: None,
        body: format!("The sign-in for {email} was locked for one hour after {} wrong passwords in {} minutes (from {ip}).\n\nIf that was you, wait an hour and try again, or use Forgot password. If it wasn't, nothing got in; tell Brittany.\n", super::auth::LOCK_AFTER, super::auth::LOCK_WINDOW_MIN),
        html: None,
    }
}

/// What lands in his inbox when a visitor sends a form. `fields` are (label, value) pairs.
pub fn form_message(to: &str, what: &str, id: i32, fields: &[(String, String)], has_photo: bool, inbox_url: &str) -> Message {
    let from = fields.iter().find(|(k, _)| k == "name").map(|(_, v)| v.as_str()).unwrap_or("a visitor");
    let reply_to = fields.iter().find(|(k, _)| k == "email").map(|(_, v)| v.clone()).filter(|v| v.contains('@') && !v.contains(['\r', '\n']));
    let lines: String = fields.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}: {v}\n")).collect();
    Message {
        to: to.to_string(),
        subject: format!("{what} #{id} from {}", from.replace(['\r', '\n'], " ")),
        reply_to,
        body: format!("{what} sent from ThePenMarket.com.\n\n{lines}\n{}Open it in the admin inbox: {inbox_url}\n\nReplying to this e-mail answers the person who sent it.\n", if has_photo { "A photo was attached; it is in the admin inbox.\n" } else { "" }),
        html: None,
    }
}

/// A pen sold (or refunded) through Stripe Checkout. `lines` are (label, value) pairs.
pub fn order_message(to: &str, subject: &str, lines: &[(String, String)], reply_to: &str, orders_url: &str) -> Message {
    let body: String = lines.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}: {v}\n")).collect();
    Message {
        to: to.to_string(),
        subject: subject.replace(['\r', '\n'], " "),
        reply_to: Some(reply_to.to_string()).filter(|v| v.contains('@') && !v.contains(['\r', '\n'])),
        body: format!("{subject}\n\n{body}\nEvery order is in the admin: {orders_url}\n\nReplying to this e-mail answers the buyer.\n"),
        html: None,
    }
}

/// What the buyer is sent when a pen is paid for: the order, and the pen exactly as it was described and
/// photographed when they paid (Nathaniel, 2026-10-05: if a buyer later asks about a flaw, the receipt
/// shows what the listing said).
pub struct Receipt {
    pub order_id: i32,
    pub pen: String,
    pub sku: String,
    pub total: String,
    pub shipping: String,
    pub ship_to: String,
    pub description: String,
    pub image_url: String,
    pub listing_url: String,
    pub phone: String,
    pub email: String,
}

fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

pub fn receipt_message(to: &str, r: &Receipt) -> Message {
    use crate::text::{escape, escape_attr};
    let pen = one_line(&r.pen);
    let mut facts: Vec<(&str, String)> = vec![("Order", format!("#{}", r.order_id)), ("Pen", r.pen.clone())];
    if !r.sku.is_empty() {
        facts.push(("SKU", r.sku.clone()));
    }
    if !r.total.is_empty() {
        facts.push(("Paid", r.total.clone()));
    }
    if !r.shipping.is_empty() {
        facts.push(("Shipping", r.shipping.clone()));
    }
    if !r.ship_to.is_empty() {
        facts.push(("Ship to", r.ship_to.clone()));
    }
    let contact = format!("Questions: call {} or e-mail {} and mention order #{}.", r.phone, r.email, r.order_id);
    let text_facts: String = facts.iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
    let body = format!(
        "Thank you for your order from ThePenMarket.com.\n\n{text_facts}\nThe pen as it was described when you paid:\n\n{}\n\n{}{}\n{contact}\n",
        r.description.trim(),
        if r.image_url.is_empty() { String::new() } else { format!("Photo: {}\n", r.image_url) },
        if r.listing_url.is_empty() { String::new() } else { format!("Listing: {}\n", r.listing_url) },
    );
    let rows: String = facts.iter().map(|(k, v)| format!("<tr><td style=\"padding:4px 16px 4px 0;color:#666;vertical-align:top\">{}</td><td style=\"padding:4px 0\">{}</td></tr>", k, escape(v))).collect();
    let paras: String = r.description.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).map(|p| format!("<p style=\"margin:0 0 12px\">{}</p>", escape(p).replace('\n', "<br>"))).collect();
    let photo = if r.image_url.is_empty() { String::new() } else { format!("<p><img src=\"{}\" alt=\"{}\" width=\"560\" style=\"max-width:100%;height:auto;border-radius:6px;background:#f2f1ef\"></p>", escape_attr(&r.image_url), escape_attr(&r.pen)) };
    let listing = if r.listing_url.is_empty() { String::new() } else { format!("<p><a href=\"{}\" style=\"color:#E41E26\">{}</a></p>", escape_attr(&r.listing_url), escape(&r.listing_url)) };
    let html = format!(
        "<!doctype html><html><body style=\"margin:0;padding:24px;background:#fff;color:#222;font:16px/1.55 -apple-system,Helvetica,Arial,sans-serif\"><div style=\"max-width:600px;margin:0 auto\">\
<p style=\"font-weight:700;color:#E41E26;margin:0 0 16px\">ThePenMarket.com</p><h1 style=\"font-size:22px;margin:0 0 16px\">Thank you for your order</h1>\
<table style=\"border-collapse:collapse;margin:0 0 20px\">{rows}</table>{photo}<h2 style=\"font-size:17px;margin:20px 0 10px\">The pen as it was described when you paid</h2>{paras}{listing}\
<p style=\"color:#666;font-size:14px;margin-top:24px\">{}</p></div></body></html>",
        escape(&contact)
    );
    Message { to: to.to_string(), subject: format!("Your ThePenMarket.com order #{}: {pen}", r.order_id), reply_to: Some(r.email.clone()), body, html: Some(html) }
}

/// The buyer paid for a pen that had just sold to someone else; the payment was refunded in full.
pub fn refund_message(to: &str, order_id: i32, pen: &str, amount: &str, phone: &str, email: &str) -> Message {
    let pen = one_line(pen);
    Message {
        to: to.to_string(),
        subject: format!("Your ThePenMarket.com payment for {pen} was refunded"),
        reply_to: Some(email.to_string()),
        body: format!("{pen} sold to another buyer before your payment went through, so your payment{} was refunded in full to the way you paid. Your bank decides how soon it shows.\n\nQuestions: call {phone} or e-mail {email} and mention order #{order_id}.\n", if amount.is_empty() { String::new() } else { format!(" of {amount}") }),
        html: None,
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
        let r = Receipt { order_id: 7, pen: "Parker <51>".into(), sku: "6971".into(), total: "$120.00".into(), shipping: "$12.00".into(), ship_to: "1 Main St".into(), description: "Restored.\n\nA small dent near the clip.".into(), image_url: "https://x/media/a-960.jpg".into(), listing_url: "https://x/product/p/".into(), phone: "(847) 708-5062".into(), email: "info@thepenmarket.com".into() };
        let rm = receipt_message("buyer@example.com", &r);
        assert!(rm.body.contains("A small dent near the clip.") && rm.body.contains("https://x/media/a-960.jpg") && rm.subject.contains("#7"));
        let h = rm.html.unwrap();
        assert!(h.contains("Parker &lt;51&gt;") && !h.contains("<51>") && h.contains("<img src=\"https://x/media/a-960.jpg\""));
        assert_eq!(m.name(), "log");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
