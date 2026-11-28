// Online instant answers for the palette (currency conversion, current weather). Each answer is
// fetched only when its parser matches what was typed; failures quietly give no answer.
import { searchApi } from "./api";
import { convert, parseCurrency, ratesUrl } from "./currency";
import { forecastUrl, geocodeUrl, parseWeather, weatherAnswer } from "./weather";

export interface Instant {
  kind: "currency" | "weather";
  /** The row's text, e.g. "92.31 EUR" or "London: 12°C, light rain". */
  text: string;
  /** What the weather row searches for when picked. */
  query?: string;
}

const TTL = 60 * 60 * 1000;
const cache = new Map<string, { at: number; value: Promise<unknown> }>();

/** One API response as JSON, kept in memory for an hour (a failure is forgotten right away). */
function getJson(url: string): Promise<any> {
  const hit = cache.get(url);
  if (hit && Date.now() - hit.at < TTL) return hit.value;
  const value = searchApi.instantFetch(url).then((t) => JSON.parse(t));
  cache.set(url, { at: Date.now(), value });
  value.catch(() => cache.delete(url));
  return value;
}

/** Whether `q` would get an online answer, so the caller can skip the debounce otherwise. */
export const wantsInstant = (q: string) => parseCurrency(q) !== null || parseWeather(q) !== null;

export async function instantAnswer(q: string): Promise<Instant | null> {
  try {
    const conv = parseCurrency(q);
    if (conv) {
      const text = convert(conv, (await getJson(ratesUrl(conv.from)))?.rates ?? {});
      return text ? { kind: "currency", text } : null;
    }
    const city = parseWeather(q);
    if (city) {
      const place = (await getJson(geocodeUrl(city)))?.results?.[0];
      if (typeof place?.latitude !== "number" || typeof place?.longitude !== "number") return null;
      const current = (await getJson(forecastUrl(place.latitude, place.longitude)))?.current ?? {};
      const text = weatherAnswer(String(place.name ?? city), current);
      return text ? { kind: "weather", text, query: q } : null;
    }
  } catch {
    // Offline, rate-limited or an unexpected payload: no answer.
  }
  return null;
}
