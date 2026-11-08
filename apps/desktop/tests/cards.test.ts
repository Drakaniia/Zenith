import { test } from "node:test";
import assert from "node:assert/strict";
import {
  brandName,
  brandOfNumber,
  cardTitle,
  expiryProblem,
  filterCards,
  formatExpiry,
  fullYear,
  groupNumber,
  isExpired,
  luhn,
  numberProblem,
  parseExpiry,
  type SavedCard,
} from "../src/lib/cards.ts";
import { placeSuggestions } from "../src/lib/passwords.ts";

const card = (brand: string, last4: string, label = "", holder = ""): SavedCard => ({
  id: `${brand}${last4}`,
  label,
  holder,
  brand,
  last4,
  expMonth: 8,
  expYear: 2031,
  hasCvc: false,
  created: 0,
  updated: 0,
  lastUsed: 0,
  timesUsed: 0,
});

test("luhn accepts real test numbers and rejects typos", () => {
  assert.ok(luhn("4242424242424242"));
  assert.ok(luhn("378282246310005"));
  assert.ok(!luhn("4242424242424241"));
  assert.ok(!luhn(""));
  assert.ok(!luhn("42x2"));
});

test("brands follow the number prefix, like the backend", () => {
  assert.equal(brandOfNumber("4242 4242 4242 4242"), "visa");
  assert.equal(brandOfNumber("5555555555554444"), "mastercard");
  assert.equal(brandOfNumber("2223003122003222"), "mastercard");
  assert.equal(brandOfNumber("378282246310005"), "amex");
  assert.equal(brandOfNumber("6011111111111117"), "discover");
  assert.equal(brandOfNumber("3530111333300000"), "jcb");
  assert.equal(brandOfNumber("30569309025904"), "diners");
  assert.equal(brandOfNumber("1234"), "card");
  assert.equal(brandOfNumber(""), "card");
  assert.equal(brandName("amex"), "American Express");
  assert.equal(brandName("card"), "Card");
});

test("numbers are grouped as printed and checked", () => {
  assert.equal(groupNumber("4242424242424242"), "4242 4242 4242 4242");
  assert.equal(groupNumber("42424"), "4242 4");
  assert.equal(groupNumber("378282246310005"), "3782 822463 10005");
  assert.equal(groupNumber("4242-4242 4242"), "4242 4242 4242");
  assert.equal(numberProblem("4242 4242 4242 4242"), "");
  assert.match(numberProblem(""), /Enter/);
  assert.match(numberProblem("4242 4242 4242 424x"), /only digits/);
  assert.match(numberProblem("4242"), /12 to 19/);
  assert.match(numberProblem("4242 4242 4242 4241"), /valid/);
});

test("expiries parse in the usual shapes", () => {
  assert.deepEqual(parseExpiry("08/31"), { month: 8, year: 2031 });
  assert.deepEqual(parseExpiry("8/2031"), { month: 8, year: 2031 });
  assert.deepEqual(parseExpiry("0831"), { month: 8, year: 2031 });
  assert.deepEqual(parseExpiry("08 31"), { month: 8, year: 2031 });
  assert.equal(parseExpiry("13/31"), null);
  assert.equal(parseExpiry("00/31"), null);
  assert.equal(parseExpiry("08/"), null);
  assert.equal(parseExpiry("abc"), null);
  assert.equal(fullYear(31), 2031);
  assert.equal(fullYear(2031), 2031);
  assert.equal(expiryProblem(8, 31), "");
  assert.notEqual(expiryProblem(13, 2031), "");
  assert.notEqual(expiryProblem(8, 1999), "");
  assert.equal(formatExpiry(8, 2031), "08/31");
});

test("a card works through the end of its expiry month", () => {
  const now = new Date(2026, 10, 8);
  assert.ok(!isExpired(11, 2026, now));
  assert.ok(isExpired(10, 2026, now));
  assert.ok(!isExpired(1, 2027, now));
  assert.ok(isExpired(12, 2025, now));
});

test("search matches brand, last four, label and holder", () => {
  const cards = [card("visa", "4242", "Personal", "Ann Lee"), card("amex", "0005", "Work"), card("mastercard", "4444")];
  assert.equal(filterCards(cards, "").length, 3);
  assert.deepEqual(filterCards(cards, "amex").map((c) => c.last4), ["0005"]);
  assert.deepEqual(filterCards(cards, "american").map((c) => c.last4), ["0005"]);
  assert.deepEqual(filterCards(cards, "4242 personal").map((c) => c.last4), ["4242"]);
  assert.deepEqual(filterCards(cards, "ann").map((c) => c.last4), ["4242"]);
  assert.equal(filterCards(cards, "zzz").length, 0);
  assert.equal(cardTitle(cards[0]), "Personal · Visa •••• 4242");
  assert.equal(cardTitle(cards[2]), "Mastercard •••• 4444");
});

test("the suggestion list is placed under a card field like under a login field", () => {
  const pane = { x: 0, y: 80, w: 1000, h: 700 };
  const p = placeSuggestions({ x: 100, y: 200, w: 400, h: 36 }, 1, 1, pane, { w: 300, h: 150 });
  assert.equal(p.above, false);
  assert.equal(p.y, 80 + 200 + 36 + 6);
});
