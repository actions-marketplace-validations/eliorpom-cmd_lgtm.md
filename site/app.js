// Launch page of LGTM.md: widget gallery and a setup that writes the workflow and README code.
"use strict";

// The release the generated workflow pins. Change it here only.
const VERSION = "v1.2.0";
const REPO_URL = "https://github.com/eliorpom-cmd/lgtm.md";

// size: "wide" spans the whole row. width: the README width that keeps the GIF sharp but not huge.
const WIDGETS = [
  { id: "grass", w: 1600, h: 744, size: "wide", width: "100%", project: true,
    desc: "The last 90 days, one tuft of grass per day. Flowers are your languages, fireflies your merged pull requests.",
    alt: "Night field: one tuft of grass per day over the last 90 days" },
  { id: "flame", w: 832, h: 552, width: "49%", project: true,
    desc: "Your current streak of active days. The fire grows at 7, 30 and 100 days.",
    alt: "Campfire for a streak of active days" },
  { id: "counter", w: 832, h: 552, width: "49%", project: true,
    desc: "Contributions over the last 12 months, rolling up like an odometer.",
    alt: "Drum counter of contributions over 12 months" },
  { id: "departures", w: 1600, h: 424, size: "wide", width: "100%", project: true,
    desc: "A station board that flips through four stats: contributions, merged pull requests, streak and top language.",
    alt: "Split-flap departure board" },
  { id: "hourglass", w: 1600, h: 744, width: "100%", project: true,
    desc: "The past year's commits as sand, one colour per language, oldest at the bottom.",
    alt: "Hourglass of coloured sand, one grain per batch of commits" },
  { id: "forest", w: 1600, h: 744, width: "100%", project: true,
    desc: "The last 12 months, one tree per week. A quiet week leaves a stump.",
    alt: "Forest with one tree per week of the year" },
  { id: "ecg", w: 1600, h: 552, width: "100%", project: true,
    desc: "The last 7 days as a heartbeat, with the best day highlighted.",
    alt: "Heartbeat of the last 7 days on a monitor" },
  { id: "records", w: 1600, h: 552, width: "100%", project: true,
    desc: "Best day, longest streak and longest break of the last 12 months.",
    alt: "Three brass plaques: best day, longest streak, longest break" },
  { id: "cherry-tree", w: 1600, h: 744, width: "100%", project: true,
    desc: "The last 90 days as a blossoming crown. Each falling petal is a merged pull request.",
    alt: "Cherry tree in bloom at dusk" },
  { id: "donut", w: 1280, h: 552, width: "80%", project: true,
    desc: "Your top languages as slices of icing, sized by their share of the code.",
    alt: "Doughnut iced in one colour per language" },
  { id: "fireworks", w: 1600, h: 744, width: "100%", project: true,
    desc: "One rocket per pull request merged in the last 7 days, coloured by language.",
    alt: "Fireworks over a lake, one rocket per merged pull request" },
  { id: "fireworks-commits", w: 1600, h: 744, width: "100%", project: true,
    desc: "One rocket per batch of commits from the last 7 days.",
    alt: "Fireworks over a lake, one rocket per batch of commits" },
  { id: "median", w: 1120, h: 680, width: "70%", project: false,
    desc: "Your tower of contributions next to the median of the accounts you follow.",
    alt: "Two builders raising brick towers" },
  { id: "duel", w: 1600, h: 744, width: "100%", project: false, file: "duel-rival",
    desc: "You against one friend over the last 30 days, day by day, with your avatars.",
    alt: "Duel between two accounts over 30 days" },
];

const LOGIN_RE = /^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$/;
const REPO_RE = /^([A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38})\/([A-Za-z0-9._-]{1,100})$/;


const state = {
  mode: "profile",
  target: "",
  rival: "",
  picked: new Set(),
};

const $ = (sel) => document.querySelector(sel);
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function allowed(widget) {
  return state.mode === "profile" || widget.project;
}

// Widgets that can go in the workflow right now, in gallery order.
function activeWidgets() {
  return WIDGETS.filter((w) => state.picked.has(w.id) && allowed(w));
}

function rivalLogin() {
  const r = state.rival.trim().replace(/^@/, "");
  return LOGIN_RE.test(r) ? r : "";
}

// The name the Action expects, and the GIF it writes.
function actionName(widget) {
  if (widget.id === "duel") return rivalLogin() ? `duel-${rivalLogin()}` : "";
  return widget.id;
}

function targetInfo() {
  const raw = state.target.trim().replace(/^@/, "").replace(/^https?:\/\/github\.com\//, "").replace(/\/+$/, "");
  if (state.mode === "profile") {
    if (!raw) return { ok: false, owner: "octocat", repo: "octocat", empty: true };
    return LOGIN_RE.test(raw) ? { ok: true, owner: raw, repo: raw } : { ok: false, owner: "octocat", repo: "octocat", error: "A GitHub login has letters, digits and single hyphens, up to 39 characters." };
  }
  if (!raw) return { ok: false, owner: "octocat", repo: "hello-world", empty: true };
  const m = raw.match(REPO_RE);
  return m ? { ok: true, owner: m[1], repo: m[2] } : { ok: false, owner: "octocat", repo: "hello-world", error: "Write the repository as owner/name, for example octocat/hello-world." };
}

// Same workflow as the README. A project publishes its GIFs on a separate lgtm-md branch:
// a nightly commit on the default branch would count as the project's own activity.
function buildYaml() {
  const names = activeWidgets().map(actionName).filter(Boolean);
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
    "",
    "concurrency: lgtm-md",
    "",
    "jobs:",
    "  draw:",
    "    runs-on: ubuntu-latest",
    "    steps:",
  ];
  if (!project) lines.push("      - uses: actions/checkout@v4");
  lines.push(`      - uses: eliorpom-cmd/lgtm.md@${VERSION}`, "        with:");
  lines.push(project ? "          repo: ${{ github.repository }}" : "          token: ${{ secrets.LGTM_MD_TOKEN || github.token }}");
  lines.push("          widgets: |");
  for (const n of names) lines.push(`            ${n}`);
  if (project) {
    lines.push(
      "          output-dir: lgtm-md",
      "      - name: Publish the widgets on the lgtm-md branch",
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
      "        run: |",
      "          git add widgets",
      "          git diff --cached --quiet && exit 0",
      '          git config user.name "github-actions[bot]"',
      '          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"',
      `          git commit -m "widgets: draw the day's widgets"`,
      "          git push",
      "",
    );
  }
  return lines.join("\n");
}

function buildHtml(t) {
  const base = state.mode === "project" ? `https://github.com/${t.owner}/${t.repo}/raw/lgtm-md/` : "widgets/";
  const out = activeWidgets()
    .filter((w) => actionName(w))
    .map((w) => `<a href="${REPO_URL}"><img src="${base}${actionName(w)}.gif" alt="${w.alt}" width="${w.width}"></a>`);
  if (out.length) out.push("", `<sub>Made with <a href="${REPO_URL}">LGTM.md</a></sub>`);
  return out.join("\n");
}

function renderGrid() {
  const grid = $("#grid");
  grid.innerHTML = WIDGETS.map((w) => {
    const file = w.file || w.id;
    const label = w.id === "duel" ? "duel-&lt;login&gt;" : w.id;
    return `<li class="card${w.size === "wide" ? " wide" : ""}" data-id="${w.id}">
      <div class="card-media" style="--ratio:${w.w} / ${w.h}">
        <img src="widgets/${file}.gif" alt="${esc(w.alt)}" width="${w.w}" height="${w.h}" loading="lazy" decoding="async">
      </div>
      <div class="card-body">
        <div>
          <h3 class="card-name">${label}<span class="card-tag" hidden>Profile only</span></h3>
          <p class="card-desc">${w.desc}</p>
        </div>
        <button class="toggle" type="button" data-id="${w.id}" aria-pressed="false" aria-label="Add ${w.id}"></button>
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

  // Mode switch and the texts that depend on it.
  for (const b of document.querySelectorAll(".mode button")) b.setAttribute("aria-selected", String(b.dataset.mode === state.mode));
  $("#mode-help").textContent = project
    ? "For a repository’s README."
    : "For your profile README.";
  $("#target-label").textContent = project ? "Repository (owner/name)" : "Your GitHub login";
  $("#target").placeholder = project ? "octocat/hello-world" : "octocat";
  const err = $("#target-error");
  err.hidden = !t.error;
  err.textContent = t.error || "";

  // Gallery cards and chips share one selection.
  for (const w of WIDGETS) {
    const ok = allowed(w);
    const on = state.picked.has(w.id) && ok;
    const card = document.querySelector(`.card[data-id="${w.id}"]`);
    card.dataset.picked = String(on);
    card.querySelector(".card-tag").hidden = ok;
    const toggle = card.querySelector(".toggle");
    toggle.disabled = !ok;
    toggle.setAttribute("aria-pressed", String(on));
    toggle.textContent = on ? "Added" : "Add";
    const chip = document.querySelector(`.chip[data-id="${w.id}"]`);
    chip.disabled = !ok;
    chip.setAttribute("aria-pressed", String(on));
  }
  const count = activeWidgets().length;
  $("#bar-count").textContent = `${count} widget${count === 1 ? "" : "s"} selected`;
  $("#bar").hidden = count === 0 || state.setupVisible;
  $("#rival-field").hidden = !(state.picked.has("duel") && !project);
  $("#pick-all").textContent = WIDGETS.filter(allowed).every((w) => state.picked.has(w.id)) ? "Clear" : "Select all";

  // Snippets and the link that opens GitHub's new-file page with the workflow filled in.
  const yaml = buildYaml();
  const html = buildHtml(t);
  const names = activeWidgets().map(actionName).filter(Boolean);
  $("#yaml").textContent = yaml;
  $("#html").textContent = html || "<!-- Pick at least one widget above. -->";
  const repo = `${t.owner}/${t.repo}`;
  $("#step1-help").textContent = project
    ? `Commit this file to ${repo}. The GIFs appear on its lgtm-md branch a minute later, then redraw every night. No token needed.`
    : `Commit this file to ${repo}. The GIFs appear in its widgets/ folder a minute later, then redraw every night.`;

  const create = $("#create");
  const ready = t.ok && names.length > 0;
  create.setAttribute("aria-disabled", String(!ready));
  create.href = ready
    ? `https://github.com/${t.owner}/${t.repo}/new/main?filename=${encodeURIComponent(".github/workflows/lgtm-md.yml")}&value=${encodeURIComponent(yaml)}`
    : "#setup";
  const hint = $("#create-hint");
  if (!names.length) {
    hint.textContent = "Pick a widget first.";
  } else if (!t.ok) {
    hint.textContent = project ? "Type the repository first." : "Type your login first.";
  } else if (project) {
    hint.innerHTML = `Opens the <code>main</code> branch. Change it in the address bar if yours has another name.`;
  } else {
    hint.innerHTML = `No profile repository? <a href="https://github.com/new?name=${encodeURIComponent(t.owner)}&amp;visibility=public" target="_blank" rel="noopener">Create a public repository named ${esc(t.owner)}</a>.`;
  }

  // Private activity needs a token only for a profile: a project's own token already reads it.
  $("#private").hidden = project;
}

function toast(text) {
  const el = $("#toast");
  el.textContent = text;
  el.classList.add("show");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove("show"), 1600);
}

async function copy(text) {
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

function toggleWidget(id) {
  if (state.picked.has(id)) state.picked.delete(id);
  else state.picked.add(id);
  // The selection bar hides once the setup is on screen.
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(([en]) => { state.setupVisible = en.isIntersecting; update(); }, { rootMargin: "0px 0px -40% 0px" }).observe($("#setup"));
  }

  update();
}

function init() {
  document.documentElement.classList.add("js");
  for (const el of document.querySelectorAll("[data-version]")) el.textContent = VERSION;
  renderGrid();
  renderChips();

  document.addEventListener("click", (e) => {
    const toggle = e.target.closest(".toggle, .chip");
    if (toggle && !toggle.disabled) return toggleWidget(toggle.dataset.id);
    const mode = e.target.closest(".mode button");
    if (mode) {
      state.mode = mode.dataset.mode;
      state.target = "";
      $("#target").value = "";
      return update();
    }
    const copyBtn = e.target.closest(".copy");
    if (copyBtn) return copy($("#" + copyBtn.dataset.copy).textContent);
    if (e.target.closest("#pick-all")) {
      const all = WIDGETS.filter(allowed);
      const full = all.every((w) => state.picked.has(w.id));
      for (const w of all) full ? state.picked.delete(w.id) : state.picked.add(w.id);
      return update();
    }
  });
  $("#target").addEventListener("input", (e) => { state.target = e.target.value; update(); });
  $("#rival").addEventListener("input", (e) => { state.rival = e.target.value; update(); });

  // Mode tabs also move with the arrow keys.
  $(".mode").addEventListener("keydown", (e) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    state.mode = state.mode === "profile" ? "project" : "profile";
    update();
    $(`#mode-${state.mode}`).focus();
  });

  update();
}

init();
