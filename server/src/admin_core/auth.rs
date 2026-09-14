//! Passwords, codes, tokens and cookies for the admin sign-in (SPEC-0001). Pure functions plus
//! the two network calls (Have I Been Pwned range API) kept behind small async fns.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::RngCore;
use sha1::Digest as _;

pub const PASSWORD_MIN: usize = 12;
pub const PASSWORD_MAX: usize = 128;
pub const SESSION_IDLE_MIN: i64 = 60;
pub const SESSION_ABSOLUTE_HOURS: i64 = 12;
pub const STEP_UP_MIN: i64 = 10;
pub const DEVICE_DAYS: i64 = 90;
pub const CODE_MIN: i64 = 10;
pub const RESET_MIN: i64 = 30;
pub const CODE_ATTEMPTS: i32 = 5;
pub const LOCK_AFTER: i32 = 10;
pub const LOCK_WINDOW_MIN: i64 = 15;
pub const LOCK_HOURS: i64 = 1;

/// Length only, as OWASP and NIST recommend: no composition rules, no forced rotation.
pub fn password_policy(pw: &str) -> Result<(), String> {
    let n = pw.chars().count();
    if n < PASSWORD_MIN {
        return Err(format!("Use at least {PASSWORD_MIN} characters. A few words you can remember work well."));
    }
    if n > PASSWORD_MAX {
        return Err(format!("Use at most {PASSWORD_MAX} characters."));
    }
    Ok(())
}

pub fn hash_password(pw: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(pw.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("hash: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(pw: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default().verify_password(pw.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// A hash to verify against when the account does not exist, so a wrong e-mail costs the same
/// time as a wrong password (no user enumeration by timing).
pub fn dummy_hash() -> String {
    hash_password("dummy-password-for-constant-time").unwrap_or_default()
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(sha2::Sha256::digest(s.as_bytes()))
}

pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn six_digit_code() -> String {
    let mut buf = [0u8; 4];
    rand::rng().fill_bytes(&mut buf);
    let n = u32::from_le_bytes(buf) % 1_000_000;
    format!("{n:06}")
}

/// Have I Been Pwned range check: only the first five characters of the SHA-1 leave the server.
/// `None` means the check could not run (offline); the caller decides whether to allow.
pub async fn breached_count(http: &reqwest::Client, pw: &str) -> Option<u64> {
    let digest = hex::encode(sha1::Sha1::digest(pw.as_bytes())).to_uppercase();
    let (prefix, suffix) = digest.split_at(5);
    let url = format!("https://api.pwnedpasswords.com/range/{prefix}");
    let resp = http.get(&url).header("Add-Padding", "true").header("User-Agent", "ThePenMarket-admin").send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    for line in body.lines() {
        let mut it = line.trim().splitn(2, ':');
        let (Some(s), Some(c)) = (it.next(), it.next()) else { continue };
        if s.eq_ignore_ascii_case(suffix) {
            return Some(c.trim().parse::<u64>().unwrap_or(0));
        }
    }
    Some(0)
}

/// Cookie names get the `__Host-` prefix on HTTPS so the browser refuses them from any other host
/// or path; plain names on the Tailscale HTTP copy.
pub fn cookie_name(https: bool, base: &str) -> String {
    if https { format!("__Host-{base}") } else { base.to_string() }
}

pub fn set_cookie(name: &str, value: &str, https: bool, max_age_secs: i64) -> String {
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}{}", if https { "; Secure" } else { "" })
}

pub fn clear_cookie(name: &str, https: bool) -> String {
    set_cookie(name, "", https, 0)
}

/// "Mac · Safari" style device names from the user agent, for the Devices page.
pub fn device_name(ua: &str) -> String {
    let os = if ua.contains("iPhone") { "iPhone" } else if ua.contains("iPad") { "iPad" } else if ua.contains("Android") { "Android phone" } else if ua.contains("Windows") { "Windows PC" } else if ua.contains("Mac OS") { "Mac" } else if ua.contains("Linux") { "Linux" } else { "Device" };
    let browser = if ua.contains("Edg/") { "Edge" } else if ua.contains("Chrome/") && !ua.contains("Edg/") { "Chrome" } else if ua.contains("Firefox/") { "Firefox" } else if ua.contains("Safari/") { "Safari" } else { "browser" };
    format!("{os} · {browser}")
}

/// Dollars typed by a person ("$1,299.99", "1299", "1299.5") to whole cents.
pub fn parse_dollars(s: &str) -> Result<i64, String> {
    let t: String = s.trim().chars().filter(|c| !matches!(c, '$' | ',' | ' ')).collect();
    if t.is_empty() {
        return Err("Enter a price.".into());
    }
    let mut parts = t.splitn(2, '.');
    let whole = parts.next().unwrap_or("0");
    let frac = parts.next().unwrap_or("0");
    if whole.is_empty() && frac.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > 2 {
        return Err("That doesn't look like a price. Try 149.99.".into());
    }
    let w: i64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| "That price is too large.".to_string())? };
    let f: i64 = if frac.is_empty() { 0 } else { format!("{frac:0<2}").parse().map_err(|_| "Bad cents.".to_string())? };
    if w > 10_000_000 {
        return Err("That price is too large.".into());
    }
    Ok(w * 100 + f)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_and_hashing() {
        assert!(password_policy("short").is_err());
        assert!(password_policy("correct horse battery staple").is_ok());
        assert!(password_policy(&"x".repeat(129)).is_err());
        let h = hash_password("correct horse battery staple").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery staple", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("x", "not-a-hash"));
        assert!(!dummy_hash().is_empty());
    }
    #[test]
    fn tokens_codes_cookies() {
        assert_eq!(random_token(32).len(), 64);
        assert_ne!(random_token(16), random_token(16));
        let c = six_digit_code();
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|x| x.is_ascii_digit()));
        assert_eq!(sha256_hex("a").len(), 64);
        assert_eq!(cookie_name(true, "pm_admin"), "__Host-pm_admin");
        assert_eq!(cookie_name(false, "pm_admin"), "pm_admin");
        let sc = set_cookie("pm_admin", "abc", true, 60);
        assert!(sc.contains("HttpOnly") && sc.contains("SameSite=Strict") && sc.contains("Secure") && sc.contains("Max-Age=60"));
        assert!(!set_cookie("pm_admin", "abc", false, 60).contains("Secure"));
        assert!(clear_cookie("pm_admin", false).contains("Max-Age=0"));
        assert_eq!(device_name("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605 (KHTML, like Gecko) Version/17 Safari/605"), "Mac · Safari");
        assert_eq!(device_name("Mozilla/5.0 (Windows NT 10.0) Chrome/120 Safari/537 Edg/120"), "Windows PC · Edge");
        assert_eq!(device_name("Mozilla/5.0 (iPhone) Safari/605"), "iPhone · Safari");
        assert_eq!(device_name(""), "Device · browser");
    }
    #[test]
    fn dollars() {
        assert_eq!(parse_dollars("$1,299.99"), Ok(129999));
        assert_eq!(parse_dollars("1299"), Ok(129900));
        assert_eq!(parse_dollars("1299.5"), Ok(129950));
        assert_eq!(parse_dollars(" 9.99 "), Ok(999));
        assert!(parse_dollars("").is_err());
        assert!(parse_dollars("abc").is_err());
        assert!(parse_dollars("1.234").is_err());
        assert!(parse_dollars("99999999").is_err());
    }
}
