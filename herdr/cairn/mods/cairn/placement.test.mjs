// Run: node placement.test.mjs. Loads the pure splitAtEmbeds/placeThreads out of main.js
// (which imports host-only modules, so it cannot be imported directly).
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("./main.js", import.meta.url), "utf8");
const from = src.indexOf("function blocksOf"), to = src.indexOf("// Cairn review panel");
assert(from > 0 && to > from, "markers not found in main.js");
const { blocksOf, splitAtEmbeds, placeThreads, settleComments, withComments } = new Function(`${src.slice(from, to)}; return { blocksOf, splitAtEmbeds, placeThreads, settleComments, withComments };`)();

// Mirrors loadDetail: embed parts keep their span and gain pic.
const build = (md, embeds, heads) => splitAtEmbeds(md, embeds, heads).map((p) => (p.embed ? { ...p, pic: null } : p));
const th = (id, locator, revision = 2) => ({ id, revision, locator });

const md = ["# A", "text", "![x](cairn:art1)", "more", "![y](cairn:art2)"].join("\n");
const embeds = [{ line: 3 }, { line: 5 }];
const parts = build(md, embeds, [{ line: 1, path: "A" }]);
const at = (r, id) => [...r.byPart].find(([, v]) => v.some((t) => t.id === id))?.[0];

let r = placeThreads([th("onEmbed", { line_start: 3 })], parts, 2);
assert.equal(parts[at(r, "onEmbed")].embed.line, 3); assert.equal(r.rest.length, 0);

// Past the last (embed) part: falls back to the last part.
r = placeThreads([th("past", { line_start: 99 })], parts, 2);
assert.equal(at(r, "past"), parts.length - 1); assert.equal(r.rest.length, 0);

r = placeThreads([th("sec", { section_path: "A" })], parts, 2);
assert.equal(parts[at(r, "sec")].section, "A");

r = placeThreads([th("old", { line_start: 3 }, 1), th("none", undefined)], parts, 2);
assert.equal(r.byPart.size, 0); assert.equal(r.rest.length, 2);


// Block splitting: each text part is one Block with its exact lines; headings keep the section.
{
  const span = (ps) => ps.map((p) => (p.embed ? ["embed", p.start] : [p.start, p.end]));
  let ps = splitAtEmbeds("one\ntwo\n\nthree", [], []);
  assert.deepEqual(span(ps), [[1, 2], [4, 4]]); assert.equal(ps[0].text, "one\ntwo");

  ps = splitAtEmbeds(["para", "", "```js", "a", "", "b", "```", "", "tail"].join("\n"), [], []);
  assert.deepEqual(span(ps), [[1, 1], [3, 7], [9, 9]]);

  ps = splitAtEmbeds(["- a", "", "- b", "  more", "", "after"].join("\n"), [], []);
  assert.deepEqual(span(ps), [[1, 4], [6, 6]]);

  ps = splitAtEmbeds(["# A", "intro", "", "body", "## B", "x"].join("\n"), [], [{ line: 1, path: "A" }, { line: 5, path: "A/B" }]);
  assert.deepEqual(span(ps), [[1, 1], [2, 2], [4, 4], [5, 5], [6, 6]]);
  assert.deepEqual(ps.map((p) => p.section), ["A", undefined, undefined, "A/B", undefined]);

  ps = splitAtEmbeds(["a", "", "![x](cairn:art1)", "", "b"].join("\n"), [{ line: 3 }], []);
  assert.deepEqual(span(ps), [[1, 1], ["embed", 3], [5, 5]]);

  // A line Thread lands under its Block; a gap (blank) line under the preceding Block.
  ps = splitAtEmbeds("one\ntwo\n\nthree", [], []);
  r = placeThreads([th("in", { line_start: 2 }), th("gap", { line_start: 3 })], ps, 2);
  assert.equal(at(r, "in"), 0); assert.equal(at(r, "gap"), 0); assert.equal(r.rest.length, 0);
}

// A rejected comment list yields detail with empty threads and a notice, never a throw.
{
  const ok = await settleComments(Promise.resolve('{"threads":[{"id":"t"}]}'));
  assert.deepEqual(withComments({ id: "a" }, ok), { detail: { id: "a" }, threads: [{ id: "t" }] });
  const bad = await settleComments(Promise.reject(new Error("db busy\nmore")));
  const m = withComments({ id: "a", notice: "N." }, bad);
  assert.deepEqual(m.threads, []);
  assert.equal(m.detail.notice, "N. Comments unavailable: db busy");
  assert.equal(withComments({ id: "a" }, bad).detail.notice, "Comments unavailable: db busy");
  assert.ok((await settleComments(Promise.resolve("not json"))).error, "bad JSON settles with an error");
}

console.log("placement ok");
