// Run: node placement.test.mjs. Loads the pure splitAtEmbeds/placeThreads out of main.js
// (which imports host-only modules, so it cannot be imported directly).
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("./main.js", import.meta.url), "utf8");
const from = src.indexOf("function splitAtEmbeds"), to = src.indexOf("// Cairn review panel");
assert(from > 0 && to > from, "markers not found in main.js");
const { splitAtEmbeds, placeThreads } = new Function(`${src.slice(from, to)}; return { splitAtEmbeds, placeThreads };`)();

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

console.log("placement ok");
