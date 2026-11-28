import { test } from "node:test";
import assert from "node:assert/strict";
import { addressLine, addressTitle, emptyAddress } from "./addresses.ts";

test("addresses are titled and summarized on one line", () => {
  const a = { ...emptyAddress(), name: "Ann Lee", street: "1 Main St\nApt 4", city: "Springfield", region: "IL", postal: "62701", country: "US" };
  assert.equal(addressTitle(a), "Ann Lee");
  assert.equal(addressLine(a), "1 Main St, Apt 4, Springfield, IL 62701, US");
  assert.equal(addressTitle({ ...emptyAddress(), street: "9 Elm Rd\nUnit 2" }), "9 Elm Rd");
  assert.equal(addressTitle({ ...emptyAddress(), email: "x@example.com" }), "x@example.com");
  assert.equal(addressTitle(emptyAddress()), "Address");
  assert.equal(addressLine({ ...emptyAddress(), postal: "10115" }), "10115");
  assert.equal(addressLine(emptyAddress()), "");
});
