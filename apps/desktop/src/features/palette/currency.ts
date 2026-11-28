// Currency conversion typed in the palette ("100 usd to eur", "50 eur usd"): the parser and
// the answer, from Frankfurter's rates (https://frankfurter.dev). Plain logic, no fetching.

// Frankfurter's currencies; anything else is never sent, so ordinary words don't leave the browser.
const CODES = new Set(
  "AUD BGN BRL CAD CHF CNY CZK DKK EUR GBP HKD HUF IDR ILS INR ISK JPY KRW MXN MYR NOK NZD PHP PLN RON SEK SGD THB TRY USD ZAR".split(" "),
);

export interface Conversion {
  amount: number;
  from: string;
  to: string;
}

/** "100 usd to eur", "100 usd in eur" or "50 eur usd" (case-insensitive); null for anything else. */
export function parseCurrency(input: string): Conversion | null {
  const m = /^(\d+(?:\.\d+)?)\s*([a-z]{3})\s+(?:(?:to|in)\s+)?([a-z]{3})$/i.exec(input.trim());
  if (!m) return null;
  const from = m[2].toUpperCase();
  const to = m[3].toUpperCase();
  if (from === to || !CODES.has(from) || !CODES.has(to)) return null;
  return { amount: Number(m[1]), from, to };
}

/** Where the rates for one base currency come from. */
export const ratesUrl = (base: string) => `https://api.frankfurter.dev/v1/latest?from=${base}`;

/** The answer row's text, e.g. "92.31 EUR", from Frankfurter's `rates` map; null if the rate is missing. */
export function convert(c: Conversion, rates: Record<string, unknown>): string | null {
  const rate = rates[c.to];
  if (typeof rate !== "number" || !Number.isFinite(rate)) return null;
  return `${(c.amount * rate).toFixed(2)} ${c.to}`;
}
