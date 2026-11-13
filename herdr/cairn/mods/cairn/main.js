import { View, div, image } from "gpui-kit";
import { v_flex, h_flex, Scrollbar, TextView, InputState, Input } from "gpui-base";
import { spaces, panes, focused, exec, appHome, run, theme, settings, notify, nextCommand } from "herdr";
import * as fs from "fs/promises";

// herdr draws no http or data: images (TextView ignores them), only image(path) for a file
// under the app folder. So image artifacts, svg diagrams and markdown cairn: embeds are
// fetched into this cache once per revision (revisions are immutable) and drawn with image().
const CACHE = "data/cairn-cache";

// Natural size from the file header: PNG IHDR, or an svg's width/height/viewBox.
async function naturalSize(abs) {
  try {
    if (abs.endsWith(".svg")) {
      const s = await fs.readFile(abs, "utf8"), tag = /<svg\b[^>]*>/i.exec(s)?.[0] ?? "";
      const num = (n) => { const m = new RegExp(`\\b${n}="([\\d.]+)(?:px)?"`).exec(tag); return m ? +m[1] : null; };
      let w = num("width"), h = num("height");
      const vb = /viewBox="[\d.\-]+[ ,]+[\d.\-]+[ ,]+([\d.]+)[ ,]+([\d.]+)"/.exec(tag);
      if ((!w || !h) && vb) { w = +vb[1]; h = +vb[2]; }
      return w && h ? { w, h } : null;
    }
    const b = await fs.readFile(abs);
    if (b[0] === 0x89 && b[1] === 0x50) {
      const u32 = (o) => ((b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]) >>> 0;
      return { w: u32(16), h: u32(20) };
    }
  } catch {}
  return null;
}

// herdr's image loader does not resolve CSS custom properties, and Pele colors everything
// through them (--_fg:var(--pele-fg,#1f1f1f); fill="var(--_fg)"), which draws black boxes.
// Replace each var() with its literal fallback. Cairn's own SVG stays themeable for the web UI.
function flattenCssVars(svg) {
  const defs = {};
  for (const m of svg.matchAll(/(--[\w-]+):\s*([^;"]+)/g)) defs[m[1]] = m[2].trim();
  const resolve = (text, depth = 0) => depth > 8 ? text : text.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*((?:[^()]|\([^()]*\))*))?\)/g,
    (_m, name, fallback) => resolve(defs[name] && !defs[name].includes(`var(${name}`) ? defs[name] : fallback ?? "inherit", depth + 1));
  return resolve(svg);
}

// Splits markdown into text and own-line embeds using the lines `cairn embeds` reports.
// An embed sharing its line with other text stays in the text (herdr draws no inline images).
function splitAtEmbeds(md, embeds) {
  const lines = md.split("\n"), parts = []; let from = 0;
  for (const e of embeds) {
    if (!/^\s*!\[[^\]]*\]\(cairn:[^)]*\)\s*$/.test(lines[e.line - 1] ?? "")) continue;
    if (e.line - 1 > from) parts.push({ text: lines.slice(from, e.line - 1).join("\n") });
    parts.push({ embed: e }); from = e.line;
  }
  if (from < lines.length) parts.push({ text: lines.slice(from).join("\n") });
  return parts;
}

// Cairn review panel: lists the Artifacts registered for the active space's project
// (cairn scopes by the cwd's git project), shows one, and opens itself when an Artifact
// is new or has a new Revision.
//
// cairn binary. The flake's herdr-mod package replaces @cairn@ with the store path of the
// cairn it was built with; an unreplaced copy (a plain checkout) fails to exec and falls
// through to `cairn` on PATH.
const BINS = ["@cairn@", "cairn"];
const POLL_MS = 3000;
// weight: the right column is resized to this share when an Artifact opens it. The main pane has the default weight 1, so 1 = half of the work area (clamped by the panel's min/max px).
const PANEL = { stack: "panel-body", child: "cairn", id: "panel", weight: 1 };

function location() {
  const space = spaces().find((s) => s.active), f = focused();
  const pane = f?.pane ? panes().find((p) => p.pane === f.pane) : null;
  const on = f?.pane === null ? (f.terminal?.includes("/") ? f.terminal.split("/")[0] : null) : space?.machine ?? null;
  return { cwd: pane?.cwd || f?.cwd || space?.folder, on };
}
const key = () => JSON.stringify(location());

function ago(iso) {
  const s = Math.max(0, (Date.now() - Date.parse(iso)) / 1000);
  if (!Number.isFinite(s)) return "";
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

export default class Cairn extends View {
  init(_props, cx) {
    this.items = []; this.seen = null; this.context = null; this.bin = null; this.parked = new Map();
    this.view = null; this.detail = null; this.threads = []; this.error = null; this.writeError = null; this.writeErrorThread = null; this.loading = false;
    // Composer: one retained InputState (created here, never in render). Polls and loadDetail
    // never touch it, so an open draft keeps its text and focus. `drafts` maps draftKey() to
    // text; the key includes the space context, so drafts are per space and outlive parking.
    this.handle = cx.focus_handle(); this.input = InputState.new({ placeholder: "Write a comment..." });
    this.composer = null; this.drafts = new Map(); this.inputFocused = false;
    this.input.on("change", (_e, cx) => {
      if (!this.composer) return;
      const text = this.input.value();
      if (text) this.drafts.set(this.composer.key, text); else this.drafts.delete(this.composer.key);
      cx.notify();
    });
    this.input.on("submit", (_e, cx) => this.sendComposer(cx));
    this.input.on("focus", (_e, cx) => { this.inputFocused = true; cx.notify(); });
    this.input.on("blur", (_e, cx) => { this.inputFocused = false; cx.notify(); });
    cx.spawn(async (cx) => {
      for (;;) {
        const c = await nextCommand();
        if (c === "focus") { if (this.composer) this.input.focus(); else this.handle.focus(); }
        cx.notify();
      }
    });
    this.refresh(cx);
    cx.timer.every(POLL_MS, (cx) => this.refresh(cx));
  }

  // Runs cairn in the active space's folder so project scoping follows the space.
  async cairn(args, { cwd, on }) {
    const bins = this.bin ? [this.bin] : BINS;
    let last;
    for (const bin of bins) {
      let r;
      try { r = await exec(bin, args, { cwd, on, timeout: 15000 }); }
      catch (e) { last = e; continue; }
      this.bin = bin;
      if (r.code !== 0) throw new Error((r.stderr || r.stdout).trim() || `cairn exited ${r.code}`);
      return r.stdout;
    }
    this.bin = null;
    throw new Error(`cairn not found (${last})`);
  }

  // Runs `cairn comment <args>` as the reviewer through the same path as reads (exec {cwd, on},
  // so remote spaces work), then reloads the open Artifact at once instead of waiting for the poll.
  // --author reviewer is explicit: it equals the role default, but a CAIRN_AGENT in herdr's
  // environment (local or remote) would otherwise override it. Callers clear their own draft only
  // when ok is true; on failure the cairn error text is in writeError for rendering under the composer.
  async write(args, id, cx) {
    const loc = location(), context = this.context;
    this.writeError = null;
    let out;
    try {
      out = JSON.parse(await this.cairn(["--json", "--role", "reviewer", "--author", "reviewer", "comment", ...args], loc));
    } catch (e) {
      const error = String(e.message ?? e);
      this.writeError = error; cx.notify();
      return { ok: false, error };
    }
    // The write landed; a failed reload only shows in the panel's error line.
    try { await this.loadDetail(id, loc, context); } catch (e) { this.error = String(e.message ?? e); }
    cx.notify();
    return { ok: true, out };
  }

  draftKey(target) { return JSON.stringify([this.context, this.view, target]); }

  // Opens the composer for `target` (any JSON-able value naming where the Comment goes, e.g.
  // "revision"). onSend(text, cx) must resolve to write()'s {ok, error}; it is the only place
  // that knows the cairn arguments. A draft saved for the same (space, Artifact, target) returns.
  openComposer(target, onSend, cx) {
    // composer is set before set_value so the change event files the text under the new key.
    this.composer = { key: this.draftKey(target), target, onSend, sending: false };
    this.input.set_value(this.drafts.get(this.composer.key) ?? "");
    this.writeError = null; this.writeErrorThread = null;
    this.input.focus(); cx.notify();
  }

  // Hides the composer. The draft stays in `drafts` unless discard (Escape, or a successful send).
  closeComposer({ discard } = {}) {
    if (!this.composer) return;
    const { key } = this.composer;
    this.composer = null;
    if (discard) this.drafts.delete(key);
    this.input.set_value("");
    this.writeError = null;
  }

  async sendComposer(cx) {
    const c = this.composer, text = this.input.value();
    if (!c || c.sending || !text.trim()) return;
    c.sending = true; this.writeErrorThread = null; cx.notify();
    let r;
    try { r = await c.onSend(text, cx); } catch (e) { r = { ok: false, error: String(e.message ?? e) }; this.writeError = r.error; }
    c.sending = false;
    if (r.ok) {
      this.drafts.delete(c.key);
      if (this.composer === c) this.closeComposer();
    }
    cx.notify();
  }

  composerView(t) {
    const text = this.input.value(), c = this.composer, canSend = !c.sending && text.trim().length > 0;
    const button = (id, label, primary, on) => h_flex().id(id).h(28).px(12).items_center().rounded(7)
      .border_1().border_color(primary ? t.solid : t.input).bg(primary ? t.solid : t.raised)
      .text_color(primary ? t.solid_foreground : t.foreground).text_size(12).font_medium()
      .when(on, (e) => e.cursor_pointer().hover((s) => s.opacity(0.88)).on_click((_e, cx) => on(cx)))
      .when(!on, (e) => e.opacity(0.5)).child(label);
    return v_flex().id("cairn-composer").track_focus(this.handle).flex_none().gap(6).p(8).rounded(7).border_1().border_color(t.border)
      .on_key_down((e, cx) => { if (e.key === "escape") { this.closeComposer({ discard: true }); cx.stop_propagation(); cx.notify(); } })
      .child(Input.new(this.input).h(32).px(10).rounded(7).border_1().border_color(this.inputFocused ? t.accent : t.input).bg(t.background).text_size(12.5).text_color(t.foreground))
      .child(this.writeErrorThread === null ? this.writeErrorLine(t) : null)
      .child(h_flex().gap(8).justify_end()
        .child(button("cairn-composer-cancel", "Cancel", false, (cx) => { this.closeComposer({ discard: true }); cx.notify(); }))
        .child(button("cairn-composer-send", c.sending ? "Sending..." : "Send", true, canSend ? (cx) => this.sendComposer(cx) : null)));
  }

  writeErrorLine(t) {
    return this.writeError ? div().text_xs().text_color("#e5484d").child(this.writeError) : null;
  }

  refresh(cx) {
    if (this.loading) return;
    const loc = location(), context = key();
    if (context !== this.context) {
      // A different space or folder: park this one's state and restore the target's, so coming
      // back finds the same Artifact open. A context never visited starts with no "seen" map,
      // so its existing Artifacts do not read as new.
      if (this.context !== null) this.parked.set(this.context, { seen: this.seen, items: this.items, view: this.view, detail: this.detail, threads: this.threads });
      const s = this.parked.get(context) ?? { seen: null, items: [], view: null, detail: null, threads: [] };
      this.closeComposer();
      this.context = context; this.seen = s.seen; this.items = s.items; this.view = s.view; this.detail = s.detail; this.threads = s.threads;
    }
    if (!loc.cwd) return;
    this.loading = true;
    cx.spawn(async (cx) => {
      try {
        const out = JSON.parse(await this.cairn(["--json", "list", "--limit", "200"], loc));
        if (key() !== context) return;
        const items = out.artifacts ?? [];
        const changed = this.seen === null ? [] : items.filter((a) => this.seen[a.id] === undefined || a.latest_revision > this.seen[a.id]);
        this.seen = Object.fromEntries(items.map((a) => [a.id, a.latest_revision]));
        this.items = items; this.error = null;
        // Newest activity is first, so changed[0] is the one to show.
        if (changed.length) this.reveal(changed[0], loc, context, cx);
        else if (this.view) await this.loadDetail(this.view, loc, context);
      } catch (e) { if (key() === context) this.error = String(e.message ?? e); }
      finally { this.loading = false; cx.notify(); }
    });
  }

  reveal(item, loc, context, cx) {
    if (this.view !== item.id) this.closeComposer();
    this.view = item.id;
    run("layout.select", { stack: PANEL.stack, child: PANEL.child });
    run("layout.resize", { id: PANEL.id, weight: PANEL.weight });
    run("layout.show", { id: PANEL.id });
    cx.spawn(async (cx) => { await this.loadDetail(item.id, loc, context).catch(() => {}); cx.notify(); });
  }

  async loadDetail(id, loc, context) {
    const item = this.items.find((a) => a.id === id);
    if (!item) return;
    const rev = item.latest_revision;
    const comments = this.cairn(["--json", "comment", "list", id, "--all"], loc);
    const detail = { id, rev, kind: item.kind, body: null, parts: null, pic: null };
    if (item.kind === "image" || item.kind === "diagram") {
      detail.pic = await this.fetchPic(id, rev, loc).catch(() => null);
      // A diagram that will not render still shows its source.
      if (item.kind === "diagram" && !detail.pic) detail.body = await this.cairn(["get", `${id}@${rev}`], loc).catch(() => null);
    } else if (item.kind === "markdown") {
      detail.body = await this.cairn(["get", `${id}@${rev}`], loc);
      const { embeds } = JSON.parse(await this.cairn(["--json", "embeds", `${id}@${rev}`], loc));
      detail.parts = [];
      for (const p of splitAtEmbeds(detail.body, embeds)) {
        const e = p.embed;
        // A dangling or unfetchable target gets a placeholder; it never fails the page.
        const pic = e && !e.dangling ? await this.fetchPic(e.target_id, e.target_revision, loc).catch(() => null) : null;
        detail.parts.push(e ? { embed: e, pic } : p);
      }
    }
    if (key() !== context || this.view !== id) return;
    this.detail = detail;
    this.threads = JSON.parse(await comments).threads ?? [];
  }

  // Returns { path, w, h } for an image or diagram (cairn renders Mermaid itself), cached per
  // id@rev because revisions are immutable. Null on another machine: the cache is local.
  async fetchPic(id, rev, loc) {
    if (loc.on) return null;
    const rel = `${CACHE}/${id}@${rev}`, dir = `${appHome()}/${rel}`;
    if (!(await fs.exists(dir))) {
      await fs.mkdir(dir, { recursive: true });
      try { await this.cairn(["--json", "get", `${id}@${rev}`, "--out", dir], loc); }
      catch (e) { await fs.rmdir(dir).catch(() => {}); throw e; }
    }
    const file = (await fs.readdir(dir))[0];
    if (file?.endsWith(".svg")) {
      const svg = await fs.readFile(`${dir}/${file}`, "utf8");
      if (svg.includes("var(--")) await fs.writeFile(`${dir}/${file}`, flattenCssVars(svg));
    }
    return file ? { path: `${rel}/${file}`, ...(await naturalSize(`${dir}/${file}`)) } : null;
  }

  // Opens the artifact in the cairn web UI. `cairn view` prints the URL (honoring CAIRN_URL);
  // it does not check that `cairn serve` is running, so probe first rather than open a dead tab.
  openInBrowser(id, rev, cx) {
    const loc = location();
    if (loc.on) { notify({ title: "Cairn", body: "Open in browser is only supported for local spaces.", level: "warning" }); return; }
    cx.spawn(async (cx) => {
      try {
        const url = (await this.cairn(["view", `${id}@${rev}`], loc)).trim();
        let up = true;
        try { await fetch(url); } catch { up = false; }
        if (!up) { notify({ title: "Cairn", body: `Nothing is listening at ${url}. Run \`cairn serve\`.`, level: "warning" }); return; }
        const r = await exec("open", [url]).catch(() => exec("xdg-open", [url]));
        if (r.code !== 0) throw new Error(r.stderr);
      } catch (e) { notify({ title: "Cairn", body: `Cannot open browser: ${e.message ?? e}`, level: "error" }); }
    });
  }

  picElement(pic, t) {
    if (!pic) return null;
    const max = Math.max(120, (this.width ?? 340) - 30);
    const w = pic.w ? Math.min(pic.w, max) : max, h = pic.w && pic.h ? w * pic.h / pic.w : w * 0.6;
    return div().flex_none().rounded(6).overflow_hidden().border_1().border_color(t.border).bg("#ffffff").w(w).h(h).child(image(pic.path).flex_none().w(w).h(h));
  }

  open(id, cx) {
    this.closeComposer();
    this.view = id; this.detail = null; this.threads = []; this.writeError = null;
    const loc = location(), context = this.context;
    cx.spawn(async (cx) => {
      try { await this.loadDetail(id, loc, context); } catch (e) { this.error = String(e.message ?? e); }
      cx.notify();
    });
    cx.notify();
  }

  header(t, title, back) {
    return h_flex().h(42).flex_none().px(14).gap(8).items_center().border_b(1).border_color(t.border)
      .when(back, (e) => e.child(div().id("cairn-back").px(6).py(2).rounded(5).cursor_pointer().text_color(t.muted).hover((s) => s.bg(t.hover)).on_click((_e, cx) => { this.closeComposer(); this.view = null; this.detail = null; cx.notify(); }).child("←")))
      .child(div().font_semibold().flex_1().min_w(0).truncate().child(title));
  }

  indexView(t) {
    const open = this.items.reduce((n, a) => n + a.open_threads, 0);
    return [
      this.header(t, "Cairn").child(div().px(5).py(1).rounded(4).border_1().border_color(t.border).text_size(10).text_color(t.muted).child(`${this.items.length} artifacts`))
        .when(open > 0, (e) => e.child(div().text_size(11).text_color(t.blocked).child(`${open} open`))),
      div().flex_1().min_h(0).relative()
        .child(v_flex().id("cairn-list").absolute().inset_0().overflow_y_scroll().p(6).children(this.items.map((a) =>
          h_flex().id(`cairn-${a.id}`).h(36).flex_none().px(8).gap(8).items_center().rounded(7).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.open(a.id, cx))
            .child(div().w(52).flex_none().text_size(10).text_color(t.dim).child(a.kind))
            .child(div().flex_1().min_w(0).truncate().child(a.name))
            .when(a.open_threads > 0, (e) => e.child(div().text_size(11).text_color(t.blocked).child(`${a.open_threads}●`)))
            .child(div().text_size(11).text_color(t.dim).child(`r${a.latest_revision} · ${ago(a.updated_at)}`)))))
        .when(!this.items.length, (e) => e.child(div().absolute().top(6).p(8).text_color(t.dim).child(this.error || (this.loading ? "Loading…" : "No artifacts for this project"))))
        .child(Scrollbar.vertical("cairn-list").absolute().inset_0()),
    ];
  }

  detailView(t) {
    const item = this.items.find((a) => a.id === this.view);
    const d = this.detail;
    const threads = this.threads;
    return [
      this.header(t, item ? item.name : this.view, true).child(div().text_size(11).text_color(t.dim).child(item ? `${item.kind} · r${item.latest_revision}` : ""))
        .when(Boolean(item), (e) => e.child(div().id("cairn-comment").flex_none().px(7).py(3).rounded(5).cursor_pointer().text_size(11).text_color(t.muted).hover((s) => s.bg(t.hover))
          .on_click((_e, cx) => this.openComposer("revision", (text, cx) => this.write(["add", item.id, text], item.id, cx), cx)).child("Comment")))
        .when(Boolean(item), (e) => e.child(div().id("cairn-browser").flex_none().px(7).py(3).rounded(5).cursor_pointer().text_size(11).text_color(t.muted).hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.openInBrowser(item.id, item.latest_revision, cx)).child("Open ↗"))),
      div().flex_1().min_h(0).relative()
        .child(v_flex().id("cairn-doc").absolute().inset_0().overflow_y_scroll().p(14).gap(12)
          .when(!d, (e) => e.child(div().text_color(t.dim).child(this.error || "Loading…")))
          .when(d && d.kind === "markdown", (e) => e.children((d.parts ?? []).map((p, i) => p.embed
            ? (p.pic ? this.picElement(p.pic, t) : div().text_size(11).text_color(t.dim).child(`[embedded ${p.embed.target_id}${p.embed.dangling ? " (deleted)" : " cannot be shown here"}${p.embed.alt ? `: ${p.embed.alt}` : ""}]`))
            : TextView.markdown(`cairn-md-${i}`, p.text).selectable())))
          .when(d && d.pic, (e) => e.child(this.picElement(d.pic, t)))
          .when(d && d.kind === "diagram" && !d.pic, (e) => e.child(div().text_size(11).text_color(t.dim).child("Could not render this diagram; showing source.")).child(div().font_family(settings().font_family ?? "JetBrains Mono").text_size(11).child(d.body ?? "")))
          .when(d && d.kind === "image" && !d.pic, (e) => e.child(div().text_color(t.dim).child("Cannot show this image here.")))
          .when(d && d.kind === "html", (e) => e.child(div().text_color(t.dim).child("html bundles are not rendered in this panel (herdr has no web view). Open it in the cairn web UI.")))
          .when(threads.length > 0, (e) => e.child(div().font_semibold().pt(6).border_t(1).border_color(t.border).child("Comments")))
          .children(threads.map((th) =>
            v_flex().gap(4).p(8).rounded(7).border_1().border_color(th.status === "open" ? t.blocked : t.border)
              .child(h_flex().gap(6).text_size(11).text_color(t.dim)
                .child(div().child(th.author)).child(div().child(th.locator?.section_path ?? (th.locator?.line_start ? `line ${th.locator.line_start}` : "whole revision")))
                .child(div().flex_1()).child(div().child(`${th.status}${th.stale ? " · stale" : ""}`))
                .child(div().id(`cairn-thread-${th.id}-toggle`).flex_none().px(7).py(3).rounded(5).cursor_pointer().text_size(11).text_color(t.muted).hover((s) => s.bg(t.hover))
                  .on_click((_e, cx) => cx.spawn(async (cx) => {
                    this.writeErrorThread = th.id;
                    await this.write([th.status === "open" ? "resolve" : "reopen", String(th.id)], this.view, cx);
                  }))
                  .child(th.status === "open" ? "Resolve" : "Reopen")))
              .child(div().text_size(12).child(th.body))
              .children((th.replies ?? []).map((r) => div().pl(10).text_size(12).text_color(t.muted).child(`${r.author}: ${r.body}`)))
              .when(this.writeErrorThread === th.id, (e) => e.child(this.writeErrorLine(t))))))
        .child(Scrollbar.vertical("cairn-doc").absolute().inset_0()),
      ...(this.composer ? [div().flex_none().p(8).border_t(1).border_color(t.border).child(this.composerView(t))] : []),
    ];
  }

  render(cx) {
    if (key() !== this.context) this.refresh(cx);
    const t = theme();
    return v_flex().id("cairn").on_size_change((size, cx) => { if (this.width !== size.width) { this.width = size.width; cx.notify(); } }).size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .children(this.view ? this.detailView(t) : this.indexView(t));
  }
}
