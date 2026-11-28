import { test } from "node:test";
import assert from "node:assert/strict";
import { convert, parseCurrency } from "./currency.ts";

test("parses the supported phrasings, case-insensitively", () => {
  const usdEur = { amount: 100, from: "USD", to: "EUR" };
  assert.deepEqual(parseCurrency("100 usd to eur"), usdEur);
  assert.deepEqual(parseCurrency("100 USD in EUR"), usdEur);
  assert.deepEqual(parseCurrency("100usd eur"), usdEur);
  assert.deepEqual(parseCurrency(" 2.5 Eur usd "), { amount: 2.5, from: "EUR", to: "USD" });
});

test("anything else is not a conversion", () => {
  for (const s of ["usd to eur", "100 usd", "100 the and", "100 usd to usd", "100 usd to euro", "1,000 usd eur", "100 usd eur please", "-5 usd eur"]) {
    assert.equal(parseCurrency(s), null, s);
  }
});

test("converts with the rate and rounds to cents", () => {
  const c = { amount: 100, from: "USD", to: "EUR" };
  assert.equal(convert(c, { EUR: 0.92314 }), "92.31 EUR");
  assert.equal(convert(c, { GBP: 0.76 }), null);
  assert.equal(convert(c, { EUR: "0.9" }), null);
});
