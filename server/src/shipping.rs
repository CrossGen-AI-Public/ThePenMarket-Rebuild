//! Shipping on pens bought online: his list of rates, each a name, a price and where it applies
//! (in the US or outside it). Stripe Checkout cannot show a rate to some countries only, so the Buy
//! form asks where the pen ships first and the session offers only the rates for that place.
//!
//! Until he saves his own list, the site uses the three rates from his e-mail of 2026-10-12: "I
//! currently have it set for $6 uninsured USPS Ground Advantage, $15 USPS Insured Priority and $55
//! for International Insured Priority." Once saved, his list rules, even if he removes one.

use serde::{Deserialize, Serialize};

/// Stripe Checkout accepts at most five shipping options on one session.
pub const MAX_PER_REGION: usize = 5;
/// Stripe's limit on a shipping rate's display name.
pub const MAX_NAME_CHARS: usize = 100;
/// More than this for shipping one pen is a typo.
pub const MAX_CENTS: i64 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    /// Ships to an address in the United States.
    Us,
    /// Ships anywhere else Stripe can collect an address for.
    Intl,
}

impl Region {
    pub fn parse(s: &str) -> Option<Region> {
        match s.trim() {
            "us" => Some(Region::Us),
            "intl" => Some(Region::Intl),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Region::Us => "us",
            Region::Intl => "intl",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Region::Us => "In the US",
            Region::Intl => "Outside the US",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShippingOption {
    pub name: String,
    pub cents: i64,
    pub region: Region,
}

/// The rates from his e-mail of 2026-10-12, in his words.
pub fn default_list() -> Vec<ShippingOption> {
    vec![
        ShippingOption { name: "Uninsured USPS Ground Advantage".into(), cents: 600, region: Region::Us },
        ShippingOption { name: "USPS Insured Priority".into(), cents: 1500, region: Region::Us },
        ShippingOption { name: "International Insured Priority".into(), cents: 5500, region: Region::Intl },
    ]
}

/// The options for one destination, in his order, never more than Stripe takes.
pub fn for_region(list: &[ShippingOption], region: Region) -> Vec<ShippingOption> {
    list.iter().filter(|o| o.region == region).take(MAX_PER_REGION).cloned().collect()
}

pub fn ships_abroad(list: &[ShippingOption]) -> bool {
    list.iter().any(|o| o.region == Region::Intl)
}

/// One row of the admin form as typed: name, price in dollars, "us" / "intl".
pub struct Typed<'a> {
    pub name: &'a str,
    pub price: &'a str,
    pub region: &'a str,
}

fn dollars(s: &str) -> Result<i64, String> {
    let t: String = s.trim().chars().filter(|c| !matches!(c, '$' | ',' | ' ')).collect();
    let (whole, frac) = t.split_once('.').unwrap_or((t.as_str(), ""));
    if t.is_empty() || (whole.is_empty() && frac.is_empty()) || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > 2 || whole.len() > 7 {
        return Err(format!("“{}” is not a price. Try 15 or 15.00.", s.trim()));
    }
    let w: i64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| "That price is too large.".to_string())? };
    let f: i64 = if frac.is_empty() { 0 } else { format!("{frac:0<2}").parse().map_err(|_| "Bad cents.".to_string())? };
    Ok(w * 100 + f)
}

/// Check the admin form. Blank rows are skipped; a row with only a name or only a price is an error,
/// so nothing he typed is silently dropped.
pub fn parse(rows: &[Typed]) -> Result<Vec<ShippingOption>, String> {
    let mut out: Vec<ShippingOption> = vec![];
    for (i, r) in rows.iter().enumerate() {
        let name: String = r.name.split_whitespace().collect::<Vec<_>>().join(" ");
        let price = r.price.trim();
        if name.is_empty() && price.is_empty() {
            continue;
        }
        let n = i + 1;
        if name.is_empty() {
            return Err(format!("Row {n} has a price but no name."));
        }
        if price.is_empty() {
            return Err(format!("“{name}” has no price. Enter 0 for free shipping."));
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(format!("Row {n}: names can be {MAX_NAME_CHARS} characters at most (Stripe's limit)."));
        }
        if name.chars().any(char::is_control) {
            return Err(format!("Row {n}: the name has characters that cannot be shown."));
        }
        let cents = dollars(price)?;
        if cents > MAX_CENTS {
            return Err(format!("“{name}”: {price} is more than shipping one pen should cost; check for a typo."));
        }
        let region = Region::parse(r.region).ok_or_else(|| format!("“{name}”: choose in the US or outside the US."))?;
        if out.iter().any(|o| o.region == region && o.name.eq_ignore_ascii_case(&name)) {
            return Err(format!("“{name}” is listed twice for the same place."));
        }
        out.push(ShippingOption { name, cents, region });
    }
    for region in [Region::Us, Region::Intl] {
        if out.iter().filter(|o| o.region == region).count() > MAX_PER_REGION {
            return Err(format!("{}: Stripe's checkout shows at most {MAX_PER_REGION} shipping choices; remove one.", region.label()));
        }
    }
    Ok(out)
}

/// Which of the offered options a charge of `cents` was (Stripe reports the amount and a rate id, not
/// the name). Two options at the same price in one place are reported together.
pub fn chosen_name(offered: &[ShippingOption], cents: i64) -> String {
    offered.iter().filter(|o| o.cents == cents).map(|o| o.name.clone()).collect::<Vec<_>>().join(" or ")
}

/// Places that pay his US rates (his e-mail of 2026-10-14: "All U.S. Territories, Hawaii and Alaska
/// should have the same shipping rates as the rest of the country"). Hawaii, Alaska and military
/// APO/FPO addresses are `US` already; Stripe lists Puerto Rico and Guam as countries of their own.
/// The US Virgin Islands, American Samoa and the Northern Marianas are not in Stripe's list, so buyers
/// there enter a US address.
pub const US_RATES: [&str; 3] = ["US", "PR", "GU"];

/// Countries Stripe Checkout can collect a shipping address for, other than the US: the
/// `shipping_address_collection[allowed_countries]` enum of POST /v1/checkout/sessions, as listed at
/// https://docs.stripe.com/api/checkout/sessions/create?query=shipping_address_collection (read
/// 2026-10-12), with `US_RATES` removed and `ZZ` ("unknown region") left out on purpose: a pen cannot
/// be posted to an address in no country.
pub const OUTSIDE_US: [&str; 234] = [
    "AC", "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AT", "AU", "AW", "AX", "AZ", "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS", "BT", "BV", "BW", "BY", "BZ", "CA", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN", "CO", "CR", "CV", "CW", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE", "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FO", "FR", "GA", "GB", "GD", "GE", "GF", "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GW", "GY", "HK", "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IS", "IT", "JE", "JM", "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MK", "ML", "MM", "MN", "MO", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NC", "NE", "NG", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM", "PN", "PS", "PT", "PY", "QA", "RE", "RO", "RS", "RU", "RW", "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS", "ST", "SV", "SX", "SZ", "TA", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO", "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UY", "UZ", "VA", "VC", "VE", "VG", "VN", "VU", "WF", "WS", "XK", "YE", "YT", "ZA", "ZM", "ZW",
];

pub fn allowed_countries(region: Region) -> &'static [&'static str] {
    match region {
        Region::Us => &US_RATES,
        Region::Intl => &OUTSIDE_US,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(name: &'a str, price: &'a str, region: &'a str) -> Typed<'a> {
        Typed { name, price, region }
    }

    #[test]
    fn default_is_his_email() {
        let d = default_list();
        assert_eq!(d.iter().map(|o| (o.cents, o.region)).collect::<Vec<_>>(), vec![(600, Region::Us), (1500, Region::Us), (5500, Region::Intl)]);
        assert_eq!(for_region(&d, Region::Us).len(), 2);
        assert!(ships_abroad(&d));
    }

    #[test]
    fn parse_checks_every_row() {
        let ok = parse(&[row(" USPS  Insured Priority ", "$15", "us"), row("", "", "us"), row("Intl", "55.5", "intl")]).unwrap();
        assert_eq!(ok, vec![ShippingOption { name: "USPS Insured Priority".into(), cents: 1500, region: Region::Us }, ShippingOption { name: "Intl".into(), cents: 5550, region: Region::Intl }]);
        assert!(parse(&[row("A", "", "us")]).is_err());
        assert!(parse(&[row("", "6", "us")]).is_err());
        assert!(parse(&[row("A", "6.001", "us")]).is_err());
        assert!(parse(&[row("A", "abc", "us")]).is_err());
        assert!(parse(&[row("A", "6", "mars")]).is_err());
        assert!(parse(&[row("A", "2000", "us")]).is_err());
        assert!(parse(&[row("A", "6", "us"), row("a", "7", "us")]).is_err());
        assert!(parse(&[row(&"x".repeat(101), "6", "us")]).is_err());
        let six: Vec<String> = (0..6).map(|i| format!("Rate {i}")).collect();
        assert!(parse(&six.iter().map(|n| row(n, "1", "us")).collect::<Vec<_>>()).is_err());
        assert_eq!(parse(&[]).unwrap(), vec![], "an empty list is his choice too");
        assert_eq!(parse(&[row("Free", "0", "us")]).unwrap()[0].cents, 0);
    }

    #[test]
    fn countries_outside_the_us() {
        assert!(!OUTSIDE_US.contains(&"US") && !OUTSIDE_US.contains(&"ZZ"));
        assert!(OUTSIDE_US.contains(&"CA") && OUTSIDE_US.contains(&"GB") && OUTSIDE_US.contains(&"JP"));
        let mut sorted = OUTSIDE_US.to_vec();
        sorted.dedup();
        assert_eq!(sorted.len(), OUTSIDE_US.len());
        assert_eq!(allowed_countries(Region::Us), &["US", "PR", "GU"]);
        assert!(!OUTSIDE_US.contains(&"PR") && !OUTSIDE_US.contains(&"GU"), "territories pay US rates");
    }

    #[test]
    fn chosen_name_matches_the_amount() {
        let d = default_list();
        assert_eq!(chosen_name(&d, 1500), "USPS Insured Priority");
        assert_eq!(chosen_name(&d, 999), "");
    }
}
