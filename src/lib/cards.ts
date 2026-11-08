// Wallet: types shared with the backend (vault/cards.rs, card_commands.rs) and pure helpers for the
// UI. Kept free of Svelte and Tauri imports so it can be unit tested.

export interface SavedCard {
  id: string;
  label: string;
  holder: string;
  /** "visa" | "mastercard" | "amex" | "discover" | "diners" | "jcb" | "unionpay" | "card" */
  brand: string;
  last4: string;
  expMonth: number;
  expYear: number;
  hasCvc: boolean;
  created: number;
  updated: number;
  lastUsed: number;
  timesUsed: number;
}

export interface CardInput {
  /** null keeps the saved number when editing. */
  number: string | null;
  holder: string;
  expMonth: number;
  expYear: number;
  label: string;
  /** null keeps the saved security code; "" removes it. */
  cvc: string | null;
}

export interface CardSaveOffer {
  offerId: number;
  tabId: string;
  site: string;
  brand: string;
  last4: string;
  expMonth: number;
  expYear: number;
  /** The page also sent a security code; it is saved only when the user ticks the box. */
  hasCvc: boolean;
  /** A saved card would get the new expiry. */
  update: boolean;
}

const BRANDS: Record<string, string> = {
  visa: "Visa",
  mastercard: "Mastercard",
  amex: "American Express",
  discover: "Discover",
  diners: "Diners Club",
  jcb: "JCB",
  unionpay: "UnionPay",
};

export function brandName(brand: string): string {
  return BRANDS[brand] ?? "Card";
}

/** "Visa •••• 4242" */
export function cardTitle(c: { brand: string; last4: string; label?: string }): string {
  return `${c.label ? c.label + " · " : ""}${brandName(c.brand)} •••• ${c.last4}`;
}

/** "08/31" */
export function formatExpiry(month: number, year: number): string {
  return `${String(month).padStart(2, "0")}/${String(year % 100).padStart(2, "0")}`;
}

/** Whether the card's last day has passed (it works through the end of its expiry month). */
export function isExpired(month: number, year: number, now = new Date()): boolean {
  return year < now.getFullYear() || (year === now.getFullYear() && month < now.getMonth() + 1);
}

/** Digits only. */
export function digitsOf(input: string): string {
  return input.replace(/\D/g, "");
}

/** The number in the groups printed on the card: 4-6-5 for American Express, else fours. */
export function groupNumber(digits: string, brand = brandOfNumber(digits)): string {
  const d = digitsOf(digits).slice(0, 19);
  if (brand === "amex") {
    return [d.slice(0, 4), d.slice(4, 10), d.slice(10, 15)].filter(Boolean).join(" ");
  }
  return d.match(/.{1,4}/g)?.join(" ") ?? "";
}

/** The Luhn check digit test card numbers pass. */
export function luhn(digits: string): boolean {
  if (!digits) return false;
  let sum = 0;
  let double = false;
  for (let i = digits.length - 1; i >= 0; i--) {
    let d = digits.charCodeAt(i) - 48;
    if (d < 0 || d > 9) return false;
    if (double) {
      d *= 2;
      if (d > 9) d -= 9;
    }
    sum += d;
    double = !double;
  }
  return sum % 10 === 0;
}

/** The brand a (possibly partial) number belongs to, mirroring the backend. */
export function brandOfNumber(input: string): string {
  const n = digitsOf(input);
  const p = (len: number) => (n.length >= len ? parseInt(n.slice(0, len), 10) : 0);
  if (n.startsWith("4")) return "visa";
  if ((p(2) >= 51 && p(2) <= 55) || (p(4) >= 2221 && p(4) <= 2720)) return "mastercard";
  if (p(2) === 34 || p(2) === 37) return "amex";
  if (p(4) === 6011 || p(2) === 65 || (p(3) >= 644 && p(3) <= 649)) return "discover";
  if (p(4) >= 3528 && p(4) <= 3589) return "jcb";
  if ((p(3) >= 300 && p(3) <= 305) || p(2) === 36 || p(2) === 38 || p(2) === 39) return "diners";
  if (p(2) === 62) return "unionpay";
  return "card";
}

/** A problem with a card number, or "" when it looks valid. */
export function numberProblem(input: string): string {
  const d = digitsOf(input);
  if (!d) return "Enter the card number.";
  if (/[^\d\s-]/.test(input)) return "A card number has only digits.";
  if (d.length < 12 || d.length > 19) return "A card number has 12 to 19 digits.";
  if (!luhn(d)) return "That doesn't look like a valid card number.";
  return "";
}

/** A two-digit year as four (31 → 2031); four-digit years unchanged. */
export function fullYear(year: number): number {
  return year < 100 ? 2000 + year : year;
}

/** A problem with an expiry, or "" when it is acceptable. */
export function expiryProblem(month: number, year: number): string {
  if (!Number.isInteger(month) || month < 1 || month > 12) return "Enter the expiry month as 1 to 12.";
  const y = fullYear(year);
  if (!Number.isInteger(y) || y < 2000 || y > 2099) return "Enter a valid expiry year.";
  return "";
}

/** Parses "08/31", "8/2031", "0831" or "08 31" into month and year; null when it isn't one. */
export function parseExpiry(input: string): { month: number; year: number } | null {
  const m = input.trim().match(/^(\d{1,2})\s*[/\-. ]?\s*(\d{2}|\d{4})$/);
  if (!m) return null;
  const month = parseInt(m[1], 10);
  const year = fullYear(parseInt(m[2], 10));
  return expiryProblem(month, year) ? null : { month, year };
}

/** Search across the label, brand, last four digits and cardholder. Every word must match. */
export function filterCards(cards: SavedCard[], query: string): SavedCard[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return cards;
  return cards.filter((c) => {
    const hay = `${c.label} ${brandName(c.brand)} ${c.brand} ${c.last4} ${c.holder}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
