#!/usr/bin/env node
// Finds the public GitHub profiles that run LGTM.md and writes site/wild.json for the "Used by" row.
//
// A profile counts when a public, non-fork repository named exactly like its owner (the profile
// README repository) has a workflow file that mentions eliorpom-cmd/lgtm.md. Logins listed in
// site/used-by-optout.json are never listed. Profiles found before are kept when one search
// misses them; a profile not seen for DROP_AFTER_DAYS is dropped.
//
// Usage: GITHUB_TOKEN=... node scripts/used-by.mjs [--dry-run]
//   --dry-run  prints the result and leaves site/wild.json untouched.
// Exit code 0 in every normal case; the caller commits site/wild.json if git sees a change.

import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const WILD = path.join(ROOT, "site", "wild.json");
const OPTOUT = path.join(ROOT, "site", "used-by-optout.json");
const QUERY = '"eliorpom-cmd/lgtm.md" path:.github/workflows';
const API = "https://api.github.com";
const PER_PAGE = 100;
const MAX_RESULTS = 1000;      // the search API never returns more than this
const SEARCH_GAP_MS = 6500;    // code search allows 10 requests per minute
const MAX_TRIES = 6;
const DROP_AFTER_DAYS = 60;
const LOGIN_RE = /^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$/;

const token = process.env.GITHUB_TOKEN;
const dryRun = process.argv.includes("--dry-run");
if (!token) {
  console.error("GITHUB_TOKEN is not set.");
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const today = new Date().toISOString().slice(0, 10);

// One GET with retries: 403/429 rate limits wait for the reset (or back off), 5xx back off.
async function get(url) {
  for (let attempt = 1; ; attempt++) {
    const res = await fetch(url, {
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${token}`,
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "lgtm-md-used-by",
      },
    });
    if (res.ok) return res.json();
    const limited = res.status === 429 || (res.status === 403 && (res.headers.get("x-ratelimit-remaining") === "0" || res.headers.get("retry-after")));
    const retryable = limited || res.status >= 500;
    if (!retryable || attempt >= MAX_TRIES) {
      const body = await res.text().catch(() => "");
      throw new Error(`${res.status} on ${url}: ${body.slice(0, 300)}`);
    }
    let wait = Math.min(60_000 * 2 ** (attempt - 1), 300_000);
    const after = Number(res.headers.get("retry-after"));
    const reset = Number(res.headers.get("x-ratelimit-reset"));
    if (after > 0) wait = after * 1000;
    else if (reset > 0 && res.headers.get("x-ratelimit-remaining") === "0") wait = Math.max(1000, reset * 1000 - Date.now() + 1000);
    console.error(`  ${res.status}, waiting ${Math.round(wait / 1000)} s (try ${attempt}/${MAX_TRIES})`);
    await sleep(wait);
  }
}

async function readJson(file, fallback) {
  try {
    return JSON.parse(await readFile(file, "utf8"));
  } catch {
    return fallback;
  }
}

// Every public profile repository whose workflows mention the Action, keyed by lowercase login.
async function search() {
  const found = new Map();
  let total = Infinity;
  for (let page = 1; (page - 1) * PER_PAGE < Math.min(total, MAX_RESULTS); page++) {
    if (page > 1) await sleep(SEARCH_GAP_MS);
    const url = `${API}/search/code?q=${encodeURIComponent(QUERY)}&per_page=${PER_PAGE}&page=${page}`;
    const data = await get(url);
    total = data.total_count ?? 0;
    if (data.incomplete_results) console.error("  the search timed out on GitHub's side; results may be partial");
    const items = data.items || [];
    for (const item of items) {
      const repo = item.repository || {};
      const owner = repo.owner?.login || "";
      if (repo.private || repo.fork) continue;
      if (repo.name?.toLowerCase() !== owner.toLowerCase()) continue;
      if (!LOGIN_RE.test(owner)) continue;
      found.set(owner.toLowerCase(), { login: owner, repo: repo.full_name });
    }
    if (items.length < PER_PAGE) break;
  }
  console.error(`Search: ${total} files, ${found.size} profile repositories`);
  return found;
}

// The user's current login and avatar, or null when the account is gone or not a user.
async function profile(login) {
  try {
    const u = await get(`${API}/users/${encodeURIComponent(login)}`);
    if (u.type !== "User") return null;
    return { login: u.login, avatar: u.avatar_url };
  } catch (e) {
    console.error(`  skipped ${login}: ${e.message.split(":")[0]}`);
    return null;
  }
}

const daysBetween = (a, b) => Math.round((Date.parse(b) - Date.parse(a)) / 86_400_000);

async function main() {
  const optout = new Set((await readJson(OPTOUT, [])).map((l) => String(l).toLowerCase()));
  const before = await readJson(WILD, []);
  const known = new Map();
  for (const e of Array.isArray(before) ? before : []) {
    if (!e || !LOGIN_RE.test(e.login || "")) continue;
    // Entries from before this script (v5) carry "captured" instead of first_seen and last_seen.
    const first = e.first_seen || e.captured || today;
    known.set(e.login.toLowerCase(), { login: e.login, avatar: e.avatar || "", first_seen: first, last_seen: e.last_seen || first });
  }

  const found = await search();
  for (const [key, hit] of found) {
    if (optout.has(key)) continue;
    const p = await profile(hit.login);
    if (!p) continue;
    const old = known.get(key);
    known.set(key, { login: p.login, avatar: p.avatar, first_seen: old?.first_seen || today, last_seen: today });
  }

  const list = [...known.entries()]
    .filter(([key, e]) => !optout.has(key) && daysBetween(e.last_seen, today) <= DROP_AFTER_DAYS)
    .map(([, e]) => e)
    .sort((a, b) => a.first_seen.localeCompare(b.first_seen) || a.login.localeCompare(b.login));

  const text = JSON.stringify(list, null, 2) + "\n";
  console.log(text);
  if (dryRun) {
    console.error("Dry run: site/wild.json left as it is.");
    return;
  }
  // Rewrite only when the logins or avatars changed: last_seen alone moving every week would
  // otherwise make a commit each run.
  const sig = (arr) => JSON.stringify((Array.isArray(arr) ? arr : []).map((e) => [e.login, e.avatar || "", e.first_seen || e.captured || ""]));
  if (sig(before) === sig(list) && before.every((e) => e.last_seen && daysBetween(e.last_seen, today) < 28)) {
    console.error("No change.");
    return;
  }
  await writeFile(WILD, text);
  console.error(`Wrote ${list.length} profiles to site/wild.json`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
