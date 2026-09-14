//! Money is integer cents everywhere. Formatting is US dollars, the only currency he sells in.

/// Parse "$1,199.99", "225. 00", "$2,500.00 OBO", "125" into cents. Returns None when no number is found.
pub fn parse_cents(s: &str) -> Option<i64> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    if cleaned.is_empty() {
        return None;
    }
    // "225. 00" becomes "225.00" once spaces are dropped; "1,199.99" becomes "1199.99".
    let mut parts = cleaned.splitn(2, '.');
    let whole: i64 = parts.next().unwrap_or("0").parse().ok()?;
    let frac = parts.next().unwrap_or("");
    let frac_cents: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac[..2].parse().ok()?,
    };
    Some(whole * 100 + frac_cents)
}

/// "$1,199.99"; whole dollars still show cents ("$125.00"), matching his catalog.
pub fn fmt_cents(cents: i64) -> String {
    let dollars = cents / 100;
    let rem = cents % 100;
    let mut s = dollars.to_string();
    let mut out = String::new();
    let n = s.len();
    for (i, ch) in s.drain(..).enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("${out}.{rem:02}")
}

/// "$1,200" style for ranges and counts where cents are noise.
pub fn fmt_dollars(cents: i64) -> String {
    let full = fmt_cents(cents);
    full.trim_end_matches(".00").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_his_price_formats() {
        assert_eq!(parse_cents("$1,199.99"), Some(119_999));
        assert_eq!(parse_cents("225. 00"), Some(22_500));
        assert_eq!(parse_cents("$2,500.00 OBO"), Some(250_000));
        assert_eq!(parse_cents("125"), Some(12_500));
        assert_eq!(parse_cents("$1199"), Some(119_900));
        assert_eq!(parse_cents("call"), None);
    }
    #[test]
    fn formats() {
        assert_eq!(fmt_cents(119_999), "$1,199.99");
        assert_eq!(fmt_cents(12_500), "$125.00");
        assert_eq!(fmt_cents(999), "$9.99");
        assert_eq!(fmt_dollars(1_649_999), "$16,499.99");
        assert_eq!(fmt_dollars(15_000), "$150");
    }
}
