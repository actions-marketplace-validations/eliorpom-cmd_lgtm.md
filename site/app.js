// Launch page of LGTM.md: widget gallery, and a setup in a dialog that writes the workflow and README code.
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
  step: "widgets",
  open: false,
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
    // Narrow screens may wrap before a tag that follows another one, between = and its value,
    // <wbr> adds no text, so a copy of the selection stays the same.
    const prev = line[m.index - 1];
    if ((cls === "t-tag" && tok[0] === "<" && prev && prev !== " ") || (cls === "t-str" && prev === "=")) out += "<wbr>";
    // A quoted URL never wraps, so it can't break at a hyphen.
    out += span(cls === "t-str" && tok.includes("://") ? "t-str t-url" : cls, tok);
    last = m.index + tok.length;
    // ...and between a tag and the text that follows it.
    if (tok.endsWith(">") && line[last] && line[last] !== " " && line[last] !== "<") out += "<wbr>";
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
function toast(text) {
  const el = $("#toast");
  el.textContent = text;
  el.classList.add("show");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove("show"), 1600);
}

// The button that was clicked says "Copied" for a moment; the toast is the fallback.
async function copy(text, button) {
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
  if (!button) return toast("Copied");
  if (!button.dataset.label) button.dataset.label = button.innerHTML;
  button.innerHTML = '<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>Copied';
  button.classList.add("is-done");
  clearTimeout(button.timer);
  button.timer = setTimeout(() => { button.innerHTML = button.dataset.label; button.classList.remove("is-done"); }, 1600);
  toast.live("Copied");
}
// Screen readers hear it through a live region inside the dialog: the page behind a modal is inert.
toast.live = (text) => { const el = $("#cfg-live") || $("#toast"); el.textContent = ""; setTimeout(() => { el.textContent = text; }, 50); };
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

// The setup, one question per screen. The token step exists only for a profile:
// a project's own token already reads its private activity.
const STEPS = [
  { key: "widgets", next: "Continue" },
  { key: "target", next: "Continue" },
  { key: "token", next: "I added the secret", time: "about 1 min", profileOnly: true },
  { key: "workflow", next: "I committed it", time: "about 1 min" },
  { key: "readme", next: "I pasted it" },
  { key: "check" },
];

function visibleSteps() {
  return STEPS.filter((s) => !(s.profileOnly && state.mode === "project"));
}

function stepIndex(key) {
  return visibleSteps().findIndex((s) => s.key === key);
}

// Why the step can't be left yet, or "" when it can. Steps 3 to 5 happen on GitHub: nothing to check.
function stepBlock(key) {
  if (key === "widgets") return pickedWidgets().length ? "" : "Pick at least one widget to continue.";
  if (key !== "target") return "";
  const t = targetInfo();
  const project = state.mode === "project";
  if (t.error) return project ? "Fix the repository name to continue." : "Fix your login to continue.";
  if (!t.ok) return project ? "Type the repository to continue." : "Type your login to continue.";
  if (duelPending()) return rivalError() ? "Fix your friend’s login to continue." : "Type a friend’s login for the duel, or go back and remove it.";
  return "";
}

// The furthest step that can be shown: the first one that is not valid yet, or the last.
function lastReachable() {
  const steps = visibleSteps();
  const i = steps.findIndex((s) => stepBlock(s.key));
  return i === -1 ? steps.length - 1 : i;
}

function renderGallery() {
  $("#filters").innerHTML = ["All", ...CATEGORIES].map((c) =>
    `<button class="filter" type="button" data-filter="${c}" aria-pressed="${c === "All"}">${c}</button>`
  ).join("");
  $("#grid").innerHTML = WIDGETS.map((w) => {
    const id = w.id === "duel" ? "duel-&lt;login&gt;" : w.id;
    // Widgets far from 2:1 get a margin of the same colour, so none touches two edges and not the others.
    const padded = Math.abs(w.w / w.h / 2 - 1) > 0.12;
    return `<li class="card" data-cat="${CATEGORY[w.id]}" data-id="${w.id}">
      <div class="card-media${padded ? " is-padded" : ""}">
        <img src="widgets/${w.file || w.id}.gif" alt="${esc(w.alt)}" width="${w.w}" height="${w.h}" loading="lazy" decoding="async">
      </div>
      <div class="card-body">
        <h3 class="card-name">${esc(w.name)}</h3>
        <p class="card-id">${id}</p>
        <p class="card-desc">${esc(w.desc)}</p>
        <p class="card-tags"><span class="tag">${CATEGORY[w.id]}</span>${w.project ? "" : '<span class="tag quiet">Profile only</span>'}</p>
      </div>
    </li>`;
  }).join("");
}

function renderPicks() {
  $("#picks").innerHTML = WIDGETS.map((w) => `
    <li><button class="pick" type="button" data-id="${w.id}" aria-pressed="false">
      <span class="pick-media"><img src="widgets/${w.file || w.id}.gif" alt="" width="${w.w}" height="${w.h}" loading="lazy" decoding="async"></span>
      <span class="pick-name">${esc(w.name)}</span>
      <span class="pick-note" id="note-${w.id}"></span>
      <span class="pick-check" aria-hidden="true"><svg class="icon" viewBox="0 0 24 24"><path d="M20 6 9 17l-5-5"/></svg></span>
    </button></li>`).join("");
}

// The README of step 2, small: the picked GIFs side by side.
function renderStrip(t) {
  $("#strip-path").textContent = state.mode === "project" ? `${t.owner} / ${t.repo} / README.md` : `${t.owner} / README.md`;
  const list = pickedWidgets();
  $("#strip-row").innerHTML = list.map((w) => `<img src="widgets/${w.file || w.id}.gif" alt="${esc(w.alt)}" width="${w.w}" height="${w.h}">`).join("");
}

function update() {
  const t = targetInfo();
  const project = state.mode === "project";
  const pending = duelPending();
  const steps = visibleSteps();
  if (stepIndex(state.step) === -1) state.step = "widgets";
  const idx = stepIndex(state.step);
  const step = steps[idx];

  // Step 1: mode, picks, count.
  for (const b of $$(".mode button")) b.setAttribute("aria-pressed", String(b.dataset.mode === state.mode));
  $("#mode-help").textContent = project
    ? "For a repository README. The widgets show the activity of that repository."
    : "For your profile README, the one at the top of your GitHub profile.";
  for (const w of WIDGETS) {
    const ok = allowed(w);
    const on = state.picked.has(w.id) && ok;
    const pick = document.querySelector(`.pick[data-id="${w.id}"]`);
    pick.disabled = !ok;
    pick.setAttribute("aria-pressed", String(on));
    const note = pick.querySelector(".pick-note");
    note.textContent = !ok ? "Profile only" : on && w.id === "duel" ? "Friend’s login next" : "";
  }
  const picked = pickedWidgets().length;
  $("#picked-count").textContent = picked ? `${picked} selected` : "None selected yet";
  $("#pick-all").textContent = WIDGETS.filter(allowed).every((w) => state.picked.has(w.id)) ? "Clear" : "Select all";

  // Step 2: login or repository, friend for the duel.
  $("#pane-target").textContent = project ? "Which repository?" : "What’s your GitHub login?";
  $("#repo-help").textContent = project
    ? "The widgets show its commits on the default branch, its merged pull requests and its languages."
    : "Your profile README lives in a repository named like your login.";
  $("#target-label").textContent = project ? "Repository (owner/name)" : "Your GitHub login";
  $("#target").placeholder = project ? "owner/repo" : "your-login";
  $("#target").setAttribute("aria-invalid", String(!!t.error));
  const err = $("#target-error");
  err.hidden = !t.error;
  err.textContent = t.error || "";
  $("#repo-note").hidden = project;
  $("#repo-create").href = t.ok && !project
    ? `https://github.com/new?name=${encodeURIComponent(t.owner)}&visibility=public`
    : "https://github.com/new?visibility=public";
  $("#rival-field").hidden = !(state.picked.has("duel") && !project);
  const rErr = rivalError();
  $("#rival-error").hidden = !rErr;
  $("#rival-error").textContent = rErr;
  $("#rival").setAttribute("aria-invalid", String(!!rErr));
  renderStrip(t);

  // Steps 3 to 6: links into the visitor's repository.
  const repoUrl = `https://github.com/${t.owner}/${t.repo}`;
  $("#secret-link").href = `${repoUrl}/settings/secrets/actions/new`;
  const yaml = buildYaml();
  const html = buildHtml(t);
  renderCode($("#yaml"), yaml, "yaml");
  renderCode($("#html"), html || "<!-- Pick a widget: its image line appears here. -->", "html");
  renderPreview(t);
  const count = activeWidgets().length;
  for (const b of $$(".copy")) b.disabled = !count;
  $("#commit-help").textContent = project
    ? `It draws the GIFs every night on a separate lgtm-md branch of ${t.owner}/${t.repo}, so your default branch gets no nightly commit.`
    : `It draws the GIFs every night in the widgets folder of ${t.owner}/${t.repo}.`;
  const create = $("#create");
  const ready = t.ok && count > 0;
  create.setAttribute("aria-disabled", String(!ready));
  // A disabled link has no href, so neither a click nor the keyboard can follow it.
  if (ready) {
    // HEAD resolves to the default branch, whatever its name.
    create.href = `${repoUrl}/new/HEAD?filename=${encodeURIComponent(".github/workflows/lgtm-md.yml")}&value=${encodeURIComponent(yaml)}`;
    create.target = "_blank";
  } else {
    create.removeAttribute("href");
    create.removeAttribute("target");
  }
  const hint = $("#create-hint");
  if (!count) hint.textContent = pending ? "Type a friend’s login for the duel." : "Pick a widget first.";
  else if (t.error) hint.textContent = "Fix your login first.";
  else if (!t.ok) hint.textContent = project ? "Type the repository first." : "Type your login first.";
  else hint.textContent = "";
  hint.hidden = !hint.textContent;
  $("#readme-link").href = repoUrl;
  $("#actions-link").href = `${repoUrl}/actions`;
  const result = $("#result-link");
  result.href = project ? repoUrl : `https://github.com/${t.owner}`;
  result.textContent = project ? "Open your repository ↗" : "Open your GitHub profile ↗";
  $("#tip-secret").hidden = project;

  // Shell: header, progress bar, the current pane, the footer.
  $("#cfg-count").innerHTML = `Step ${idx + 1} of ${steps.length}${step.time ? ` <span class="cfg-time">· ${step.time}</span>` : ""}`;
  // The segments are kept between steps so each one can fill in its pixel steps.
  const bar = $("#cfg-bar");
  if (bar.children.length !== steps.length) bar.innerHTML = steps.map(() => "<li></li>").join("");
  [...bar.children].forEach((li, i) => li.classList.toggle("on", i <= idx));
  for (const pane of $$(".pane")) pane.hidden = pane.dataset.pane !== state.step;
  const block = stepBlock(state.step);
  const next = $("#next");
  next.hidden = !step.next;
  next.textContent = step.next || "";
  next.disabled = !!block;
  $("#cfg-reason").textContent = block;
  $("#cfg-reason").hidden = !block;
  $("#back").hidden = idx === 0;
  $("#skip-token").hidden = state.step !== "token";
  $("#close-text").hidden = state.step !== "check";

  saveUrl();
}

// The selection lives in the address bar, so a link restores it, and #setup opens the setup.
function saveUrl() {
  const p = new URLSearchParams();
  const ids = WIDGETS.filter((w) => state.picked.has(w.id)).map((w) => w.id);
  if (ids.length) p.set("w", ids.join(","));
  if (state.target.trim()) p.set("u", state.target.trim());
  if (state.mode !== "profile") p.set("m", state.mode);
  if (state.rival.trim()) p.set("r", state.rival.trim());
  if (state.step !== "widgets") p.set("s", String(STEPS.findIndex((s) => s.key === state.step) + 1));
  const q = p.toString().replace(/%2C/g, ",");
  const url = location.pathname + (q ? `?${q}` : "") + (state.open ? "#setup" : "");
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
  // A shared step never skips a step that is not filled in yet.
  const s = STEPS[parseInt(p.get("s"), 10) - 1];
  const steps = visibleSteps();
  if (s && stepIndex(s.key) !== -1) state.step = steps[Math.min(stepIndex(s.key), lastReachable())].key;
  return p.has("w") || location.hash === "#setup";
}

function goTo(key, focus = true) {
  if (stepIndex(key) === -1 || stepIndex(key) > lastReachable()) return;
  const back = stepIndex(key) < stepIndex(state.step);
  state.step = key;
  update();
  $("#cfg-body").scrollTop = 0;
  enterPane(back);
  if (focus) document.getElementById(`pane-${key}`).focus({ preventScroll: true });
}

// A short slide of the new step's content; the frame never moves.
function enterPane(back) {
  const pane = $(`.pane[data-pane="${state.step}"]`);
  pane.classList.remove("is-entering", "from-left");
  void pane.offsetWidth;
  pane.classList.add("is-entering");
  pane.classList.toggle("from-left", back);
  pane.addEventListener("animationend", () => pane.classList.remove("is-entering", "from-left"), { once: true });
  if (state.step === "check") buildMark($("#setup .done-mark"));
}

function move(delta) {
  const steps = visibleSteps();
  const i = stepIndex(state.step) + delta;
  if (i >= 0 && i < steps.length) goTo(steps[i].key);
}

let opener = null;

function openSetup(from) {
  const dialog = $("#setup");
  if (dialog.open) return;
  opener = from || document.activeElement;
  state.open = true;
  document.documentElement.classList.add("modal-open");
  dialog.showModal();
  update();
  $("#cfg-body").scrollTop = 0;
  if (state.step === "check") buildMark($("#setup .done-mark"));
  document.getElementById(`pane-${state.step}`).focus({ preventScroll: true });
}

function onClose() {
  state.open = false;
  document.documentElement.classList.remove("modal-open");
  saveUrl();
  if (opener && document.contains(opener)) opener.focus();
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

// Profiles that run LGTM.md, from wild.json (updated weekly by .github/workflows/used-by.yml):
// one round avatar each. The row shows from five profiles.
const USED_MIN = 5;
async function renderUsed() {
  try {
    const res = await fetch("wild.json", { cache: "no-cache" });
    if (!res.ok) return;
    const list = (await res.json()).filter((e) => e && LOGIN_RE.test(e.login || "")).slice(0, 24);
    if (list.length < USED_MIN) return;
    // Avatars come from the GitHub API (scripts/used-by.mjs); older entries fall back to github.com/<login>.png.
    const avatar = (e) => /^https:\/\/avatars\.githubusercontent\.com\//.test(e.avatar || "") ? `${e.avatar}${e.avatar.includes("?") ? "&" : "?"}s=64` : `https://github.com/${e.login}.png?size=64`;
    $("#used-list").innerHTML = list.map((e) => `<li><a href="https://github.com/${e.login}" target="_blank" rel="noopener" title="${e.login}" aria-label="${e.login} on GitHub"><img src="${esc(avatar(e))}" alt="" width="32" height="32" loading="lazy"></a></li>`).join("");
    $("#used").hidden = false;
  } catch { /* the row stays hidden */ }
}

// Pixel build: the engine behind every pixel moment of the page. Each slot fills with blocks of
// its own still image that fall and stack from the bottom row up, the mosaic sharpens in a few
// steps, then the real GIF takes over. One-shot; callers skip it with reduced motion.
const PIXEL = {
  rows: 620,             // ms between the bottom row and the top row starting to fall
  jitter: 140,           // random extra delay per block, ms
  fall: 420,             // ms a block takes to land
  drop: [3, 9],          // fall height, in blocks (min, max)
  stepMs: 90,            // ms per sharpening step
  fade: 220,             // ms of the hand-over to the GIF
  wait: 2500,            // after this, the GIF shows even if it has not loaded
};

function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

// The still reduced so one pixel stands for k×k of its pixels (averaged by the browser).
function mosaic(img, k) {
  const c = document.createElement("canvas");
  c.width = Math.max(1, Math.round(img.naturalWidth / k));
  c.height = Math.max(1, Math.round(img.naturalHeight / k));
  const g = c.getContext("2d", { willReadFrequently: true });
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = "high";
  g.drawImage(img, 0, 0, c.width, c.height);
  return c;
}

// slots: [{ rect: [x, y, w, h] in CSS px from the canvas' top left, still: loaded Image,
//           start: ms, cell: still pixels per block, sharpen: [still pixels per step...],
//           ready: () => the GIF can show, onLive: () => show it }]
// Returns { done: Promise, stop() }; stop() shows every GIF at once.
function pixelBuild(canvas, slots, options = {}) {
  const o = { ...PIXEL, ...options };
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const box = canvas.getBoundingClientRect();
  canvas.width = Math.round(box.width * dpr);
  canvas.height = Math.round(box.height * dpr);
  const ctx = canvas.getContext("2d");
  const D = (v) => Math.round(v * dpr);

  const plans = slots.map((slot) => {
    const [x, y, w, h] = slot.rect;
    const levels = [slot.cell, ...slot.sharpen].map((k) => mosaic(slot.still, k));
    const coarse = levels[0];
    const cols = coarse.width, rowsN = coarse.height;
    const px = coarse.getContext("2d").getImageData(0, 0, cols, rowsN).data;
    const bw = w / cols, bh = h / rowsN;
    const start = slot.start || 0;
    const blocks = [];
    for (let row = 0; row < rowsN; row++) {
      for (let col = 0; col < cols; col++) {
        const i = (row * cols + col) * 4;
        const fromBottom = (rowsN - 1 - row) / Math.max(1, rowsN - 1);
        blocks.push({
          x0: D(x + col * bw), x1: D(x + (col + 1) * bw),
          y0: D(y + row * bh), y1: D(y + (row + 1) * bh),
          color: `rgb(${px[i]},${px[i + 1]},${px[i + 2]})`,
          delay: start + fromBottom * o.rows + Math.random() * o.jitter,
          drop: o.drop[0] + Math.floor(Math.random() * (o.drop[1] - o.drop[0] + 1)),
        });
      }
    }
    const landed = start + o.rows + o.jitter + o.fall;
    return { slot, levels, blocks, bh: D(bh), rect: [D(x), D(y), D(x + w) - D(x), D(y + h) - D(y)], landed, live: false, fadeAt: 0 };
  });

  let raf = 0, over = false, resolve;
  const done = new Promise((r) => { resolve = r; });
  const stop = () => {
    if (over) return;
    over = true;
    cancelAnimationFrame(raf);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    for (const p of plans) if (!p.live) { p.live = true; p.slot.onLive(); }
    resolve();
  };
  const easeOut = (p) => 1 - Math.pow(1 - p, 3);
  const t0 = performance.now();
  const frame = (now) => {
    const t = now - t0;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    let busy = false;
    for (const p of plans) {
      const sharpenAt = t - p.landed;
      if (sharpenAt < 0) {
        busy = true;
        // Each slot is a well: blocks enter from its top edge and stack inside it.
        ctx.save();
        ctx.beginPath();
        ctx.rect(...p.rect);
        ctx.clip();
        for (const b of p.blocks) {
          const q = (t - b.delay) / o.fall;
          if (q <= 0) continue;
          // Stepped motion: the block moves down whole block heights and fades in three steps.
          const lift = q >= 1 ? 0 : Math.round(b.drop * (1 - easeOut(q))) * p.bh;
          ctx.globalAlpha = Math.min(1, Math.ceil(q * 3) / 3);
          ctx.fillStyle = b.color;
          ctx.fillRect(b.x0, b.y0 - lift, b.x1 - b.x0, b.y1 - b.y0);
        }
        ctx.restore();
        continue;
      }
      // Sharpen through the mosaic levels, then hand over to the GIF once it has loaded.
      const step = Math.min(p.levels.length - 1, 1 + Math.floor(sharpenAt / o.stepMs));
      const sharp = sharpenAt >= o.stepMs * (p.levels.length - 1);
      if (sharp && !p.live && (p.slot.ready() || sharpenAt > o.wait)) {
        p.live = true;
        p.fadeAt = t;
        p.slot.onLive();
      }
      const alpha = p.live ? 1 - (t - p.fadeAt) / o.fade : 1;
      if (alpha <= 0) continue;
      busy = true;
      ctx.globalAlpha = alpha;
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(p.levels[step], ...p.rect);
      ctx.globalAlpha = 1;
    }
    if (busy) raf = requestAnimationFrame(frame);
    else stop();
  };
  raf = requestAnimationFrame(frame);
  return { done, stop };
}

const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const gifReady = (img) => img.complete && img.naturalWidth > 0;

// Hero: the three slots of the README card, one after the other. The head script hides the GIFs
// (class hero-intro) only when motion is allowed, and shows them after 4 s whatever happens.
async function heroIntro() {
  const root = document.documentElement;
  const art = $(".hero-art");
  if (!art || !root.classList.contains("hero-intro")) return;
  const canvas = art.querySelector(".hero-fx");
  const slots = [...art.querySelectorAll(".slot")];
  let run = null, over = false;
  const finish = () => {
    if (over) return;
    over = true;
    if (run) run.stop();
    for (const s of slots) s.classList.add("is-live");
    root.classList.remove("hero-intro");
    window.removeEventListener("resize", finish);
  };
  if (document.hidden || window.scrollY > art.offsetTop) return finish();
  window.addEventListener("resize", finish);

  let stills;
  try {
    const timeout = new Promise((_, reject) => setTimeout(reject, 1200));
    stills = await Promise.race([Promise.all(slots.map((s) => loadImage(s.dataset.still))), timeout]);
  } catch { return finish(); }
  if (over) return;
  const box = canvas.getBoundingClientRect();
  const starts = [0, 200, 340];
  run = pixelBuild(canvas, slots.map((slot, i) => {
    const r = slot.getBoundingClientRect();
    const img = slot.querySelector("img");
    return {
      rect: [r.left - box.left, r.top - box.top, r.width, r.height],
      still: stills[i], start: starts[i], cell: 10, sharpen: [5, 2, 1],
      ready: () => gifReady(img), onLive: () => slot.classList.add("is-live"),
    };
  }));
  run.done.then(finish);
}

// Gallery: a card's thumbnail assembles from its pixels the first time it scrolls into view.
// Cards that enter together are staggered; at most CARD_MAX builds run at once, the rest wait.
const CARD_GAP = 110;
const CARD_MAX = 4;
function galleryIntro() {
  if (reducedMotion() || !("IntersectionObserver" in window)) return;
  const cards = $$(".card");
  for (const c of cards) c.classList.add("is-assembling");
  const show = (card) => card.classList.remove("is-assembling");
  const runs = new Set();
  const queue = [];
  let lastStart = 0;

  const startCard = async (card) => {
    const w = BY_ID.get(card.dataset.id);
    const media = card.querySelector(".card-media");
    const img = media.querySelector("img");
    let still;
    try {
      still = await Promise.race([loadImage(`stills/${w.file || w.id}.png`), new Promise((_, r) => setTimeout(r, 1500))]);
    } catch { return show(card); }
    if (!card.classList.contains("is-assembling") || card.hidden) return show(card);
    // The GIF is contained in its box: the build covers exactly the drawn image.
    const m = media.getBoundingClientRect(), b = img.getBoundingClientRect();
    if (!m.width || !b.width) return show(card);
    const scale = Math.min(b.width / w.w, b.height / w.h);
    const dw = w.w * scale, dh = w.h * scale;
    const canvas = document.createElement("canvas");
    canvas.className = "card-fx";
    canvas.setAttribute("aria-hidden", "true");
    media.appendChild(canvas);
    // Blocks of about 6 CSS pixels, whatever the card width.
    const cell = still.naturalWidth / Math.max(8, dw / 6);
    const now = performance.now();
    const delay = Math.max(0, lastStart + CARD_GAP - now);
    lastStart = now + delay;
    const run = pixelBuild(canvas, [{
      rect: [b.left - m.left + (b.width - dw) / 2, b.top - m.top + (b.height - dh) / 2, dw, dh],
      still, start: delay, cell, sharpen: [cell / 2, cell / 4].filter((k) => k >= 1),
      ready: () => gifReady(img), onLive: () => show(card),
    }], { rows: 420, jitter: 120, fall: 360 });
    runs.add(run);
    await run.done;
    runs.delete(run);
    canvas.remove();
    if (queue.length) startCard(queue.shift());
  };

  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      io.unobserve(e.target);
      if (runs.size < CARD_MAX) startCard(e.target); else queue.push(e.target);
    }
  }, { threshold: 0.35 });
  for (const c of cards) io.observe(c);
  // A resize or a filter change ends every build: the GIFs show as they are.
  const stopAll = () => { for (const r of runs) r.stop(); };
  window.addEventListener("resize", stopAll);
  $("#filters").addEventListener("click", () => {
    stopAll();
    for (const c of queue) show(c);
    queue.length = 0;
  });
}

// Pixel marks that build themselves: the 16×16 mark is split into one rect per grid pixel, each
// dropping in with a delay that starts from the bottom row. Called again to replay.
function buildMark(host) {
  if (!host) return;
  const svg = host.querySelector("svg.mark");
  if (!svg.dataset.split) {
    const ns = "http://www.w3.org/2000/svg";
    const cells = [];
    for (const path of [...svg.querySelectorAll("path")]) {
      for (const m of path.getAttribute("d").matchAll(/M(\d+) (\d+)h(\d+)v(\d+)/g)) {
        const [x, y, w, h] = m.slice(1).map(Number);
        for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) cells.push([x + i, y + j, path.getAttribute("class")]);
      }
      path.remove();
    }
    for (const [x, y, cls] of cells) {
      const r = document.createElementNS(ns, "rect");
      r.setAttribute("x", x);
      r.setAttribute("y", y);
      r.setAttribute("width", 1);
      r.setAttribute("height", 1);
      if (cls) r.setAttribute("class", cls);
      svg.appendChild(r);
    }
    svg.dataset.split = "1";
  }
  if (reducedMotion()) return;
  for (const r of svg.querySelectorAll("rect")) {
    const y = Number(r.getAttribute("y"));
    r.style.setProperty("--d", `${Math.round((15 - y) * 38 + Math.random() * 90)}ms`);
  }
  svg.classList.remove("is-building");
  void svg.getBoundingClientRect();
  svg.classList.add("is-building");
}

// The closing band's mark builds once, when the band comes into view.
function closingIntro() {
  const host = $(".closing-mark");
  if (!host) return;
  if (reducedMotion() || !("IntersectionObserver" in window)) return;
  host.style.visibility = "hidden";
  const io = new IntersectionObserver((entries) => {
    if (!entries.some((e) => e.isIntersecting)) return;
    io.disconnect();
    host.style.visibility = "";
    buildMark(host);
  }, { threshold: 0.6 });
  io.observe(host);
}

function init() {
  document.documentElement.classList.add("js");
  for (const el of $$("[data-version]")) el.textContent = VERSION;
  loadStars();
  // compare.html shares this script for the star count only.
  if (!$("#setup")) return;
  heroIntro();

  renderGallery();
  galleryIntro();
  closingIntro();
  renderPicks();
  const open = loadUrl();
  const dialog = $("#setup");

  document.addEventListener("click", (e) => {
    const openBtn = e.target.closest("[data-open]");
    if (openBtn) return openSetup(openBtn);
    if (e.target.closest("[data-close]")) return dialog.close();
    const pick = e.target.closest(".pick");
    if (pick && !pick.disabled) {
      const id = pick.dataset.id;
      state.picked.has(id) ? state.picked.delete(id) : state.picked.add(id);
      return update();
    }
    const mode = e.target.closest(".mode button");
    if (mode) return setMode(mode.dataset.mode);
    const filter = e.target.closest(".filter");
    if (filter) {
      for (const f of $$(".filter")) f.setAttribute("aria-pressed", String(f === filter));
      const c = filter.dataset.filter;
      for (const card of $$(".card")) card.hidden = c !== "All" && card.dataset.cat !== c;
      return;
    }
    const copyBtn = e.target.closest(".copy");
    if (copyBtn && !copyBtn.disabled) return copy(activeWidgets().length ? $("#" + copyBtn.dataset.copy).dataset.raw : "", copyBtn);
    if (e.target.closest("#pick-all")) {
      const all = WIDGETS.filter(allowed);
      const full = all.every((w) => state.picked.has(w.id));
      for (const w of all) full ? state.picked.delete(w.id) : state.picked.add(w.id);
      return update();
    }
    if (e.target.closest("#next")) return state.step === visibleSteps().at(-1).key ? dialog.close() : move(1);
    if (e.target.closest("#back")) return move(-1);
    if (e.target.closest("#skip-token")) return move(1);
  });
  // A click on the dimmed backdrop lands on the dialog itself, outside its content.
  dialog.addEventListener("click", (e) => { if (e.target === dialog) dialog.close(); });
  dialog.addEventListener("close", onClose);
  $("#target").addEventListener("input", (e) => { state.target = e.target.value; update(); });
  $("#rival").addEventListener("input", (e) => { state.rival = e.target.value; update(); });
  // Enter in a field moves on when the step is valid.
  for (const input of [$("#target"), $("#rival")]) {
    input.addEventListener("keydown", (e) => { if (e.key === "Enter" && !stepBlock(state.step)) { e.preventDefault(); move(1); } });
  }
  // The mode switch also moves with the arrow keys.
  for (const group of $$(".mode")) {
    group.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      e.preventDefault();
      setMode(state.mode === "profile" ? "project" : "profile");
      group.querySelector(`[data-mode="${state.mode}"]`).focus();
    });
  }
  window.addEventListener("hashchange", () => { if (location.hash === "#setup") openSetup(); });

  update();
  renderUsed();
  if (open) openSetup($("main [data-open]"));
}

init();
