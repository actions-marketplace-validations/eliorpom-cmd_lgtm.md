//! Port of `collect.py`: reads an account's activity from GitHub's GraphQL API into data.json.
//!
//! Usage: lgtm-md collect <login> [out.json] [friend...]
//! Token: `GITHUB_TOKEN`, else `GH_TOKEN`, else `gh auth token`. A personal token with the
//! `repo` scope sees private contributions, private repositories and their languages.
//!
//! Dates: Python mixes the local date (`date.today()`) and the UTC date; this port uses UTC
//! everywhere. GitHub Actions runners are on UTC, so the output is the same there.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::dates::{civil_from_days, days_from_civil, iso, today};

const DAYS: usize = 90;
const API: &str = "https://api.github.com/graphql";
/// Tries per query when GitHub times out.
const RETRIES: usize = 4;
/// Pages read per list at most (50 repositories or 10 followed accounts per page).
const MAX_PAGES: usize = 100;

type Result<T> = std::result::Result<T, String>;

struct Client {
    token: String,
    agent: ureq::Agent,
}

impl Client {
    fn new() -> Result<Client> {
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| {
                let out = std::process::Command::new("gh").args(["auth", "token"]).output().ok()?;
                let token = String::from_utf8(out.stdout).ok()?.trim().to_string();
                (!token.is_empty()).then_some(token)
            })
            .ok_or("no token: set GITHUB_TOKEN (or log in with `gh auth login`)")?;
        // The system's certificate store, so proxies with their own authority keep working.
        let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        Ok(Client { token, agent })
    }

    /// Runs a query and returns its `data`. GitHub answers heavy queries with 502/503/504
    /// now and then: those are retried after a pause.
    fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({"query": query, "variables": variables});
        let mut attempt = 0;
        let mut response = loop {
            attempt += 1;
            match self.agent.post(API)
                .header("Authorization", &format!("bearer {}", self.token))
                .header("User-Agent", "lgtm-md")
                .send_json(&body)
            {
                Ok(response) => break response,
                Err(ureq::Error::StatusCode(code)) if (502..=504).contains(&code) && attempt < RETRIES => {
                    std::thread::sleep(std::time::Duration::from_secs(3 * attempt as u64));
                }
                // A dropped connection is retried the same way.
                Err(ureq::Error::Io(_)) if attempt < RETRIES => {
                    std::thread::sleep(std::time::Duration::from_secs(3 * attempt as u64));
                }
                Err(error) => return Err(format!("GitHub API: {error}")),
            }
        };
        let mut reply: Value = response.body_mut().read_json().map_err(|e| format!("GitHub API reply: {e}"))?;
        // A partial reply (data plus errors) would read as zeros: fail instead.
        let has_errors = reply.get("errors").and_then(Value::as_array).is_some_and(|errors| !errors.is_empty());
        match reply.get_mut("data") {
            Some(data) if !data.is_null() && !has_errors => Ok(data.take()),
            _ => Err(format!("GitHub API error: {}", reply.get("errors").unwrap_or(&reply))),
        }
    }

    fn download(&self, url: &str, path: &Path) -> Result<()> {
        let mut response = self.agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
        let bytes = response.body_mut().with_config().limit(10 << 20).read_to_vec().map_err(|e| format!("{url}: {e}"))?;
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Follows `pageInfo` cursors; `select` picks the connection inside `data`.
    fn paginate(&self, query: &str, variables: Value, select: impl Fn(&Value) -> &Value) -> Result<Vec<Value>> {
        let mut nodes = Vec::new();
        let mut cursor: Option<String> = None;
        // A ceiling, so a huge account (or a cursor that never ends) cannot loop for ever.
        for _ in 0..MAX_PAGES {
            let mut vars = variables.clone();
            if let Some(cursor) = &cursor {
                vars["cursor"] = json!(cursor);
            }
            let data = self.graphql(query, vars)?;
            let page = select(&data);
            nodes.extend(page["nodes"].as_array().cloned().unwrap_or_default());
            if !page["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false) {
                return Ok(nodes);
            }
            cursor = page["pageInfo"]["endCursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok(nodes)
    }
}

/// The last year of daily contributions, up to yesterday (UTC).
fn calendar(client: &Client, login: &str) -> Result<(i64, Vec<Value>)> {
    let query = r#"
    query($login: String!) {
      user(login: $login) {
        contributionsCollection {
          contributionCalendar { totalContributions weeks { contributionDays { date contributionCount } } }
        }
      }
    }"#;
    let data = client.graphql(query, json!({"login": login}))?;
    let result = &data["user"]["contributionsCollection"]["contributionCalendar"];
    let yesterday = iso(today() - 1);
    let days = result["weeks"]
        .as_array()
        .ok_or(format!("no calendar for {login}"))?
        .iter()
        .flat_map(|week| week["contributionDays"].as_array().cloned().unwrap_or_default())
        .filter(|day| day["date"].as_str().unwrap_or("") <= yesterday.as_str())
        .collect();
    Ok((result["totalContributions"].as_i64().unwrap_or(0), days))
}

/// Consecutive days with at least one contribution, ending yesterday.
fn streak(days: &[Value]) -> i64 {
    days.iter().rev().take_while(|day| day["contributionCount"].as_i64().unwrap_or(0) != 0).count() as i64
}

/// PRs merged between two dates, both included, with the main language of their repository.
/// `scope` is a search qualifier: `author:<login>` for an account, `repo:<owner/name>` for a repository.
fn merged_prs(client: &Client, scope: &str, first: &str, last: &str) -> Result<Vec<Value>> {
    let query = r#"
    query($q: String!, $cursor: String) {
      search(query: $q, type: ISSUE, first: 100, after: $cursor) {
        issueCount
        pageInfo { hasNextPage endCursor }
        nodes { ... on PullRequest { mergedAt repository { nameWithOwner primaryLanguage { name color } } } }
      }
    }"#;
    let q = format!("{scope} is:pr is:merged merged:{first}..{last}");
    let nodes = client.paginate(query, json!({"q": q}), |data| &data["search"])?;
    Ok(nodes
        .iter()
        .filter(|node| node["mergedAt"].is_string())
        .map(|node| {
            let language = &node["repository"]["primaryLanguage"];
            let (name, color) = if language.is_null() {
                (Value::Null, "#cccccc".to_string())
            } else {
                (language["name"].clone(), language["color"].as_str().unwrap_or("#cccccc").to_string())
            };
            json!({"merged_at": node["mergedAt"], "repo": node["repository"]["nameWithOwner"],
                   "language": name, "color": color})
        })
        .collect())
}

/// Bytes per language across the account's own repositories, private ones included.
fn languages(repos: &[Value]) -> Vec<Value> {
    let mut totals: IndexMap<String, i64> = IndexMap::new();
    let mut colors: IndexMap<String, String> = IndexMap::new();
    for repo in repos {
        for edge in repo["languages"]["edges"].as_array().into_iter().flatten() {
            let name = edge["node"]["name"].as_str().unwrap_or("").to_string();
            *totals.entry(name.clone()).or_default() += edge["size"].as_i64().unwrap_or(0);
            colors.insert(name, edge["node"]["color"].as_str().unwrap_or("#cccccc").to_string());
        }
    }
    let total = totals.values().sum::<i64>();
    let total = if total == 0 { 1 } else { total };
    let mut ranked: Vec<(String, i64)> = totals.into_iter().collect();
    ranked.sort_by_key(|(_, size)| -size); // Stable, like Python's sorted().
    ranked
        .into_iter()
        .map(|(name, size)| json!({"name": name, "color": colors[&name], "share": size as f64 / total as f64}))
        .collect()
}

/// Yearly contributions of the accounts this one follows, for comparison.
fn following_totals(client: &Client, login: &str) -> Result<Vec<Value>> {
    // Small pages: asking for 100 calendars at once times out on GitHub's side.
    let query = r#"
    query($login: String!, $cursor: String) {
      user(login: $login) {
        following(first: 10, after: $cursor) {
          pageInfo { hasNextPage endCursor }
          nodes { login contributionsCollection { contributionCalendar { totalContributions } } }
        }
      }
    }"#;
    let nodes = client.paginate(query, json!({"login": login}), |data| &data["user"]["following"])?;
    Ok(nodes
        .iter()
        .filter(|node| node["login"].is_string())
        .map(|node| {
            let total = node["contributionsCollection"]["contributionCalendar"]["totalContributions"].as_i64();
            json!({"login": node["login"], "total": total.unwrap_or(0)})
        })
        .collect())
}

/// The last `count` calendar months, the current one included, oldest first, as
/// (start, end) day numbers, end excluded; the last one stops before today, like the calendar.
fn month_windows(count: usize) -> Vec<(i64, i64)> {
    let now = today();
    let (mut year, mut month, _) = civil_from_days(now);
    let mut starts = Vec::new();
    for _ in 0..count {
        starts.push(days_from_civil(year, month, 1));
        (year, month) = if month > 1 { (year, month - 1) } else { (year - 1, 12) };
    }
    starts.reverse();
    (0..starts.len()).map(|i| (starts[i], if i + 1 < starts.len() { starts[i + 1] } else { now })).collect()
}

/// Repositories owned by the account (no forks): name, main language and language sizes.
fn owned_repos(client: &Client, login: &str) -> Result<Vec<Value>> {
    let query = r#"
    query($login: String!, $cursor: String) {
      user(login: $login) {
        repositories(ownerAffiliations: OWNER, isFork: false, first: 50, after: $cursor) {
          pageInfo { hasNextPage endCursor }
          nodes { name primaryLanguage { name color } languages(first: 20) { edges { size node { name color } } } }
        }
      }
    }"#;
    client.paginate(query, json!({"login": login}), |data| &data["user"]["repositories"])
}

/// Commits per window and per language.
///
/// GitHub's contribution API hides which private repository a commit belongs to,
/// so owned repositories are counted from their own history (default branch),
/// and repositories owned by others come from the public contribution list.
fn commits_by_month(client: &Client, login: &str, repos: &[Value], windows: &[(i64, i64)]) -> Result<Vec<Value>> {
    let viewer = client.graphql("query($login: String!) { user(login: $login) { id } }", json!({"login": login}))?;
    let viewer_id = viewer["user"]["id"].as_str().unwrap_or("").to_string();
    let mut months: Vec<IndexMap<String, (i64, String)>> = vec![IndexMap::new(); windows.len()];
    let add = |months: &mut Vec<IndexMap<String, (i64, String)>>, index: usize, language: &Value, count: i64| {
        if count == 0 {
            return;
        }
        let name = if language.is_null() { "Other".to_string() } else { language["name"].as_str().unwrap_or("").to_string() };
        let color = language["color"].as_str().unwrap_or("#8b949e").to_string();
        months[index].entry(name).or_insert((0, color)).0 += count;
    };

    for chunk in repos.chunks(4) {
        let parts: Vec<String> = chunk
            .iter()
            .enumerate()
            .map(|(r, repo)| {
                let fields: Vec<String> = windows
                    .iter()
                    .enumerate()
                    .map(|(m, (start, end))| {
                        format!(
                            r#"m{m}: history(author: {{id: "{viewer_id}"}}, since: "{}T00:00:00Z", until: "{}T00:00:00Z") {{ totalCount }}"#,
                            iso(*start),
                            iso(*end)
                        )
                    })
                    .collect();
                format!(
                    r#"r{r}: repository(owner: "{login}", name: "{}") {{ defaultBranchRef {{ target {{ ... on Commit {{ {} }} }} }} }}"#,
                    repo["name"].as_str().unwrap_or(""),
                    fields.join(" ")
                )
            })
            .collect();
        let result = client.graphql(&format!("query {{ {} }}", parts.join(" ")), json!({}))?;
        for (r, repo) in chunk.iter().enumerate() {
            let branch = &result[format!("r{r}")]["defaultBranchRef"];
            if branch.is_null() {
                continue;
            }
            for m in 0..windows.len() {
                let count = branch["target"][format!("m{m}")]["totalCount"].as_i64().unwrap_or(0);
                add(&mut months, m, &repo["primaryLanguage"], count);
            }
        }
    }

    let others = r#"
    query($login: String!, $from: DateTime!, $to: DateTime!) {
      user(login: $login) {
        contributionsCollection(from: $from, to: $to) {
          commitContributionsByRepository(maxRepositories: 100) {
            repository { owner { login } primaryLanguage { name color } }
            contributions { totalCount }
          }
        }
      }
    }"#;
    for (m, (start, end)) in windows.iter().enumerate() {
        let vars = json!({"login": login, "from": format!("{}T00:00:00Z", iso(*start)), "to": format!("{}T00:00:00Z", iso(*end))});
        let result = client.graphql(others, vars)?;
        for item in result["user"]["contributionsCollection"]["commitContributionsByRepository"].as_array().into_iter().flatten() {
            let owner = item["repository"]["owner"]["login"].as_str().unwrap_or("");
            if owner.to_lowercase() != login.to_lowercase() {
                let count = item["contributions"]["totalCount"].as_i64().unwrap_or(0);
                add(&mut months, m, &item["repository"]["primaryLanguage"], count);
            }
        }
    }
    Ok(months
        .into_iter()
        .zip(windows)
        .map(|(languages, (start, _))| {
            let mut ranked: Vec<(String, (i64, String))> = languages.into_iter().collect();
            ranked.sort_by_key(|(_, (count, _))| -count);
            let languages: Vec<Value> =
                ranked.into_iter().map(|(name, (count, color))| json!({"name": name, "count": count, "color": color})).collect();
            json!({"month": &iso(*start)[..7], "languages": languages})
        })
        .collect())
}

/// Downloads the profile picture next to data.json; returns its path relative to it.
fn avatar(client: &Client, login: &str, base: &Path) -> Result<String> {
    let dir = base.join("avatars");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let relative = format!("avatars/{}.png", login.to_lowercase());
    let data = client.graphql("query($login: String!) { user(login: $login) { avatarUrl(size: 96) } }", json!({"login": login}))?;
    let url = data["user"]["avatarUrl"].as_str().ok_or(format!("no avatar for {login}"))?;
    client.download(url, &base.join(&relative))?;
    Ok(relative)
}

fn account(client: &Client, login: &str, full: bool, base: &Path) -> Result<Map<String, Value>> {
    let (total, year_days) = calendar(client, login)?;
    // Same key order as collect.py.
    let mut data = Map::new();
    data.insert("login".into(), json!(login));
    data.insert("total_contributions".into(), json!(total));
    data.insert("streak".into(), json!(streak(&year_days)));
    data.insert("year_days".into(), json!(year_days));
    data.insert("days".into(), json!(year_days[year_days.len().saturating_sub(DAYS)..]));
    data.insert("avatar".into(), json!(avatar(client, login, base)?));
    if full {
        let now = today();
        let prs = merged_prs(client, &format!("author:{login}"), &iso(now - 30), &iso(now - 1))?;
        let count = prs.len();
        data.insert("merged_prs".into(), json!(prs));
        data.insert("merged_prs_30d".into(), json!(count));
        let repos = owned_repos(client, login)?;
        data.insert("languages".into(), json!(languages(&repos)));
        data.insert("following".into(), json!(following_totals(client, login)?));
        // The 7 days ending yesterday ride along as a 13th window: one pass over the history.
        let mut windows = month_windows(12);
        windows.push((now - 7, now));
        let mut months = commits_by_month(client, login, &repos, &windows)?;
        let week = months.pop().expect("13 windows");
        data.insert("commits_by_month".into(), json!(months));
        data.insert("commits_7d".into(), week["languages"].clone());
    }
    Ok(data)
}

pub fn run(args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) == Some("--repo") {
        return run_project(&args[1..]);
    }
    let login = args.first().ok_or("usage: lgtm-md collect <login> [out.json] [friend...]")?;
    for name in args.iter().take(1).chain(args.iter().skip(2)) {
        if !crate::data::valid_login(name) {
            return Err(format!("not a GitHub login: {name:?}"));
        }
    }
    let out = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("data.json"));
    let base = out.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    let client = Client::new()?;
    let mut data = account(&client, login, true, &base)?;
    // Optional accounts to duel against, public data only.
    let mut friends = Map::new();
    for friend in args.iter().skip(2) {
        friends.insert(friend.clone(), Value::Object(account(&client, friend, false, &base)?));
    }
    data.insert("friends".into(), Value::Object(friends));
    let text = serde_json::to_string_pretty(&Value::Object(data)).map_err(|e| e.to_string())?;
    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("{}", out.display());
    Ok(())
}


// --- Project mode: a repository's activity, in the same data.json shape. ---

/// Days asked per query when counting a repository's commits day by day.
const DAYS_PER_QUERY: usize = 46;

/// The repository's commits per day on its default branch, all authors, from `first` to
/// yesterday (UTC). Each day is its own `history(since, until) { totalCount }`, so the count is
/// exact whatever the repository's size, at a fixed cost of a few queries.
fn repo_days(client: &Client, owner: &str, name: &str, first: i64, now: i64) -> Result<Vec<Value>> {
    let all: Vec<i64> = (first..now).collect();
    let mut days = Vec::new();
    for chunk in all.chunks(DAYS_PER_QUERY) {
        days.extend(count_days(client, owner, name, chunk)?);
    }
    Ok(days)
}

/// Commits on each of these days. A very large repository can time out on a full batch:
/// the batch is then split in two and each half asked again, down to one day.
fn count_days(client: &Client, owner: &str, name: &str, chunk: &[i64]) -> Result<Vec<Value>> {
    let mut days = Vec::new();
    {
        let fields: Vec<String> = chunk
            .iter()
            .map(|day| {
                format!(
                    // Both ends inside the day: a commit at midnight is never counted twice.
                    r#"d{day}: history(since: "{0}T00:00:00Z", until: "{0}T23:59:59Z") {{ totalCount }}"#,
                    iso(*day)
                )
            })
            .collect();
        let query = format!(
            r#"query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ defaultBranchRef {{ target {{ ... on Commit {{ {} }} }} }} }} }}"#,
            fields.join(" ")
        );
        let data = match client.graphql(&query, json!({"owner": owner, "name": name})) {
            Ok(data) => data,
            Err(error) if chunk.len() > 1 && error.contains("http status: 50") => {
                let (left, right) = chunk.split_at(chunk.len() / 2);
                let mut days = count_days(client, owner, name, left)?;
                days.extend(count_days(client, owner, name, right)?);
                return Ok(days);
            }
            Err(error) => return Err(error),
        };
        // An empty repository has no default branch: every day is zero.
        let target = &data["repository"]["defaultBranchRef"]["target"];
        for day in chunk {
            let count = target[format!("d{day}")]["totalCount"].as_i64().unwrap_or(0);
            days.push(json!({"date": iso(*day), "contributionCount": count}));
        }
    }
    Ok(days)
}

/// How many PRs were merged between two dates, both included, past search's 1000-result cap.
fn merged_pr_count(client: &Client, scope: &str, first: &str, last: &str) -> Result<i64> {
    let query = "query($q: String!) { search(query: $q, type: ISSUE, first: 1) { issueCount } }";
    let q = format!("{scope} is:pr is:merged merged:{first}..{last}");
    let data = client.graphql(query, json!({"q": q}))?;
    Ok(data["search"]["issueCount"].as_i64().unwrap_or(0))
}

/// Sum of the daily counts inside each window (start included, end excluded), credited to the
/// repository's main language: a commit is not split by the languages of the files it touches,
/// exactly as account mode credits a commit to its repository's main language.
fn windows_by_language(days: &[Value], windows: &[(i64, i64)], language: &Value) -> Vec<Value> {
    let (name, color) = if language.is_null() {
        ("Other".to_string(), "#8b949e".to_string())
    } else {
        (language["name"].as_str().unwrap_or("Other").to_string(), language["color"].as_str().unwrap_or("#8b949e").to_string())
    };
    windows
        .iter()
        .map(|(start, end)| {
            let count: i64 = days
                .iter()
                .filter(|day| {
                    let number = crate::dates::day_number(day["date"].as_str().unwrap_or(""));
                    number >= *start && number < *end
                })
                .map(|day| day["contributionCount"].as_i64().unwrap_or(0))
                .sum();
            let languages = if count == 0 { vec![] } else { vec![json!({"name": name, "count": count, "color": color})] };
            json!({"month": &iso(*start)[..7], "languages": languages})
        })
        .collect()
}

fn project(client: &Client, repo: &str) -> Result<Map<String, Value>> {
    let (owner, name) = repo.split_once('/').expect("validated");
    let query = r#"
    query($owner: String!, $name: String!) {
      repository(owner: $owner, name: $name) {
        nameWithOwner stargazerCount
        primaryLanguage { name color }
        languages(first: 20, orderBy: {field: SIZE, direction: DESC}) { edges { size node { name color } } }
      }
    }"#;
    let info = client.graphql(query, json!({"owner": owner, "name": name})).map_err(|e| {
        if e.contains("Could not resolve") {
            format!("{e}\n{repo} not found: check the name; a private repository needs a token that can read it")
        } else {
            e
        }
    })?;
    let info = &info["repository"];
    let full_name = info["nameWithOwner"].as_str().unwrap_or(repo).to_string();

    // Same span as GitHub's contribution calendar: from the Sunday a year ago to yesterday.
    let now = today();
    let year_ago = now - 365;
    let first = year_ago - (year_ago + 4).rem_euclid(7); // 1970-01-01 was a Thursday.
    let year_days = repo_days(client, owner, name, first, now)?;
    // "Last 12 months" on the widgets: the 365 days ending yesterday, not the calendar's extra days.
    let total: i64 = year_days[year_days.len().saturating_sub(365)..]
        .iter()
        .map(|day| day["contributionCount"].as_i64().unwrap_or(0))
        .sum();
    // Search lists 1000 results at most: counts come from the search total instead, and the
    // list itself only covers the 7 days the fireworks show, so it stays complete far longer.
    let scope = format!("repo:{full_name}");
    let pr_count = merged_pr_count(client, &scope, &iso(now - 30), &iso(now - 1))?;
    let week_count = merged_pr_count(client, &scope, &iso(now - 7), &iso(now - 1))?;
    let prs = merged_prs(client, &scope, &iso(now - 7), &iso(now - 1))?;
    let mut windows = month_windows(12);
    windows.push((now - 7, now));
    let mut months = windows_by_language(&year_days, &windows, &info["primaryLanguage"]);
    let week = months.pop().expect("13 windows");

    let mut data = Map::new();
    data.insert("login".into(), json!(full_name));
    data.insert("repo".into(), json!(full_name));
    data.insert("total_contributions".into(), json!(total));
    data.insert("streak".into(), json!(streak(&year_days)));
    data.insert("days".into(), json!(year_days[year_days.len().saturating_sub(DAYS)..]));
    data.insert("year_days".into(), json!(year_days));
    data.insert("merged_prs_30d".into(), json!(pr_count.max(prs.len() as i64)));
    data.insert("merged_prs_7d".into(), json!(week_count.max(prs.len() as i64)));
    data.insert("merged_prs".into(), json!(prs));
    data.insert("languages".into(), json!(languages(std::slice::from_ref(info))));
    data.insert("commits_by_month".into(), json!(months));
    data.insert("commits_7d".into(), week["languages"].clone());
    data.insert("stars".into(), json!(info["stargazerCount"].as_i64().unwrap_or(0)));
    data.insert("friends".into(), json!({}));
    Ok(data)
}

fn run_project(args: &[String]) -> Result<()> {
    let repo = args.first().ok_or("usage: lgtm-md collect --repo <owner/name> [out.json]")?;
    if !crate::data::valid_repo(repo) {
        return Err(format!("not a repository written owner/name: {repo:?}"));
    }
    if args.len() > 2 {
        return Err("duels compare accounts: no rivals in project mode".into());
    }
    let out = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("data.json"));
    let client = Client::new()?;
    let data = project(&client, repo)?;
    let text = serde_json::to_string_pretty(&Value::Object(data)).map_err(|e| e.to_string())?;
    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("{}", out.display());
    Ok(())
}
