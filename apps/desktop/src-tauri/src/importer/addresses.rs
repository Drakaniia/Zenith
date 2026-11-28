//! Addresses saved in Chromium-family browsers, read from the profile's `Web Data` database for
//! the vault (see `vault::addresses`).
//!
//! Chromium has stored them three ways: `autofill_profiles` with `autofill_profile_names`,
//! `_emails` and `_phones` (until 2023); then one row per field in `local_addresses_type_tokens`;
//! and now `address_type_tokens` (local and account addresses together). The newest found wins.

use std::collections::HashMap;

use rusqlite::Connection;

use super::{scan, with_copy};
use crate::vault::addresses::Address;

/// Reads the saved addresses of a profile found by `detect`.
pub fn read(id: &str) -> Result<Vec<Address>, String> {
    let found = scan()
        .into_iter()
        .find(|f| f.source.id == id && f.source.addresses)
        .ok_or("That browser profile has no addresses to import.")?;
    let name = &found.source.name;
    with_copy(&found.dir.join("Web Data"), chromium_addresses).map_err(|_| format!("Couldn't read {name}'s addresses. Close {name} and try again."))
}

pub fn chromium_addresses(c: &Connection) -> rusqlite::Result<Vec<Address>> {
    let tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?
        .query_map([], |r| r.get(0))?
        .filter_map(Result::ok)
        .collect();
    let has = |t: &str| tables.iter().any(|x| x == t);
    for t in ["address_type_tokens", "local_addresses_type_tokens"] {
        if has(t) {
            let found = from_tokens(c, t)?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    if has("autofill_profiles") && has("autofill_profile_names") {
        return from_profiles(c);
    }
    Ok(Vec::new())
}

/// One row per (address, field type). The types are Chromium's `FieldType` numbers.
fn from_tokens(c: &Connection, table: &str) -> rusqlite::Result<Vec<Address>> {
    let mut stmt = c.prepare(&format!("SELECT guid, type, COALESCE(value, '') FROM {table}"))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, Address> = HashMap::new();
    for (guid, kind, value) in rows.filter_map(Result::ok) {
        let a = by_id.entry(guid.clone()).or_insert_with(|| {
            order.push(guid);
            Address::default()
        });
        let field = match kind {
            7 => &mut a.name,
            9 => &mut a.email,
            14 => &mut a.phone,
            33 => &mut a.city,
            34 => &mut a.region,
            35 => &mut a.postal,
            36 => &mut a.country,
            60 => &mut a.organization,
            77 => &mut a.street,
            _ => continue,
        };
        *field = value;
    }
    Ok(order.into_iter().filter_map(|g| by_id.remove(&g)).collect())
}

fn from_profiles(c: &Connection) -> rusqlite::Result<Vec<Address>> {
    let mut stmt = c.prepare(
        "SELECT
            (SELECT COALESCE(first_name, '') || ' ' || COALESCE(middle_name, '') || ' ' || COALESCE(last_name, '')
               FROM autofill_profile_names n WHERE n.guid = p.guid LIMIT 1),
            COALESCE(company_name, ''), COALESCE(street_address, ''), COALESCE(city, ''), COALESCE(state, ''),
            COALESCE(zipcode, ''), COALESCE(country_code, ''),
            (SELECT number FROM autofill_profile_phones x WHERE x.guid = p.guid LIMIT 1),
            (SELECT email FROM autofill_profile_emails e WHERE e.guid = p.guid LIMIT 1)
         FROM autofill_profiles p",
    )?;
    let rows = stmt.query_map([], |r| {
        let s = |i: usize| -> rusqlite::Result<String> { Ok(r.get::<_, Option<String>>(i)?.unwrap_or_default()) };
        Ok(Address {
            name: s(0)?.split_whitespace().collect::<Vec<_>>().join(" "),
            organization: s(1)?,
            street: s(2)?,
            city: s(3)?,
            region: s(4)?,
            postal: s(5)?,
            country: s(6)?,
            phone: s(7)?,
            email: s(8)?,
            ..Address::default()
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_token_tables() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE local_addresses (guid TEXT PRIMARY KEY, use_count INTEGER);
             CREATE TABLE local_addresses_type_tokens (guid TEXT, type INTEGER, value TEXT, verification_status INTEGER, observations BLOB);
             INSERT INTO local_addresses_type_tokens VALUES ('a', 7, 'Ann Lee', 0, NULL), ('a', 77, '1 Main St\nApt 4', 0, NULL),
                 ('a', 33, 'Springfield', 0, NULL), ('a', 35, '12345', 0, NULL), ('a', 36, 'US', 0, NULL),
                 ('a', 9, 'ann@example.com', 0, NULL), ('a', 14, '+1 555 0100', 0, NULL), ('a', 3, 'Ann', 0, NULL),
                 ('b', 60, 'Acme', 0, NULL), ('b', 34, 'CA', 0, NULL), ('b', 7, NULL, 0, NULL);",
        )
        .unwrap();
        let list = chromium_addresses(&c).unwrap();
        assert_eq!(list.len(), 2);
        let a = &list[0];
        assert_eq!((a.name.as_str(), a.street.as_str(), a.city.as_str()), ("Ann Lee", "1 Main St\nApt 4", "Springfield"));
        assert_eq!((a.postal.as_str(), a.country.as_str(), a.email.as_str(), a.phone.as_str()), ("12345", "US", "ann@example.com", "+1 555 0100"));
        assert_eq!((list[1].organization.as_str(), list[1].region.as_str(), list[1].name.as_str()), ("Acme", "CA", ""));

        // The newest table wins when it holds addresses.
        c.execute_batch("CREATE TABLE address_type_tokens (guid TEXT, type INTEGER, value TEXT); INSERT INTO address_type_tokens VALUES ('n', 33, 'Paris');")
            .unwrap();
        let list = chromium_addresses(&c).unwrap();
        assert_eq!((list.len(), list[0].city.as_str()), (1, "Paris"));
    }

    #[test]
    fn reads_the_old_profile_tables() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE autofill_profiles (guid TEXT, company_name TEXT, street_address TEXT, city TEXT, state TEXT,
                 zipcode TEXT, country_code TEXT, date_modified INTEGER);
             CREATE TABLE autofill_profile_names (guid TEXT, first_name TEXT, middle_name TEXT, last_name TEXT, full_name TEXT);
             CREATE TABLE autofill_profile_emails (guid TEXT, email TEXT);
             CREATE TABLE autofill_profile_phones (guid TEXT, number TEXT);
             INSERT INTO autofill_profiles VALUES ('g', 'Acme', '1 Main St', 'Springfield', 'IL', '62701', 'US', 0);
             INSERT INTO autofill_profile_names VALUES ('g', 'Ann', NULL, 'Lee', 'Ann Lee');
             INSERT INTO autofill_profile_emails VALUES ('g', 'ann@example.com');",
        )
        .unwrap();
        let list = chromium_addresses(&c).unwrap();
        assert_eq!(list.len(), 1);
        let a = &list[0];
        assert_eq!((a.name.as_str(), a.organization.as_str(), a.region.as_str()), ("Ann Lee", "Acme", "IL"));
        assert_eq!((a.email.as_str(), a.phone.as_str()), ("ann@example.com", ""));
        assert!(chromium_addresses(&Connection::open_in_memory().unwrap()).unwrap().is_empty());
    }
}
