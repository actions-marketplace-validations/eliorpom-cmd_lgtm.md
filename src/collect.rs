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
/// Pages read per list at most (100 repositories or 10 followed accounts per page).
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

    /// Runs a query and returns its `data`, like `graphql()` in collect.py.
    fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({"query": query, "variables": variables});
        let mut response = self.agent.post(API)
            .header("Authorization", &format!("bearer {}", self.token))
            .header("User-Agent", "lgtm-md")
            .send_json(&body)
            .map_err(|e| format!("GitHub API: {e}"))?;
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

/// Merged PRs since a date, with the main language of their repository.
/// PRs merged between two dates, both included.
fn merged_prs(client: &Client, login: &str, first: &str, last: &str) -> Result<Vec<Value>> {
    let query = r#"
    query($q: String!, $cursor: String) {
      search(query: $q, type: ISSUE, first: 100, after: $cursor) {
        issueCount
        pageInfo { hasNextPage endCursor }
        nodes { ... on PullRequest { mergedAt repository { nameWithOwner primaryLanguage { name color } } } }
      }
    }"#;
    let q = format!("author:{login} is:pr is:merged merged:{first}..{last}");
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
        repositories(ownerAffiliations: OWNER, isFork: false, first: 100, after: $cursor) {
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

    for chunk in repos.chunks(5) {
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
        let prs = merged_prs(client, login, &iso(now - 30), &iso(now - 1))?;
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

