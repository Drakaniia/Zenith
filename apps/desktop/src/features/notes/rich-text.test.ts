import { test } from "node:test";
import assert from "node:assert/strict";
import { formatCommand, noteText, normalizeRuns, parseInline, parseNote, serializeNote, type Block, type Run } from "./rich-text.ts";

const roundTrip = (md: string) => serializeNote(parseNote(md));

test("plain notes written before formatting read back unchanged", () => {
  for (const md of [
    "Call Anna\nabout Friday",
    "",
    "a\n\n\nb",
    "5 * 3 = 15",
    "price: 3*",
    "C:\\Users\\me\\notes.txt",
    "ends with a backslash \\",
    "  indented line",
  ]) {
    assert.equal(noteText(md), md.replace(/\r/g, ""), md);
    assert.equal(roundTrip(md), md, md);
  }
});

test("bold, italic and both", () => {
  assert.deepEqual(parseInline("a **b** *c* ***d***"), [
    { text: "a " },
    { text: "b", bold: true },
    { text: " " },
    { text: "c", italic: true },
    { text: " " },
    { text: "d", bold: true, italic: true },
  ]);
});

test("asterisks that can't be markers stay text", () => {
  assert.deepEqual(parseInline("5 * 3 * 2"), [{ text: "5 * 3 * 2" }]);
  assert.deepEqual(parseInline("** not bold **"), [{ text: "** not bold **" }]);
  assert.deepEqual(parseInline("*open only"), [{ text: "*open only" }]);
  assert.deepEqual(parseInline("\\*not italic\\*"), [{ text: "*not italic*" }]);
});

test("lines starting with a dash or a number are list items", () => {
  const b = parseNote("- milk\n* eggs\n3. third\n4. fourth\nplain");
  assert.deepEqual(
    b.map((x) => [x.kind, x.n]),
    [["ul", undefined], ["ul", undefined], ["ol", 3], ["ol", 4], ["p", undefined]],
  );
  assert.equal(serializeNote(b), "- milk\n- eggs\n3. third\n4. fourth\nplain");
});

test("numbered lists count on from their first number", () => {
  const blocks: Block[] = [
    { kind: "ol", n: 1, runs: [{ text: "a" }] },
    { kind: "ol", runs: [{ text: "b" }] },
    { kind: "p", runs: [] },
    { kind: "ol", runs: [{ text: "c" }] },
  ];
  assert.equal(serializeNote(blocks), "1. a\n2. b\n\n1. c");
});

test("a paragraph that looks like a list item stays a paragraph", () => {
  for (const text of ["- not a bullet", "1. not a number", "* not a bullet"]) {
    const md = serializeNote([{ kind: "p", runs: [{ text }] }]);
    const back = parseNote(md);
    assert.equal(back[0].kind, "p", md);
    assert.equal(back[0].runs.map((r) => r.text).join(""), text);
  }
});

test("every mix of neighbouring formats survives a round trip", () => {
  const formats: Partial<Run>[] = [{}, { bold: true }, { italic: true }, { bold: true, italic: true }];
  for (const a of formats) {
    for (const b of formats) {
      for (const sep of ["", " "]) {
        const runs = normalizeRuns([{ text: "a", ...a }, { text: sep }, { text: "b", ...b }]);
        const md = serializeNote([{ kind: "p", runs }]);
        assert.deepEqual(parseNote(md)[0].runs, runs, `${JSON.stringify(runs)} -> ${md}`);
      }
    }
  }
});

test("literal asterisks and backslashes inside formatting survive", () => {
  const runs: Run[] = [{ text: "a*b\\", bold: true }, { text: " x\\*y" }];
  const md = serializeNote([{ kind: "p", runs }]);
  assert.deepEqual(parseNote(md)[0].runs, runs, md);
});

test("spaces at the edges of formatting move outside it", () => {
  assert.deepEqual(normalizeRuns([{ text: " bold ", bold: true }, { text: "" }]), [
    { text: " " },
    { text: "bold", bold: true },
    { text: " " },
  ]);
  assert.equal(serializeNote([{ kind: "p", runs: [{ text: "a" }, { text: " b ", bold: true }, { text: "c" }] }]), "a **b** c");
});

test("plain text drops the markup; with markers lists keep bullets and numbers", () => {
  const md = "**Plan**\n- *milk*\n- eggs\n2. one\n3. two";
  assert.equal(noteText(md), "Plan\nmilk\neggs\none\ntwo");
  assert.equal(noteText(md, true), "Plan\n• milk\n• eggs\n2. one\n3. two");
});

test("formatting keys", () => {
  const k = (vk: number, shift = false, ctrl = true, alt = false) => formatCommand({ ctrl, alt, shift, vk });
  assert.equal(k(0x42), "bold");
  assert.equal(k(0x49), "italic");
  assert.equal(k(0x38, true), "bullets");
  assert.equal(k(0x37, true), "numbers");
  assert.equal(k(0x42, true), null);
  assert.equal(k(0x42, false, false), null);
  assert.equal(k(0x42, false, true, true), null);
});
