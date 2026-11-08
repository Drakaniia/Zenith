//! Saved payment cards (the wallet).
//!
//! Cards live in the same vault file as passwords and share its key hierarchy: the card number
//! and the optional security code are sealed with the vault key (AES-256-GCM, bound to the card's
//! id, brand and last four digits), so they open only with the master password.
//!
//! The metadata kept for the locked state (DPAPI-protected, like the sites and usernames of
//! passwords) is deliberately small: label, cardholder, brand, last four digits and expiry. It is
//! enough to suggest "Visa ending 4242" on a checkout page before the master password is entered,
//! and it contains no fingerprint of the number: a keyed hash would let anyone who can read the
//! DPAPI blob recover the number by trying candidates.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::*;

const MAX_HOLDER: usize = 200;
const MIN_PAN: usize = 12;
const MAX_PAN: usize = 19;
const MAX_CVC: usize = 4;
const NO_CARD: &str = "That card no longer exists.";

// ---------------------------------------------------------------------------------------------
// Stored forms

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct CardMeta {
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    holder: String,
    brand: String,
    last4: String,
    exp_month: u8,
    exp_year: u16,
    created: i64,
    updated: i64,
    #[serde(default)]
    last_used: i64,
    #[serde(default)]
    times_used: u32,
    #[serde(default)]
    has_cvc: bool,
}

#[derive(Serialize, Deserialize)]
struct CardSecret {
    number: String,
    #[serde(default)]
    cvc: String,
}

impl Drop for CardSecret {
    fn drop(&mut self) {
        self.number.zeroize();
        self.cvc.zeroize();
    }
}

// ---------------------------------------------------------------------------------------------
// Values for the UI

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CardInfo {
    pub id: String,
    pub label: String,
    pub holder: String,
    pub brand: String,
    pub last4: String,
    pub exp_month: u8,
    pub exp_year: u16,
    pub has_cvc: bool,
    pub created: i64,
    pub updated: i64,
    pub last_used: i64,
    pub times_used: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRevealed {
    pub number: String,
    pub cvc: String,
}

impl Drop for CardRevealed {
    fn drop(&mut self) {
        self.number.zeroize();
        self.cvc.zeroize();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardInput {
    /// `None` keeps the saved number (when editing).
    pub number: Option<String>,
    #[serde(default)]
    pub holder: String,
    pub exp_month: u32,
    pub exp_year: u32,
    #[serde(default)]
    pub label: String,
    /// `None` keeps the saved security code; an empty string removes it.
    pub cvc: Option<String>,
}

impl Drop for CardInput {
    fn drop(&mut self) {
        if let Some(n) = &mut self.number {
            n.zeroize();
        }
        if let Some(c) = &mut self.cvc {
            c.zeroize();
        }
    }
}

/// What a card captured on a page is, compared with the wallet.
#[derive(Debug, PartialEq, Eq)]
pub enum CardCapture {
    /// Already saved with this expiry.
    Saved,
    /// Saved, but with another expiry: offer to update it.
    Update(String),
    New,
}

/// Everything needed to fill a card into a page.
pub struct FillCard {
    pub number: Zeroizing<String>,
    pub cvc: Zeroizing<String>,
    pub holder: String,
    pub exp_month: u8,
    pub exp_year: u16,
}

// ---------------------------------------------------------------------------------------------
// Card numbers

/// The digits of what a person typed (spaces and dashes dropped); `None` for anything else.
pub fn digits_only(input: &str) -> Option<Zeroizing<String>> {
    let mut out = Zeroizing::new(String::with_capacity(input.len()));
    for c in input.chars() {
        match c {
            '0'..='9' => out.push(c),
            ' ' | '-' | '\u{a0}' => {}
            _ => return None,
        }
    }
    Some(out)
}

pub fn luhn(number: &str) -> bool {
    let mut sum = 0u32;
    let mut double = false;
    for c in number.chars().rev() {
        let Some(mut d) = c.to_digit(10) else { return false };
        if double {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
        double = !double;
    }
    !number.is_empty() && sum % 10 == 0
}

/// A plausible card number: 12 to 19 digits that pass the Luhn check.
pub fn valid_number(number: &str) -> bool {
    (MIN_PAN..=MAX_PAN).contains(&number.len()) && luhn(number)
}

pub fn brand_of(number: &str) -> &'static str {
    let p = |n: usize| number.get(..n).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    if number.starts_with('4') {
        "visa"
    } else if (51..=55).contains(&p(2)) || (2221..=2720).contains(&p(4)) {
        "mastercard"
    } else if p(2) == 34 || p(2) == 37 {
        "amex"
    } else if p(4) == 6011 || p(2) == 65 || (644..=649).contains(&p(3)) {
        "discover"
    } else if (3528..=3589).contains(&p(4)) {
        "jcb"
    } else if (300..=305).contains(&p(3)) || p(2) == 36 || p(2) == 38 || p(2) == 39 {
        "diners"
    } else if p(2) == 62 {
        "unionpay"
    } else {
        "card"
    }
}

fn last4_of(number: &str) -> String {
    number.get(number.len().saturating_sub(4)..).unwrap_or("").to_string()
}

/// A two- or four-digit year as four digits (2025 for 25); `None` when it isn't a plausible year.
pub fn normalize_year(year: u32) -> Option<u16> {
    let y = if year < 100 { 2000 + year } else { year };
    (2000..=2099).contains(&y).then_some(y as u16)
}

fn check_expiry(month: u32, year: u32) -> Result<(u8, u16), String> {
    if !(1..=12).contains(&month) {
        return Err("Enter the expiry month as 1 to 12.".into());
    }
    let year = normalize_year(year).ok_or("Enter a valid expiry year.")?;
    Ok((month as u8, year))
}

/// An expiry as a payment form shows it: the month and year may be text such as "08 - August" or
/// "2031"; the leading number counts.
pub fn parse_expiry(month: &str, year: &str) -> Option<(u8, u16)> {
    let lead = |s: &str| -> Option<u32> {
        let digits: String = s.trim().chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    };
    check_expiry(lead(month)?, lead(year)?).ok()
}

fn check_cvc(cvc: &str) -> Result<(), String> {
    if cvc.len() > MAX_CVC || !cvc.bytes().all(|b| b.is_ascii_digit()) {
        return Err("The security code is 3 or 4 digits.".into());
    }
    Ok(())
}

fn card_aad(id: &str, brand: &str, last4: &str) -> Vec<u8> {
    format!("zenith-vault:card:v1\n{id}\n{brand}\n{last4}").into_bytes()
}

fn info(m: &CardMeta) -> CardInfo {
    CardInfo {
        id: m.id.clone(),
        label: m.label.clone(),
        holder: m.holder.clone(),
        brand: m.brand.clone(),
        last4: m.last4.clone(),
        exp_month: m.exp_month,
        exp_year: m.exp_year,
        has_cvc: m.has_cvc,
        created: m.created,
        updated: m.updated,
        last_used: m.last_used,
        times_used: m.times_used,
    }
}

// ---------------------------------------------------------------------------------------------
// The wallet

impl Vault {
    fn card_meta(&self, id: &str) -> Result<&CardMeta, String> {
        self.index.cards.iter().find(|m| m.id == id).ok_or_else(|| NO_CARD.to_string())
    }

    fn card_secret(&self, m: &CardMeta) -> Result<CardSecret, String> {
        let key = self.key()?;
        let sealed = self.file.card_secrets.get(&m.id).ok_or("This card is missing from the vault.")?;
        let plain = crypto::open(key, sealed, &card_aad(&m.id, &m.brand, &m.last4))
            .ok_or("This card could not be decrypted: the vault may be damaged.")?;
        serde_json::from_slice::<CardSecret>(&plain).map_err(|e| e.to_string())
    }

    /// Seals a card's secret under its (possibly new) id, brand and last four digits.
    fn seal_card(&mut self, id: &str, brand: &str, last4: &str, number: &str, cvc: &str) -> Result<(), String> {
        let key = self.key()?.clone();
        let secret = CardSecret { number: number.to_string(), cvc: cvc.to_string() };
        let plain = Zeroizing::new(serde_json::to_vec(&secret).map_err(|e| e.to_string())?);
        self.file.card_secrets.insert(id.to_string(), crypto::seal(&key, &plain, &card_aad(id, brand, last4)));
        Ok(())
    }

    /// Cards for the manager. Needs the vault unlocked, like the password list.
    pub fn cards_list(&mut self) -> Result<Vec<CardInfo>, String> {
        self.key()?;
        self.touch();
        let mut out: Vec<CardInfo> = self.index.cards.iter().map(info).collect();
        out.sort_by(|a, b| (a.brand.clone(), a.last4.clone()).cmp(&(b.brand.clone(), b.last4.clone())));
        Ok(out)
    }

    /// Cards that can be suggested on a checkout page. Works while locked (metadata only).
    pub fn cards_for_page(&self) -> Vec<CardInfo> {
        if !self.exists() || self.damaged.is_some() {
            return Vec::new();
        }
        let mut out: Vec<CardInfo> = self.index.cards.iter().map(info).collect();
        out.sort_by(|a, b| b.last_used.cmp(&a.last_used).then(a.brand.cmp(&b.brand)).then(a.last4.cmp(&b.last4)));
        out
    }

    pub fn card_reveal(&mut self, id: &str) -> Result<CardRevealed, String> {
        let m = self.card_meta(id)?.clone();
        let s = self.card_secret(&m)?;
        self.touch();
        Ok(CardRevealed { number: s.number.clone(), cvc: s.cvc.clone() })
    }

    fn card_insert(&mut self, number: &str, cvc: &str, holder: &str, month: u8, year: u16, label: &str) -> Result<String, String> {
        self.key()?;
        check_len(holder, MAX_HOLDER, "cardholder name")?;
        check_len(label, MAX_LABEL, "name")?;
        let (brand, last4) = (brand_of(number), last4_of(number));
        let id = new_id();
        self.seal_card(&id, brand, &last4, number, cvc)?;
        let now = now_ms();
        self.index.cards.push(CardMeta {
            id: id.clone(),
            label: label.trim().to_string(),
            holder: holder.trim().to_string(),
            brand: brand.to_string(),
            last4,
            exp_month: month,
            exp_year: year,
            created: now,
            updated: now,
            last_used: 0,
            times_used: 0,
            has_cvc: !cvc.is_empty(),
        });
        Ok(id)
    }

    pub fn card_add(&mut self, input: &CardInput) -> Result<String, String> {
        self.key()?;
        let number = digits_only(input.number.as_deref().unwrap_or("")).ok_or("A card number has only digits.")?;
        if !valid_number(&number) {
            return Err("That doesn't look like a valid card number.".into());
        }
        let (month, year) = check_expiry(input.exp_month, input.exp_year)?;
        let cvc = input.cvc.as_deref().unwrap_or("").trim();
        check_cvc(cvc)?;
        let id = self.card_insert(&number, cvc, &input.holder, month, year, &input.label)?;
        self.save()?;
        self.touch();
        Ok(id)
    }

    pub fn card_update(&mut self, id: &str, input: &CardInput) -> Result<(), String> {
        let m = self.card_meta(id)?.clone();
        let old = self.card_secret(&m)?;
        let number = match input.number.as_deref() {
            Some(n) if !n.trim().is_empty() => digits_only(n).ok_or("A card number has only digits.")?,
            _ => Zeroizing::new(old.number.clone()),
        };
        if !valid_number(&number) {
            return Err("That doesn't look like a valid card number.".into());
        }
        let (month, year) = check_expiry(input.exp_month, input.exp_year)?;
        let cvc = match &input.cvc {
            Some(c) => c.trim().to_string(),
            None => old.cvc.clone(),
        };
        check_cvc(&cvc)?;
        check_len(&input.holder, MAX_HOLDER, "cardholder name")?;
        check_len(&input.label, MAX_LABEL, "name")?;
        let (brand, last4) = (brand_of(&number), last4_of(&number));
        self.seal_card(id, brand, &last4, &number, &cvc)?;
        let m = self.index.cards.iter_mut().find(|m| m.id == id).ok_or(NO_CARD)?;
        m.brand = brand.to_string();
        m.last4 = last4;
        m.exp_month = month;
        m.exp_year = year;
        m.holder = input.holder.trim().to_string();
        m.label = input.label.trim().to_string();
        m.has_cvc = !cvc.is_empty();
        m.updated = now_ms();
        self.save()?;
        self.touch();
        Ok(())
    }

    pub fn cards_delete(&mut self, ids: &[String]) -> Result<usize, String> {
        self.key()?;
        let before = self.index.cards.len();
        self.index.cards.retain(|m| !ids.contains(&m.id));
        for id in ids {
            self.file.card_secrets.remove(id);
        }
        self.save()?;
        self.touch();
        Ok(before - self.index.cards.len())
    }

    /// Compares a card the user just submitted with the wallet (works while locked, from the
    /// brand and last four digits).
    pub fn classify_card(&self, number: &str, month: u8, year: u16) -> CardCapture {
        let (brand, last4) = (brand_of(number), last4_of(number));
        match self.index.cards.iter().find(|m| m.brand == brand && m.last4 == last4) {
            Some(m) if m.exp_month == month && m.exp_year == year => CardCapture::Saved,
            Some(m) => CardCapture::Update(m.id.clone()),
            None => CardCapture::New,
        }
    }

    /// Saves a card offered on a page: a new one, or a new expiry (and holder, security code)
    /// for `offer.update`.
    pub fn save_card(&mut self, offer: &CardOffer, save_cvc: bool) -> Result<String, String> {
        self.key()?;
        let cvc = if save_cvc { offer.cvc.as_str() } else { "" };
        check_cvc(cvc)?;
        let id = match &offer.update {
            Some(id) => {
                let m = self.card_meta(id)?.clone();
                let old = self.card_secret(&m)?;
                let keep_cvc = if save_cvc { offer.cvc.as_str() } else { old.cvc.as_str() };
                self.seal_card(id, &m.brand, &m.last4, &old.number, keep_cvc)?;
                let has_cvc = !keep_cvc.is_empty();
                let m = self.index.cards.iter_mut().find(|m| m.id == *id).ok_or(NO_CARD)?;
                m.exp_month = offer.exp_month;
                m.exp_year = offer.exp_year;
                if !offer.holder.trim().is_empty() {
                    m.holder = offer.holder.trim().to_string();
                }
                m.has_cvc = has_cvc;
                m.updated = now_ms();
                id.clone()
            }
            None => self.card_insert(&offer.number, cvc, &offer.holder, offer.exp_month, offer.exp_year, "")?,
        };
        self.save()?;
        self.touch();
        Ok(id)
    }

    /// The card to fill into a page. The caller has checked the page and the master password.
    pub fn card_for_fill(&mut self, id: &str) -> Result<FillCard, String> {
        let m = self.card_meta(id)?.clone();
        let s = self.card_secret(&m)?;
        if let Some(e) = self.index.cards.iter_mut().find(|e| e.id == id) {
            e.last_used = now_ms();
            e.times_used = e.times_used.saturating_add(1);
        }
        let _ = self.save();
        self.touch();
        Ok(FillCard {
            number: Zeroizing::new(s.number.clone()),
            cvc: Zeroizing::new(s.cvc.clone()),
            holder: m.holder,
            exp_month: m.exp_month,
            exp_year: m.exp_year,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Save offers

/// A card captured on a page, waiting for the user to save it. Never leaves Rust: the UI is told
/// only the brand, last four digits and expiry.
pub struct CardOffer {
    pub tab: String,
    pub origin: String,
    pub number: Zeroizing<String>,
    pub cvc: Zeroizing<String>,
    pub holder: String,
    pub exp_month: u8,
    pub exp_year: u16,
    pub update: Option<String>,
    at: Instant,
}

impl CardOffer {
    pub fn new(
        tab: &str,
        origin: &str,
        number: Zeroizing<String>,
        cvc: Zeroizing<String>,
        holder: &str,
        exp_month: u8,
        exp_year: u16,
        update: Option<String>,
    ) -> Self {
        Self { tab: tab.into(), origin: origin.into(), number, cvc, holder: holder.into(), exp_month, exp_year, update, at: Instant::now() }
    }
}

static CARD_OFFERS: LazyLock<Mutex<HashMap<u64, CardOffer>>> = LazyLock::new(Default::default);
const CARD_OFFER_TTL: Duration = Duration::from_secs(15 * 60);

pub fn add_card_offer(offer: CardOffer) -> u64 {
    let id = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut o) = CARD_OFFERS.lock() {
        // One offer per tab: a newer card replaces the previous one.
        o.retain(|_, x| x.tab != offer.tab);
        o.insert(id, offer);
    }
    id
}

pub fn take_card_offer(id: u64) -> Option<CardOffer> {
    CARD_OFFERS.lock().ok()?.remove(&id)
}

pub fn has_card_offer(id: u64) -> bool {
    CARD_OFFERS.lock().map(|o| o.contains_key(&id)).unwrap_or(false)
}

/// Puts an offer back (saving needed the master password first).
pub fn restore_card_offer(id: u64, offer: CardOffer) {
    if let Ok(mut o) = CARD_OFFERS.lock() {
        o.insert(id, offer);
    }
}

pub fn drop_tab_card_offers(tab: &str) {
    if let Ok(mut o) = CARD_OFFERS.lock() {
        o.retain(|_, x| x.tab != tab);
    }
}

/// Forgets offers nobody answered (called by the vault timer).
pub(super) fn prune_card_offers() {
    if let Ok(mut o) = CARD_OFFERS.lock() {
        o.retain(|_, x| x.at.elapsed() < CARD_OFFER_TTL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(name: &str) -> Vault {
        let dir = std::env::temp_dir().join(format!("zenith-cards-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Vault::open(dir.join("vault.json"), KdfParams::FAST)
    }

    fn card(number: &str, cvc: Option<&str>) -> CardInput {
        CardInput { number: Some(number.into()), holder: "Ann Lee".into(), exp_month: 8, exp_year: 2031, label: String::new(), cvc: cvc.map(str::to_string) }
    }

    const VISA: &str = "4242 4242 4242 4242";
    const AMEX: &str = "3782 822463 10005";

    #[test]
    fn numbers_brands_and_expiry() {
        assert!(luhn("4242424242424242"));
        assert!(!luhn("4242424242424241"));
        assert!(!luhn(""));
        assert_eq!(&*digits_only(VISA).unwrap(), "4242424242424242");
        assert!(digits_only("4242-4242-4242-424x").is_none());
        assert!(valid_number("378282246310005"));
        assert!(!valid_number("4242"), "too short");
        assert_eq!(brand_of("4242424242424242"), "visa");
        assert_eq!(brand_of("5555555555554444"), "mastercard");
        assert_eq!(brand_of("2223003122003222"), "mastercard");
        assert_eq!(brand_of("378282246310005"), "amex");
        assert_eq!(brand_of("6011111111111117"), "discover");
        assert_eq!(brand_of("3530111333300000"), "jcb");
        assert_eq!(brand_of("30569309025904"), "diners");
        assert_eq!(normalize_year(31), Some(2031));
        assert_eq!(normalize_year(2031), Some(2031));
        assert_eq!(normalize_year(1999), None);
        assert!(check_expiry(13, 2030).is_err());
        assert!(check_expiry(0, 2030).is_err());
        assert_eq!(parse_expiry("08 - August", "2031"), Some((8, 2031)));
        assert_eq!(parse_expiry("8", "31"), Some((8, 2031)));
        assert_eq!(parse_expiry("Month", "2031"), None);
        assert_eq!(parse_expiry("13", "2031"), None);
    }

    #[test]
    fn add_lock_reveal_and_file_stays_encrypted() {
        let mut v = temp_vault("add");
        v.setup("master master").unwrap();
        let id = v.card_add(&card(VISA, Some("123"))).unwrap();
        let list = v.cards_list().unwrap();
        assert_eq!((list[0].brand.as_str(), list[0].last4.as_str(), list[0].has_cvc), ("visa", "4242", true));

        let raw = String::from_utf8_lossy(&std::fs::read(&v.path).unwrap()).to_string();
        for needle in ["4242424242424242", "4242 4242 4242 4242", "Ann Lee", "\"123\""] {
            assert!(!raw.contains(needle), "{needle} leaked into the file");
        }
        assert!(raw.contains("\"version\": 2"), "a vault with cards needs the newer format");

        v.lock();
        assert!(v.card_reveal(&id).is_err(), "locked vaults reveal nothing");
        assert!(v.cards_list().is_err());
        assert_eq!(v.cards_for_page().len(), 1, "suggestions work while locked");
        v.unlock("master master").unwrap();
        let r = v.card_reveal(&id).unwrap();
        assert_eq!((r.number.as_str(), r.cvc.as_str()), ("4242424242424242", "123"));

        let mut again = Vault::open(v.path.clone(), KdfParams::FAST);
        assert_eq!(again.cards_for_page().len(), 1);
        again.unlock("master master").unwrap();
        assert_eq!(again.card_reveal(&id).unwrap().number.as_str(), "4242424242424242");
    }

    #[test]
    fn rejects_bad_input() {
        let mut v = temp_vault("bad");
        v.setup("master master").unwrap();
        assert!(v.card_add(&card("4242 4242 4242 4241", None)).is_err());
        assert!(v.card_add(&card("abcd", None)).is_err());
        assert!(v.card_add(&card(VISA, Some("12a"))).is_err());
        let mut late = card(VISA, None);
        late.exp_month = 14;
        assert!(v.card_add(&late).is_err());
        assert_eq!(v.cards_list().unwrap().len(), 0);
    }

    #[test]
    fn edit_reseals_and_keeps_what_is_not_given() {
        let mut v = temp_vault("edit");
        v.setup("master master").unwrap();
        let id = v.card_add(&card(VISA, Some("123"))).unwrap();
        let mut e = card(VISA, None);
        e.number = None;
        e.exp_year = 2033;
        e.label = "Work".into();
        v.card_update(&id, &e).unwrap();
        let r = v.card_reveal(&id).unwrap();
        assert_eq!((r.number.as_str(), r.cvc.as_str()), ("4242424242424242", "123"));
        assert_eq!(v.cards_list().unwrap()[0].exp_year, 2033);

        // A new number changes the brand and last four, and still opens.
        let mut n = card("378282246310005", Some(""));
        n.exp_year = 2033;
        v.card_update(&id, &n).unwrap();
        let l = v.cards_list().unwrap();
        assert_eq!((l[0].brand.as_str(), l[0].last4.as_str(), l[0].has_cvc), ("amex", "0005", false));
        assert_eq!(v.card_reveal(&id).unwrap().cvc.as_str(), "");

        assert_eq!(v.cards_delete(&[id.clone()]).unwrap(), 1);
        assert!(v.card_reveal(&id).is_err());
        assert!(v.file.card_secrets.is_empty());
    }

    #[test]
    fn a_secret_cannot_be_moved_to_another_card() {
        let mut v = temp_vault("aad");
        v.setup("master master").unwrap();
        let a = v.card_add(&card(VISA, None)).unwrap();
        let b = v.card_add(&card(AMEX, None)).unwrap();
        let sealed = v.file.card_secrets.get(&a).unwrap().clone();
        v.file.card_secrets.insert(b.clone(), sealed);
        assert!(v.card_reveal(&b).is_err());
    }

    #[test]
    fn capture_classification_and_saving() {
        let mut v = temp_vault("capture");
        v.setup("master master").unwrap();
        let id = v.card_add(&card(VISA, None)).unwrap();
        v.lock();
        // Works while locked.
        assert_eq!(v.classify_card("4242424242424242", 8, 2031), CardCapture::Saved);
        assert_eq!(v.classify_card("4242424242424242", 9, 2034), CardCapture::Update(id.clone()));
        assert_eq!(v.classify_card("378282246310005", 8, 2031), CardCapture::New);

        v.unlock("master master").unwrap();
        let offer = CardOffer::new("t", "https://shop.example", Zeroizing::new("4242424242424242".into()), Zeroizing::new("999".into()), "A. Lee", 9, 2034, Some(id.clone()));
        assert_eq!(v.save_card(&offer, false).unwrap(), id);
        let l = v.cards_list().unwrap();
        assert_eq!((l[0].exp_month, l[0].exp_year, l[0].has_cvc), (9, 2034, false), "the security code is only saved when asked");
        assert_eq!(v.classify_card("4242424242424242", 9, 2034), CardCapture::Saved);

        let fresh = CardOffer::new("t", "https://shop.example", Zeroizing::new("378282246310005".into()), Zeroizing::new("1234".into()), "", 1, 2030, None);
        let new_id = v.save_card(&fresh, true).unwrap();
        let r = v.card_reveal(&new_id).unwrap();
        assert_eq!((r.number.as_str(), r.cvc.as_str()), ("378282246310005", "1234"));
    }

    #[test]
    fn fill_counts_use_and_orders_by_recency() {
        let mut v = temp_vault("fill");
        v.setup("master master").unwrap();
        let a = v.card_add(&card(VISA, None)).unwrap();
        let b = v.card_add(&card(AMEX, Some("1234"))).unwrap();
        let f = v.card_for_fill(&b).unwrap();
        assert_eq!((f.number.as_str(), f.cvc.as_str(), f.exp_year), ("378282246310005", "1234", 2031));
        assert_eq!(v.cards_for_page()[0].id, b, "most recently used first");
        assert_ne!(v.cards_for_page()[1].id, b);
        assert!(v.card_for_fill("nope").is_err());
        v.lock();
        assert!(v.card_for_fill(&a).is_err());
    }

    #[test]
    fn passwords_only_files_keep_the_old_format() {
        let mut v = temp_vault("v1");
        v.setup("master master").unwrap();
        v.add(&EntryInput { url: "example.com".into(), username: "ann".into(), password: Some("pw".into()), label: String::new(), notes: None }).unwrap();
        let raw = std::fs::read_to_string(&v.path).unwrap();
        assert!(raw.contains("\"version\": 1"), "no cards: older Zenith versions can still open it");
        let again = Vault::open(v.path.clone(), KdfParams::FAST);
        assert!(again.index.cards.is_empty() && again.damaged.is_none());
    }

    #[test]
    fn offers_are_one_per_tab() {
        let mk = |tab: &str| CardOffer::new(tab, "https://x.example", Zeroizing::new("4242424242424242".into()), Zeroizing::new(String::new()), "", 1, 2030, None);
        let first = add_card_offer(mk("tab-offers"));
        let second = add_card_offer(mk("tab-offers"));
        assert!(!has_card_offer(first) && has_card_offer(second));
        let taken = take_card_offer(second).unwrap();
        assert!(!has_card_offer(second));
        restore_card_offer(second, taken);
        assert!(has_card_offer(second));
        drop_tab_card_offers("tab-offers");
        assert!(!has_card_offer(second));
    }
}
