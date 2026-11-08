import { test } from "node:test";
import assert from "node:assert/strict";
// @ts-expect-error -- plain ES module without type declarations
import {
  FILES,
  compareVersions,
  currentVersions,
  nextVersion,
  readVersion,
  setVersion,
} from "./release.mjs";

test("versions bump and compare like semver", () => {
  assert.equal(nextVersion("0.1.0", "patch"), "0.1.1");
  assert.equal(nextVersion("0.1.9", "minor"), "0.2.0");
  assert.equal(nextVersion("0.9.3", "major"), "1.0.0");
  assert.equal(nextVersion("0.1.0", "v0.2.0"), "0.2.0");
  assert.equal(nextVersion("1.0.0-beta.1", "patch"), "1.0.0");
  assert.equal(nextVersion("1.0.0", "1.1.0-rc.1"), "1.1.0-rc.1");
  assert.throws(() => nextVersion("0.2.0", "0.2.0"), /not newer/);
  assert.throws(() => nextVersion("0.2.0", "0.1.5"), /not newer/);
  assert.throws(() => nextVersion("0.2.0", "banana"), /not a version/);
  assert.ok(compareVersions("1.0.0-rc.1", "1.0.0") < 0);
  assert.ok(compareVersions("1.10.0", "1.9.0") > 0);
});

test("only the app version is rewritten in each file", () => {
  const [pkg, cargo, lock, tauri] = FILES.map(
    (f: { pattern: RegExp }) => f.pattern,
  );
  const pkgText =
    '{\n  "name": "zenith",\n  "version": "0.1.0",\n  "dependencies": { "x": { "version": "9.9.9" } }\n}\n';
  assert.equal(
    setVersion(pkgText, pkg, "0.2.0"),
    pkgText.replace('"0.1.0"', '"0.2.0"'),
  );
  const cargoText =
    '[package]\nname = "zenith"\nversion = "0.1.0"\n\n[dependencies]\nserde = { version = "1" }\n';
  assert.equal(
    readVersion(setVersion(cargoText, cargo, "0.2.0"), cargo),
    "0.2.0",
  );
  assert.ok(
    setVersion(cargoText, cargo, "0.2.0").includes('serde = { version = "1" }'),
  );
  const lockText =
    '[[package]]\nname = "zip"\nversion = "9.4.1"\n\n[[package]]\nname = "zenith"\nversion = "0.1.0"\ndependencies = []\n';
  const lockOut = setVersion(lockText, lock, "0.2.0");
  assert.ok(
    lockOut.includes('name = "zip"\nversion = "9.4.1"'),
    "other packages untouched",
  );
  assert.ok(lockOut.includes('name = "zenith"\nversion = "0.2.0"'));
  assert.equal(
    readVersion(
      '{ "productName": "Zenith", "version": "0.1.0", "plugins": {} }',
      tauri,
    ),
    "0.1.0",
  );
  assert.throws(() => setVersion("{}", pkg, "1.0.0"), /could not find/);
});

test("the repository files agree on one version", () => {
  const versions = Object.values(currentVersions());
  assert.equal(new Set(versions).size, 1, JSON.stringify(currentVersions()));
});
