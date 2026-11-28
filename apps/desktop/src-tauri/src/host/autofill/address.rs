//! Address forms: which fields take which part of a saved address, the saved-address list under a
//! clicked field, and the values filled into the form.
//!
//! The page script only describes the form's fields (tag, type, autocomplete, name, id, label);
//! the matching happens here so it can be tested.

use std::sync::Arc;

use super::{with_tab, FieldRect, Msg, Suggest};
use crate::host::emit_tab;
use crate::host::tab::TabShared;
use crate::vault::{self, addresses::Address};

/// What a form field takes ("name", "given", "family", "org", "street", "line1", "line2", "city",
/// "region", "postal", "country", "tel", "email"), from its description as the page script sends
/// it: `[tag initial, type, autocomplete, name, id, label]`. The autocomplete attribute decides
/// when it names a field; otherwise the name, id and label are matched word by word.
pub(super) fn field_kind(f: &[String]) -> Option<&'static str> {
    let get = |i: usize| f.get(i).map(String::as_str).unwrap_or("");
    let select = get(0) == "s";
    let mut other = false;
    for token in get(2).to_ascii_lowercase().split_whitespace() {
        let kind = match token {
            "name" => "name",
            "given-name" => "given",
            "family-name" => "family",
            "organization" => "org",
            "street-address" => "street",
            "address-line1" => "line1",
            "address-line2" => "line2",
            "address-level2" => "city",
            "address-level1" => "region",
            "postal-code" => "postal",
            "country" | "country-name" => "country",
            "tel" | "tel-national" => "tel",
            "email" => "email",
            "on" | "off" | "shipping" | "billing" | "home" | "work" | "mobile" => continue,
            t if t.starts_with("section-") => continue,
            _ => {
                other = true;
                continue;
            }
        };
        return Some(kind);
    }
    // username, cc-number, one-time-code…: the page says what it is, and it is no address.
    if other {
        return None;
    }
    match get(1) {
        "email" => return Some("email"),
        "tel" => return Some("tel"),
        _ => {}
    }
    let words = words(&format!("{} {} {}", get(3), get(4), get(5)));
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    let kind = if has(&["email", "mail"]) {
        "email"
    } else if has(&["phone", "tel", "telephone", "mobile", "cell"]) {
        "tel"
    } else if has(&["zip", "zipcode", "postcode", "postal", "postalcode"]) || (has(&["post"]) && has(&["code"])) {
        "postal"
    } else if has(&["country"]) {
        "country"
    } else if has(&["state", "province", "region", "county"]) {
        "region"
    } else if has(&["city", "town", "locality", "suburb"]) {
        "city"
    } else if has(&["company", "organization", "organisation", "org", "business"]) {
        "org"
    } else if has(&["apt", "apartment", "suite", "unit"]) || (has(&["address", "addr", "street", "line"]) && has(&["2"])) {
        "line2"
    } else if has(&["address", "addr", "street"]) {
        if get(0) == "t" { "street" } else { "line1" }
    } else if has(&["first", "firstname", "fname", "given", "forename"]) {
        "given"
    } else if has(&["last", "lastname", "lname", "surname", "family"]) {
        "family"
    } else if has(&["name", "fullname"]) && !has(&["user", "username", "login", "nick", "nickname"]) {
        "name"
    } else {
        return None;
    };
    // Drop-downs only ever hold a country or a region.
    (!select || matches!(kind, "country" | "region")).then_some(kind)
}

/// "billingAddressLine2" → ["billing", "address", "line", "2"].
fn words(s: &str) -> Vec<String> {
    let mut spaced = String::new();
    let mut prev = ' ';
    for c in s.chars() {
        if !c.is_alphanumeric() {
            spaced.push(' ');
        } else {
            if (prev.is_lowercase() && c.is_uppercase()) || (prev.is_alphanumeric() && prev.is_ascii_digit() != c.is_ascii_digit()) {
                spaced.push(' ');
            }
            spaced.extend(c.to_lowercase());
        }
        prev = c;
    }
    spaced.split_whitespace().map(String::from).collect()
}

/// The list is offered when the clicked field takes part of an address and the form has at
/// least one other such field (a lone email or phone box is a newsletter, not an address form).
pub(super) fn offers(fields: &[Vec<String>], focused: usize) -> bool {
    fields.get(focused).and_then(|f| field_kind(f)).is_some() && fields.iter().filter(|f| field_kind(f).is_some()).count() >= 2
}

fn value_for(kind: &str, a: &Address, textarea: bool) -> String {
    let lines: Vec<&str> = a.street.lines().collect();
    let name = a.name.trim();
    let (given, family) = name.rsplit_once(' ').map(|(g, f)| (g.trim(), f)).unwrap_or((name, ""));
    match kind {
        "name" => name.into(),
        "given" => given.into(),
        "family" => family.into(),
        "org" => a.organization.clone(),
        "street" | "line1" if textarea => a.street.clone(),
        "street" => lines.join(", "),
        "line1" => lines.first().copied().unwrap_or("").into(),
        "line2" => lines.get(1..).unwrap_or(&[]).join(", "),
        "city" => a.city.clone(),
        "region" => a.region.clone(),
        "postal" => a.postal.clone(),
        "country" => a.country.clone(),
        "tel" => a.phone.clone(),
        "email" => a.email.clone(),
        _ => String::new(),
    }
}

/// What to fill: `(field index, "name|id" to check it is still the same field, value)`.
pub(super) fn plan(fields: &[Vec<String>], a: &Address) -> Vec<(usize, String, String)> {
    let get = |f: &[String], i: usize| f.get(i).cloned().unwrap_or_default();
    fields
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let value = value_for(field_kind(f)?, a, get(f, 0) == "t");
            (!value.is_empty()).then(|| (i, format!("{}|{}", get(f, 3), get(f, 4)), value))
        })
        .collect()
}

/// A field of an address form was clicked: the saved addresses are listed under it.
pub(super) fn on_address_field(sh: &Arc<TabShared>, origin: &str, m: &Msg) {
    let tab_id = sh.id();
    with_tab(&tab_id, |t| t.muted = false);
    if !offers(&m.fs, m.i) {
        return;
    }
    let list = vault::try_with(|v| v.addresses_for_page()).unwrap_or_default();
    if list.is_empty() {
        return;
    }
    with_tab(&tab_id, |t| {
        t.generated = None;
        t.suggesting = true;
        t.address_fields = m.fs.clone();
    });
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    emit_tab(
        &tab_id,
        "password-suggest",
        Suggest {
            tab_id: tab_id.clone(),
            site: vault::display_site(origin),
            field: "address".into(),
            new_password: false,
            rect: FieldRect { x: finite(m.x), y: finite(m.y), w: finite(m.w).max(0.0), h: finite(m.h).max(0.0) },
            dpr: if m.dpr.is_finite() && m.dpr > 0.0 { m.dpr } else { 1.0 },
            accounts: Vec::new(),
            cards: Vec::new(),
            addresses: list,
            generated: None,
            refresh: false,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(tag: &str, ty: &str, ac: &str, name: &str, id: &str, label: &str) -> Vec<String> {
        [tag, ty, ac, name, id, label].iter().map(|s| s.to_string()).collect()
    }
    fn kind(name: &str) -> Option<&'static str> {
        field_kind(&f("i", "text", "", name, "", ""))
    }

    #[test]
    fn autocomplete_decides() {
        assert_eq!(field_kind(&f("i", "text", "shipping address-line1", "x", "", "")), Some("line1"));
        assert_eq!(field_kind(&f("i", "text", "section-a billing postal-code", "", "", "")), Some("postal"));
        assert_eq!(field_kind(&f("i", "text", "given-name", "lastname", "", "")), Some("given"));
        assert_eq!(field_kind(&f("t", "textarea", "street-address", "", "", "")), Some("street"));
        assert_eq!(field_kind(&f("s", "select-one", "country", "", "", "")), Some("country"));
        assert_eq!(field_kind(&f("i", "text", "username", "name", "", "")), None, "a named non-address field");
        assert_eq!(field_kind(&f("i", "text", "cc-name", "name", "", "")), None);
        assert_eq!(field_kind(&f("i", "text", "off", "city", "", "")), Some("city"), "off falls back to the name");
    }

    #[test]
    fn names_ids_and_labels_fall_back() {
        assert_eq!(kind("firstName"), Some("given"));
        assert_eq!(kind("last_name"), Some("family"));
        assert_eq!(kind("fullName"), Some("name"));
        assert_eq!(kind("user_name"), None);
        assert_eq!(kind("username"), None);
        assert_eq!(kind("billingAddressLine2"), Some("line2"));
        assert_eq!(kind("address1"), Some("line1"));
        assert_eq!(kind("street"), Some("line1"));
        assert_eq!(kind("ship-city"), Some("city"));
        assert_eq!(kind("post_code"), Some("postal"));
        assert_eq!(kind("ZIP"), Some("postal"));
        assert_eq!(kind("stateProvince"), Some("region"));
        assert_eq!(kind("companyName"), Some("org"));
        assert_eq!(kind("phone_number"), Some("tel"));
        assert_eq!(kind("q"), None);
        assert_eq!(kind("coupon_code"), None);
        assert_eq!(field_kind(&f("i", "text", "", "f1", "", "Email address")), Some("email"));
        assert_eq!(field_kind(&f("i", "tel", "", "f2", "", "")), Some("tel"));
        assert_eq!(field_kind(&f("t", "textarea", "", "address", "", "")), Some("street"));
        assert_eq!(field_kind(&f("s", "select-one", "", "state", "", "")), Some("region"));
        assert_eq!(field_kind(&f("s", "select-one", "", "title_name", "", "")), None, "drop-downs hold a country or region only");
    }

    #[test]
    fn offered_only_in_address_forms() {
        let form = vec![f("i", "text", "", "first_name", "", ""), f("i", "text", "", "city", "", ""), f("i", "text", "", "q", "", "")];
        assert!(offers(&form, 0));
        assert!(!offers(&form, 2), "the clicked field takes no address");
        assert!(!offers(&form, 9));
        assert!(!offers(&[f("i", "email", "", "newsletter", "", "")], 0), "a lone email box");
    }

    #[test]
    fn plan_fills_every_matching_field() {
        let a = Address {
            name: "Ann Marie Lee".into(),
            street: "1 Main St\nApt 4\nBack door".into(),
            city: "Springfield".into(),
            country: "US".into(),
            ..Address::default()
        };
        let form = vec![
            f("i", "text", "", "fname", "a", ""),
            f("i", "text", "", "lname", "", ""),
            f("i", "text", "address-line1", "", "", ""),
            f("i", "text", "address-line2", "", "", ""),
            f("i", "text", "", "zip", "", ""),
            f("s", "select-one", "country", "c", "", ""),
            f("i", "text", "", "q", "", ""),
        ];
        let p = plan(&form, &a);
        let got: Vec<(usize, &str, &str)> = p.iter().map(|(i, k, v)| (*i, k.as_str(), v.as_str())).collect();
        assert_eq!(
            got,
            [(0, "fname|a", "Ann Marie"), (1, "lname|", "Lee"), (2, "|", "1 Main St"), (3, "|", "Apt 4, Back door"), (5, "c|", "US")],
            "empty values (zip) and other fields are left alone"
        );
        let street = plan(&[f("t", "textarea", "street-address", "s", "", ""), f("i", "text", "street-address", "s2", "", "")], &a);
        assert_eq!(street[0].2, "1 Main St\nApt 4\nBack door");
        assert_eq!(street[1].2, "1 Main St, Apt 4, Back door");
        let one = Address { name: "Cher".into(), ..Address::default() };
        assert_eq!(plan(&[f("i", "text", "given-name", "", "", ""), f("i", "text", "family-name", "", "", "")], &one).len(), 1);
    }
}
