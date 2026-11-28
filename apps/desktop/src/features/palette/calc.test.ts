import { test } from "node:test";
import assert from "node:assert/strict";
import { calc } from "./calc.ts";

test("evaluates the basic operators", () => {
  assert.equal(calc("1 + 2"), 3);
  assert.equal(calc("10-4"), 6);
  assert.equal(calc("6*7"), 42);
  assert.equal(calc("7/2"), 3.5);
  assert.equal(calc("10 % 3"), 1);
  assert.equal(calc("2^10"), 1024);
});

test("respects precedence, parentheses and associativity", () => {
  assert.equal(calc("2+3*4"), 14);
  assert.equal(calc("(2+3)*4"), 20);
  assert.equal(calc("2^3^2"), 512);
  assert.equal(calc("8-3-2"), 3);
});

test("handles decimals and unary minus", () => {
  assert.equal(calc("0.1+0.2"), 0.3);
  assert.equal(calc(".5*4"), 2);
  assert.equal(calc("-3+5"), 2);
  assert.equal(calc("2*-3"), -6);
  assert.equal(calc("-2^2"), -4);
  assert.equal(calc("2^-1"), 0.5);
});

test("needs at least one operator", () => {
  assert.equal(calc("2024"), null);
  assert.equal(calc("-5"), null);
  assert.equal(calc("(5)"), null);
  assert.equal(calc(""), null);
});

test("rejects anything that isn't pure arithmetic", () => {
  for (const s of ["a-b", "example.com", "https://x.com/1+1", "1+", "(1+2", "1+2)", "1.2.3+1", "2 x 3", "1e3+1"]) {
    assert.equal(calc(s), null, s);
  }
});

test("rejects division by zero and non-finite results", () => {
  assert.equal(calc("1/0"), null);
  assert.equal(calc("5%0"), null);
  assert.equal(calc("10^400"), null);
});
