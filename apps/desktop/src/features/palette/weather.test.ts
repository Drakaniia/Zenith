import { test } from "node:test";
import assert from "node:assert/strict";
import { parseWeather, weatherAnswer, weatherText } from "./weather.ts";

test("finds the city before or after the word weather", () => {
  assert.equal(parseWeather("weather London"), "London");
  assert.equal(parseWeather("New York weather"), "New York");
  assert.equal(parseWeather("WEATHER são paulo"), "são paulo");
});

test("anything else is not a weather query", () => {
  for (const s of ["weather", "weather 123", "london", "weatherman london", "weather https://x.com", "weather a"]) {
    assert.equal(parseWeather(s), null, s);
  }
});

test("describes WMO codes and builds the answer", () => {
  assert.equal(weatherText(0), "clear sky");
  assert.equal(weatherText(45), "fog");
  assert.equal(weatherText(61), "light rain");
  assert.equal(weatherText(95), "thunderstorm");
  assert.equal(weatherAnswer("London", { temperature_2m: 11.6, weather_code: 61 }), "London: 12°C, light rain");
  assert.equal(weatherAnswer("London", {}), null);
});
