// Launch page of LGTM.md: widget gallery and a setup that writes the workflow and README code.
"use strict";

// The release the generated workflow pins. Change it here only.
const VERSION = "v1.2.0";
const REPO_URL = "https://github.com/eliorpom-cmd/lgtm.md";
const REPO_API = "https://api.github.com/repos/eliorpom-cmd/lgtm.md";

// Gallery filters, in tab order.
const CATEGORIES = ["Streak", "Activity", "Languages", "Pull requests", "Compare"];
const CATEGORY = {
  flame: "Streak", records: "Streak",
  grass: "Activity", counter: "Activity", departures: "Activity", forest: "Activity", ecg: "Activity", "cherry-tree": "Activity",
  hourglass: "Languages", donut: "Languages",
  fireworks: "Pull requests", "fireworks-commits": "Pull requests",
  median: "Compare", duel: "Compare",
};

// w/h: size of the preview GIF. width: the README width that keeps the GIF sharp but not huge.
// project: allowed in Project mode. why: the reason shown when it is not.
const WIDGETS = [
  { id: "grass", name: "Grass field", w: 1600, h: 744, width: "100%", project: true,
    desc: "The last 90 days, one tuft of grass per day. Flowers are your languages, fireflies your merged pull requests.",
    alt: "Night field: one tuft of grass per day over the last 90 days" },
  { id: "flame", name: "Campfire", w: 832, h: 552, width: "49%", project: true,
    desc: "Your current streak of active days. The fire grows at 7, 30 and 100 days.",
    alt: "Campfire for a streak of active days" },
  { id: "counter", name: "Odometer", w: 832, h: 552, width: "49%", project: true,
    desc: "Contributions over the last 12 months, rolling up like an odometer.",
    alt: "Drum counter of contributions over 12 months" },
  { id: "departures", name: "Departures board", w: 1600, h: 424, width: "100%", project: true,
    desc: "A station board that flips through four stats: contributions, merged pull requests, streak and top language.",
    alt: "Split-flap departure board" },
  { id: "hourglass", name: "Hourglass", w: 1600, h: 744, width: "100%", project: true,
    desc: "The past year's commits as sand, one colour per language, oldest at the bottom.",
    alt: "Hourglass of coloured sand, one grain per batch of commits" },
  { id: "forest", name: "Forest", w: 1600, h: 744, width: "100%", project: true,
    desc: "The last 12 months, one tree per week. A quiet week leaves a stump.",
    alt: "Forest with one tree per week of the year" },
  { id: "ecg", name: "Heartbeat", w: 1600, h: 552, width: "100%", project: true,
    desc: "The last 7 days as a heartbeat, with the best day highlighted.",
    alt: "Heartbeat of the last 7 days on a monitor" },
  { id: "records", name: "Records", w: 1600, h: 552, width: "100%", project: true,
    desc: "Best day, longest streak and longest break of the last 12 months.",
    alt: "Three brass plaques: best day, longest streak, longest break" },
  { id: "cherry-tree", name: "Cherry tree", w: 1600, h: 744, width: "100%", project: true,
    desc: "The last 90 days as a blossoming crown. Each falling petal is a merged pull request.",
    alt: "Cherry tree in bloom at dusk" },
  { id: "donut", name: "Donut", w: 1280, h: 552, width: "80%", project: true,
    desc: "Your top languages as slices of icing, sized by their share of the code.",
    alt: "Doughnut iced in one colour per language" },
  { id: "fireworks", name: "Fireworks (pull requests)", w: 1600, h: 744, width: "100%", project: true,
    desc: "One rocket per pull request merged in the last 7 days, coloured by language.",
    alt: "Fireworks over a lake, one rocket per merged pull request" },
  { id: "fireworks-commits", name: "Fireworks (commits)", w: 1600, h: 744, width: "100%", project: true,
    desc: "One rocket per batch of commits from the last 7 days.",
    alt: "Fireworks over a lake, one rocket per batch of commits" },
  { id: "median", name: "Median", w: 1120, h: 680, width: "70%", project: false,
    why: "Profile only: it compares you with the accounts you follow.",
    desc: "Your tower of contributions next to the median of the accounts you follow.",
    alt: "Two builders raising brick towers" },
  { id: "duel", name: "Duel", w: 1600, h: 744, width: "100%", project: false, file: "duel-rival",
    why: "Profile only: it compares two accounts.",
    desc: "You against one friend over the last 30 days, day by day, with your avatars.",
    alt: "Duel between two accounts over 30 days" },
];
const BY_ID = new Map(WIDGETS.map((w) => [w.id, w]));

const LOGIN_RE = /^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$/;
const REPO_RE = /^([A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38})\/([A-Za-z0-9._-]{1,100})$/;
const LOGIN_ERROR = "A GitHub login has letters, digits and single hyphens, up to 39 characters.";

const state = {
  mode: "profile",
  target: "",
  rival: "",
  picked: new Set(),
  tab: "preview",
  setupVisible: false,
};

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => [...document.querySelectorAll(sel)];
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function allowed(widget) {
  return state.mode === "profile" || widget.project;
}

// Widgets picked and allowed in the current mode, in gallery order.
function pickedWidgets() {
  return WIDGETS.filter((w) => state.picked.has(w.id) && allowed(w));
}

// Widgets that go in the workflow right now: the duel needs a valid friend login.
function activeWidgets() {
  return pickedWidgets().filter((w) => actionName(w));
}

function cleanLogin(s) {
  return s.trim().replace(/^@/, "");
}

function rivalLogin() {
  const r = cleanLogin(state.rival);
  return LOGIN_RE.test(r) ? r : "";
}

function rivalError() {
  const r = cleanLogin(state.rival);
  return r && !LOGIN_RE.test(r) ? LOGIN_ERROR : "";
}

// The duel is picked but has no valid friend login yet.
function duelPending() {
  return state.mode === "profile" && state.picked.has("duel") && !rivalLogin();
}

// The name the Action expects, and the GIF it writes.
function actionName(widget) {
  if (widget.id === "duel") return rivalLogin() ? `duel-${rivalLogin()}` : "";
  return widget.id;
}

function targetInfo() {
  const raw = state.target.trim().replace(/^@/, "").replace(/^https?:\/\/github\.com\//, "").replace(/\/+$/, "");
  if (state.mode === "profile") {
    if (!raw) return { ok: false, owner: "your-login", repo: "your-login", empty: true };
    return LOGIN_RE.test(raw) ? { ok: true, owner: raw, repo: raw } : { ok: false, owner: "your-login", repo: "your-login", error: LOGIN_ERROR };
  }
  if (!raw) return { ok: false, owner: "owner", repo: "repo", empty: true };
  const m = raw.match(REPO_RE);
  if (m) return { ok: true, owner: m[1], repo: m[2] };
  // "login/" right after switching mode is not an error yet: the name is still to come.
  if (/^[^/]+\/$/.test(state.target.trim()) && LOGIN_RE.test(raw)) return { ok: false, owner: raw, repo: "repo", empty: true };
  return { ok: false, owner: "owner", repo: "repo", error: "Write the repository as owner/name, for example octocat/hello-world." };
}

// Same workflow as examples/profile-workflow.yml and examples/repo-workflow.yml.
// A project publishes its GIFs on a separate lgtm-md branch:
// a nightly commit on the default branch would count as the project's own activity.
function buildYaml() {
  const names = activeWidgets().map(actionName);
  const project = state.mode === "project";
  const lines = [
    "name: LGTM.md",
    "",
    "on:",
    "  schedule:",
    '    - cron: "30 0 * * *"   # every night, just after midnight UTC',
    "  workflow_dispatch:",
    "  push:",
    "    paths: [.github/workflows/lgtm-md.yml]   # also runs as soon as you commit this file",
    "",
    "permissions:",
    "  contents: write",
  ];
  if (project) lines.push("  pull-requests: read   # lets the default token see the pull requests of a private repository");
  lines.push(
    "",
    "concurrency: lgtm-md",
    "",
    "jobs:",
    "  draw:",
    "    runs-on: ubuntu-latest",
    "    steps:",
  );
  if (!project) lines.push("      - uses: actions/checkout@v4");
  lines.push(`      - uses: eliorpom-cmd/lgtm.md@${VERSION}`, "        with:");
  lines.push(project ? "          repo: ${{ github.repository }}" : "          token: ${{ secrets.LGTM_MD_TOKEN || github.token }}");
  lines.push("          widgets: |");
  for (const n of names) lines.push(`            ${n}`);
  lines.push(`          output-dir: ${project ? "lgtm-md" : "widgets"}`);
  if (project) {
    lines.push(
      "      - name: Publish the widgets on the lgtm-md branch",
      "        shell: bash",
      "        env:",
      "          GITHUB_TOKEN: ${{ github.token }}",
      "        run: |",
      "          cd lgtm-md",
      "          git init --quiet --initial-branch lgtm-md",
      "          git add .",
      '          git -c user.name="github-actions[bot]" \\',
      '              -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \\',
      '              commit --quiet --message "Draw the widgets"',
      "          # One commit, replaced every night: the branch never grows.",
      "          git push --quiet --force \\",
      '            "https://x-access-token:${GITHUB_TOKEN}@github.com/${GITHUB_REPOSITORY}.git" lgtm-md',
      "",
    );
  } else {
    lines.push(
      "      - name: Commit the widgets if they changed",
      "        shell: bash",
      "        run: |",
      "          git add widgets",
      "          git diff --cached --quiet && exit 0",
      '          git config user.name "github-actions[bot]"',
      '          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"',
      `          git commit --quiet --message "widgets: draw the day's widgets"`,
      "          git push --quiet",
      "",
    );
  }
  return lines.join("\n");
}

function imageBase(t) {
  return state.mode === "project" ? `https://github.com/${t.owner}/${t.repo}/raw/lgtm-md/` : "widgets/";
}

function buildHtml(t) {
  const base = imageBase(t);
  const out = activeWidgets().map((w) => `<a href="${REPO_URL}"><img src="${base}${actionName(w)}.gif" alt="${w.alt}" width="${w.width}"></a>`);
  if (out.length) out.push("", "<!-- Optional: the credit line below can go. -->", `<sub>Made with <a href="${REPO_URL}">LGTM.md</a></sub>`);
  return out.join("\n");
}

// The README as GitHub renders it: consecutive image lines form one paragraph, so two 49% images
// sit side by side and a 100% image takes its own row. Sample GIFs stand in for the visitor's.
function renderPreview(t) {
  $("#md-path").textContent = state.mode === "project" ? `${t.owner} / ${t.repo} / README.md` : `${t.owner} / README.md`;
  const list = activeWidgets();
  if (!list.length) {
    $("#md-body").innerHTML = `<div class="md-empty">${duelPending() ? "Type a friend’s login for the duel and it shows here." : "Pick a widget and it shows here, at the size it takes in your README."}</div>`;
    return;
  }
  const imgs = list.map((w) => `<a href="${REPO_URL}" tabindex="-1"><img src="widgets/${w.file || w.id}.gif" alt="${esc(w.alt)}" width="${w.width}"></a>`).join("\n");
  $("#md-body").innerHTML = `<p>${imgs}</p><p><sub>Made with <a href="${REPO_URL}" tabindex="-1">LGTM.md</a></sub></p>`;
}

// Tiny highlighters with GitHub's dark colours. They take one raw line and return escaped HTML.
const span = (cls, s) => `<span class="${cls}">${esc(s)}</span>`;

function highlightHtml(line) {
  const re = /<!--.*?-->|<\/?[A-Za-z][\w-]*|\/?>|[A-Za-z-]+(?==")|"[^"]*"/g;
  let out = "", last = 0, m;
  while ((m = re.exec(line))) {
    out += esc(line.slice(last, m.index));
    const tok = m[0];
    const cls = tok.startsWith("<!--") ? "t-com" : tok[0] === "<" || tok.endsWith(">") ? "t-tag" : tok[0] === '"' ? "t-str" : "t-attr";
    out += span(cls, tok);
    last = m.index + tok.length;
  }
  return out + esc(line.slice(last));
}

function highlightYaml(line) {
  // A comment starts at a # outside quotes that follows a space or begins the line.
  let code = line, comment = "", quote = false;
  for (let i = 0; i < line.length; i++) {
    if (line[i] === '"') quote = !quote;
    if (line[i] === "#" && !quote && (i === 0 || line[i - 1] === " ")) { code = line.slice(0, i); comment = line.slice(i); break; }
  }
  let out = "";
  const key = code.match(/^(\s*(?:- )?)([\w.-]+)(:)(?=\s|$)/);
  if (key) {
    out += esc(key[1]) + span("t-key", key[2]) + esc(key[3]);
    code = code.slice(key[0].length);
  }
  out += code.split(/("[^"]*")/).map((part) => (part.startsWith('"') && part.endsWith('"') && part.length > 1 ? span("t-str", part) : esc(part))).join("");
  return out + (comment ? span("t-com", comment) : "");
}

// One element per line, so CSS can number them. Lines that were not there before slide in.
function renderCode(el, text, lang) {
  const before = new Set((el.dataset.raw || "").split("\n"));
  const first = !("raw" in el.dataset);
  el.dataset.raw = text;
  const hl = lang === "yaml" ? highlightYaml : highlightHtml;
  el.innerHTML = text.replace(/\n$/, "").split("\n").map((line) => {
    const indent = line.match(/^ */)[0].length;
    const fresh = !first && line.trim() && !before.has(line);
    return `<span class="ln${fresh ? " is-new" : ""}"><span class="lc" style="--i:${indent}ch">${hl(line) || " "}</span></span>`;
  }).join("");
}

function renderGrid() {
  $("#filters").innerHTML = ["All", ...CATEGORIES].map((c) =>
    `<button class="filter" type="button" data-filter="${c}" aria-pressed="${c === "All"}">${c}</button>`
  ).join("");
  $("#grid").innerHTML = WIDGETS.map((w) => {
    const file = w.file || w.id;
    const id = w.id === "duel" ? "duel-&lt;login&gt;" : w.id;
    return `<li class="card" data-id="${w.id}" data-cat="${CATEGORY[w.id]}">
      <div class="card-media">
        <img src="widgets/${file}.gif" alt="${esc(w.alt)}" width="${w.w}" height="${w.h}" loading="lazy" decoding="async">
      </div>
      <div class="card-body">
        <div class="card-text">
          <h3 class="card-name">${esc(w.name)}</h3>
          <p class="card-id">${id}</p>
          <p class="card-desc">${esc(w.desc)}</p>
          <p class="card-tag" id="tag-${w.id}" hidden></p>
        </div>
        <button class="toggle pill-btn small ghost" type="button" data-id="${w.id}" aria-pressed="false" aria-label="Add ${esc(w.name)}" aria-describedby="tag-${w.id}">Add</button>
      </div>
    </li>`;
  }).join("");
}

function renderChips() {
  $("#chips").innerHTML = WIDGETS.map((w) =>
    `<button class="chip" type="button" data-id="${w.id}" aria-pressed="false">${w.id === "duel" ? "duel-&lt;login&gt;" : w.id}</button>`
  ).join("");
}

function update() {
  const t = targetInfo();
  const project = state.mode === "project";
  const pending = duelPending();

  // Both mode switches, and the texts that depend on the mode.
  for (const b of $$(".mode button")) b.setAttribute("aria-pressed", String(b.dataset.mode === state.mode));
  $("#repo-title").textContent = project ? "Your repository" : "Your profile repository";
  $("#repo-help").textContent = project
    ? "The widgets show this repository's activity: commits on its default branch, merged pull requests and languages."
    : "A profile README lives in a repository named like your login. It shows at the top of your GitHub profile.";
  $("#target-label").textContent = project ? "Repository (owner/name)" : "Your GitHub login";
  $("#target").placeholder = project ? "owner/repo" : "your-login";
  $("#target").setAttribute("aria-invalid", String(!!t.error));
  const err = $("#target-error");
  err.hidden = !t.error;
  err.textContent = t.error || "";
  $("#repo-check").hidden = project;
  const createRepo = $("#repo-create");
  createRepo.hidden = project;
  createRepo.href = t.ok && !project
    ? `https://github.com/new?name=${encodeURIComponent(t.owner)}&visibility=public`
    : "https://github.com/new?visibility=public";

  // Gallery cards and chips share one selection.
  for (const w of WIDGETS) {
    const ok = allowed(w);
    const on = state.picked.has(w.id) && ok;
    const needs = on && w.id === "duel" && pending;
    const card = document.querySelector(`.card[data-id="${w.id}"]`);
    card.classList.toggle("is-picked", on);
    card.classList.toggle("needs-login", needs);
    const tag = card.querySelector(".card-tag");
    tag.textContent = !ok ? w.why : needs ? "Type a friend’s login in the setup below." : "";
    tag.hidden = !tag.textContent;
    const toggle = card.querySelector(".toggle");
    toggle.disabled = !ok;
    toggle.setAttribute("aria-pressed", String(on));
    toggle.textContent = on ? "Added ✓" : "Add";
    const chip = document.querySelector(`.chip[data-id="${w.id}"]`);
    chip.disabled = !ok;
    chip.setAttribute("aria-pressed", String(on));
    chip.classList.toggle("needs-login", needs);
    if (w.id === "duel") chip.textContent = `duel-${rivalLogin() || "<login>"}`;
  }

  const active = activeWidgets();
  const count = active.length;
  const countText = `${count} widget${count === 1 ? "" : "s"} selected`;
  $("#bar-count").textContent = countText;
  $("#picked-count").textContent = count ? countText : "None picked yet";
  const showBar = count > 0 && !state.setupVisible;
  $("#bar").classList.toggle("show", showBar);
  $("#bar").setAttribute("aria-hidden", String(!showBar));
  $("#bar a").tabIndex = showBar ? 0 : -1;
  $("#bar-thumbs").innerHTML = active.slice(0, 6).map((w) => `<img src="widgets/${w.file || w.id}.gif" alt="">`).join("");
  $("#rival-field").hidden = !(state.picked.has("duel") && !project);
  const rErr = rivalError();
  $("#rival-error").hidden = !rErr;
  $("#rival-error").textContent = rErr;
  $("#rival").setAttribute("aria-invalid", String(!!rErr));
  $("#pick-all").textContent = WIDGETS.filter(allowed).every((w) => state.picked.has(w.id)) ? "Clear" : "Select all";

  // Snippets, preview, and the link that opens GitHub's new-file page with the workflow filled in.
  const yaml = buildYaml();
  const html = buildHtml(t);
  renderCode($("#yaml"), yaml, "yaml");
  renderCode($("#html"), html || "<!-- Pick a widget: its image line appears here. -->", "html");
  renderPreview(t);
  for (const b of $$(".copy")) b.disabled = !count;
  $("#copy-label").textContent = state.tab === "yaml" ? "Copy workflow" : "Copy README";

  const repo = `${t.owner}/${t.repo}`;
  $("#commit-help").textContent = project
    ? `The GIFs go on a separate lgtm-md branch of ${repo}, so your default branch gets no nightly commit. No token needed.`
    : `The GIFs go in the widgets/ folder of ${repo} and are redrawn every night.`;

  const create = $("#create");
  const ready = t.ok && count > 0;
  create.setAttribute("aria-disabled", String(!ready));
  // A disabled link has no href, so neither a click nor the keyboard can follow it.
  if (ready) {
    // HEAD resolves to the default branch, whatever its name.
    create.href = `https://github.com/${t.owner}/${t.repo}/new/HEAD?filename=${encodeURIComponent(".github/workflows/lgtm-md.yml")}&value=${encodeURIComponent(yaml)}`;
    create.target = "_blank";
  } else {
    create.removeAttribute("href");
    create.removeAttribute("target");
  }
  const hint = $("#create-hint");
  if (!count) hint.textContent = pending ? "Type a friend’s login for the duel." : "Pick a widget first.";
  else if (t.error) hint.textContent = "Fix your login above.";
  else if (!t.ok) hint.textContent = project ? "Type the repository first." : "Type your login first.";
  else hint.textContent = "";
  hint.hidden = !hint.textContent;

  $("#actions-link").href = `https://github.com/${t.owner}/${t.repo}/actions`;
  const result = $("#result-link");
  result.href = project ? `https://github.com/${t.owner}/${t.repo}` : `https://github.com/${t.owner}`;
  result.textContent = project ? "See your repository ↗" : "See your profile ↗";

  // Private activity needs a token only for a profile: a project's own token already reads it.
  $("#step-token").hidden = project;

  saveUrl();
}

// The selection lives in the address bar, so a link restores it.
function saveUrl() {
  const p = new URLSearchParams();
  const ids = WIDGETS.filter((w) => state.picked.has(w.id)).map((w) => w.id);
  if (ids.length) p.set("w", ids.join(","));
  if (state.target.trim()) p.set("u", state.target.trim());
  if (state.mode !== "profile") p.set("m", state.mode);
  if (state.rival.trim()) p.set("r", state.rival.trim());
  const q = p.toString().replace(/%2C/g, ",");
  const url = location.pathname + (q ? `?${q}` : "") + location.hash;
  try { history.replaceState(null, "", url); } catch { /* file:// and sandboxed frames */ }
}

// Everything read from the address bar is checked before use.
function loadUrl() {
  const p = new URLSearchParams(location.search);
  const m = p.get("m");
  if (m === "profile" || m === "project") state.mode = m;
  for (const id of (p.get("w") || "").split(",").slice(0, WIDGETS.length)) if (BY_ID.has(id)) state.picked.add(id);
  const u = (p.get("u") || "").trim().slice(0, 140);
  if (u && (state.mode === "profile" ? LOGIN_RE.test(u) : REPO_RE.test(u) || /^[A-Za-z0-9-]{1,39}\/$/.test(u))) state.target = u;
  const r = (p.get("r") || "").trim().slice(0, 40);
  if (LOGIN_RE.test(r)) state.rival = r;
  $("#target").value = state.target;
  $("#rival").value = state.rival;
}

function selectTab(file) {
  state.tab = file;
  for (const t of $$(".gh-tab")) t.setAttribute("aria-selected", String(t.dataset.file === file));
  $("#preview").hidden = file !== "preview";
  $("#html").hidden = file !== "html";
  $("#yaml").hidden = file !== "yaml";
  $("#copy-label").textContent = file === "yaml" ? "Copy workflow" : "Copy README";
}

function toast(text) {
  const el = $("#toast");
  el.textContent = text;
  el.classList.add("show");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove("show"), 1600);
}

async function copy(text) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Older browsers and non-secure pages: fall back to a hidden text area.
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
  toast("Copied");
}

// Switching mode keeps the login: Project prefills "login/", Profile keeps the owner part.
function setMode(mode) {
  if (mode !== state.mode) {
    const raw = state.target.trim();
    if (mode === "project") state.target = raw && !raw.includes("/") ? `${raw}/` : raw;
    else state.target = raw.split("/")[0];
    state.mode = mode;
    $("#target").value = state.target;
  }
  update();
}

function toggleWidget(id, button) {
  if (state.picked.has(id)) state.picked.delete(id);
  else state.picked.add(id);
  update();
  if (button && button.classList.contains("toggle")) {
    button.classList.remove("pop");
    void button.offsetWidth;
    button.classList.add("pop");
  }
}

// Real profiles that use the widgets, from wild.json. One entry gets one wide card.
async function renderWild() {
  try {
    const res = await fetch("wild.json", { cache: "no-cache" });
    if (!res.ok) return;
    const list = (await res.json()).filter((e) =>
      e && LOGIN_RE.test(e.login || "") && /^wild\/[\w.-]+\.(webp|png)$/.test(e.image || "") && Array.isArray(e.widgets));
    if (!list.length) return;
    const names = (ids) => {
      const n = ids.filter((id) => BY_ID.has(id)).map((id) => BY_ID.get(id).name.toLowerCase());
      return n.length > 1 ? `${n.slice(0, -1).join(", ")} and ${n[n.length - 1]}` : n.join("");
    };
    const date = (d) => {
      const t = /^\d{4}-\d{2}-\d{2}$/.test(d || "") ? new Date(`${d}T12:00:00Z`) : null;
      return t ? t.toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" }) : "";
    };
    $("#wild-list").classList.toggle("single", list.length === 1);
    $("#wild-list").innerHTML = list.map((e) => `
      <a class="wild-card" href="https://github.com/${e.login}" target="_blank" rel="noopener">
        <div class="wild-shot"><img src="${e.image}" alt="README of ${e.login} on GitHub with the ${esc(names(e.widgets))} widgets" width="${+e.width || 1600}" height="${+e.height || 1000}" loading="lazy" decoding="async"></div>
        <div class="wild-meta">
          <div class="wild-who"><img src="https://github.com/${e.login}.png?size=64" alt="" width="40" height="40" loading="lazy"><span class="wild-login">${e.login}</span></div>
          <p class="wild-uses">Uses ${esc(names(e.widgets))}.</p>
          ${date(e.captured) ? `<p class="wild-date">Screenshot from ${date(e.captured)}</p>` : ""}
          <span class="pill-btn small ghost">View profile ↗</span>
        </div>
      </a>`).join("");
    $("#wild").hidden = false;
    numberSections();
  } catch { /* the section stays hidden */ }
}

// Section labels count only the sections on screen.
function numberSections() {
  let n = 0;
  for (const sec of $$("main .sec")) {
    if (sec.hidden) continue;
    n += 1;
    sec.querySelector(".sec-num").textContent = `${String(n).padStart(2, "0")} / `;
  }
}

// Live star count, cached for an hour. On any failure, or at zero, the count stays hidden.
async function loadStars() {
  const el = $("#star-count");
  const show = (n) => { if (Number.isInteger(n) && n > 0) { el.textContent = n.toLocaleString("en-US"); el.hidden = false; } };
  try {
    const cached = JSON.parse(localStorage.getItem("lgtm-stars") || "null");
    if (cached && Date.now() - cached.at < 3600e3) return show(cached.n);
  } catch { /* storage blocked */ }
  try {
    const res = await fetch(REPO_API);
    if (!res.ok) return;
    const n = (await res.json()).stargazers_count;
    show(n);
    try { localStorage.setItem("lgtm-stars", JSON.stringify({ n, at: Date.now() })); } catch { /* storage blocked */ }
  } catch { /* offline or rate-limited */ }
}

function init() {
  document.documentElement.classList.add("js");
  for (const el of $$("[data-version]")) el.textContent = VERSION;
  renderGrid();
  renderChips();
  loadUrl();
  numberSections();

  document.addEventListener("click", (e) => {
    const toggle = e.target.closest(".toggle, .chip");
    if (toggle && !toggle.disabled) return toggleWidget(toggle.dataset.id, toggle);
    const mode = e.target.closest(".mode button");
    if (mode) return setMode(mode.dataset.mode);
    const tab = e.target.closest(".gh-tab");
    if (tab) return selectTab(tab.dataset.file);
    const filter = e.target.closest(".filter");
    if (filter) {
      for (const f of $$(".filter")) f.setAttribute("aria-pressed", String(f === filter));
      const c = filter.dataset.filter;
      for (const card of $$(".card")) card.hidden = c !== "All" && card.dataset.cat !== c;
      return;
    }
    const copyBtn = e.target.closest(".copy");
    if (copyBtn && !copyBtn.disabled) {
      const file = copyBtn.dataset.copy === "active" && state.tab === "yaml" ? "yaml" : "html";
      return copy(activeWidgets().length ? $("#" + file).dataset.raw : "");
    }
    if (e.target.closest("#pick-all")) {
      const all = WIDGETS.filter(allowed);
      const full = all.every((w) => state.picked.has(w.id));
      for (const w of all) full ? state.picked.delete(w.id) : state.picked.add(w.id);
      return update();
    }
  });
  $("#target").addEventListener("input", (e) => { state.target = e.target.value; update(); });
  $("#rival").addEventListener("input", (e) => { state.rival = e.target.value; update(); });

  // Pointing at the button shows the file it creates.
  for (const ev of ["mouseenter", "focus"]) $("#create").addEventListener(ev, () => selectTab("yaml"));

  // Mode switches also move with the arrow keys.
  for (const group of $$(".mode")) {
    group.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      e.preventDefault();
      setMode(state.mode === "profile" ? "project" : "profile");
      group.querySelector(`[data-mode="${state.mode}"]`).focus();
    });
  }

  if ("IntersectionObserver" in window) {
    // The selection bar hides once the setup is on screen.
    new IntersectionObserver(([en]) => { state.setupVisible = en.isIntersecting; update(); }, { rootMargin: "0px 0px -40% 0px" }).observe($("#setup"));
    // The step in the middle of the screen is the active one.
    const steps = $$(".step");
    const io = new IntersectionObserver((entries) => {
      for (const en of entries) if (en.isIntersecting) for (const st of steps) st.classList.toggle("is-active", st === en.target);
    }, { rootMargin: "-45% 0px -45% 0px" });
    for (const st of steps) io.observe(st);
  }

  update();
  renderWild();
  loadStars();
}

init();
