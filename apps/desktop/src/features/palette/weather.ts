// Current weather typed in the palette ("weather london", "paris weather"): the parser and the
// answer, from Open-Meteo's geocoding and forecast APIs (https://open-meteo.com). No fetching here.

/** The city in "weather <city>" or "<city> weather"; null for anything else. */
export function parseWeather(input: string): string | null {
  const s = input.trim();
  const m = /^weather\s+(.+)$/i.exec(s) ?? /^(.+?)\s+weather$/i.exec(s);
  const city = m?.[1].trim() ?? "";
  // Letters (any script), spaces, and the punctuation place names use.
  return /^[\p{L}][\p{L} .'’,-]{1,59}$/u.test(city) ? city : null;
}

export const geocodeUrl = (city: string) =>
  `https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(city)}&count=1`;

export const forecastUrl = (lat: number, lon: number) =>
  `https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}&current=temperature_2m,weather_code`;

// WMO weather codes, as Open-Meteo documents them.
const CODES: [number, string][] = [
  [0, "clear sky"],
  [1, "mainly clear"],
  [2, "partly cloudy"],
  [3, "overcast"],
  [48, "fog"],
  [55, "drizzle"],
  [57, "freezing drizzle"],
  [61, "light rain"],
  [63, "rain"],
  [65, "heavy rain"],
  [67, "freezing rain"],
  [75, "snow"],
  [77, "snow grains"],
  [82, "rain showers"],
  [86, "snow showers"],
  [99, "thunderstorm"],
];

/** A short description of a WMO weather code: the first group the code falls in. */
export function weatherText(code: number): string {
  return CODES.find(([max]) => code <= max)?.[1] ?? "unknown";
}

/** "London: 12°C, light rain" from the geocoded name and the forecast's `current` block. */
export function weatherAnswer(name: string, current: { temperature_2m?: unknown; weather_code?: unknown }): string | null {
  const t = current.temperature_2m;
  const code = current.weather_code;
  if (typeof t !== "number" || typeof code !== "number") return null;
  return `${name}: ${Math.round(t)}°C, ${weatherText(code)}`;
}
