//! `data.json` model, as written by `collect.py`.
//!
//! Fields read in Python with `data.get(key)` (no default) are `Option`s so a port can
//! tell "missing" from "empty"; the others default to empty or zero.

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};

/// A JSON `null` reads as the default value (0, ""), like a missing field.
fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(deserializer: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Day {
    #[serde(default, deserialize_with = "nullable")]
    pub date: String,
    #[serde(rename = "contributionCount")]
    #[serde(default, deserialize_with = "nullable")]
    pub count: i64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct MergedPr {
    #[serde(default, deserialize_with = "nullable")]
    pub merged_at: String,
    /// Kept in data.json for readers; no widget draws it.
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "nullable")]
    pub repo: String,
    pub language: Option<String>,
    pub color: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Language {
    #[serde(default, deserialize_with = "nullable")]
    pub name: String,
    pub color: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub share: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Followed {
    /// Kept in data.json for readers; only the totals are drawn.
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "nullable")]
    pub login: String,
    #[serde(default, deserialize_with = "nullable")]
    pub total: i64,
}

/// Commits of one language: `{name, count, color}`.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct LanguageCount {
    #[serde(default, deserialize_with = "nullable")]
    pub name: String,
    #[serde(default, deserialize_with = "nullable")]
    pub count: i64,
    pub color: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Month {
    /// "YYYY-MM". Months come in order, so widgets go by position.
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "nullable")]
    pub month: String,
    pub languages: Vec<LanguageCount>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Data {
    #[serde(default, deserialize_with = "nullable")]
    pub login: String,
    #[serde(default, deserialize_with = "nullable")]
    pub total_contributions: i64,
    #[serde(default, deserialize_with = "nullable")]
    pub streak: i64,
    pub year_days: Option<Vec<Day>>,
    /// Last 90 days, oldest first, ending yesterday.
    pub days: Vec<Day>,
    pub merged_prs: Vec<MergedPr>,
    #[serde(default, deserialize_with = "nullable")]
    pub merged_prs_30d: i64,
    pub languages: Vec<Language>,
    pub following: Option<Vec<Followed>>,
    /// Compared accounts, in file order. Same fields, without PRs or languages.
    pub friends: IndexMap<String, Data>,
    pub commits_by_month: Option<Vec<Month>>,
    pub commits_7d: Vec<LanguageCount>,
    /// Path of the avatar image, relative to data.json (see `resolve`).
    pub avatar: Option<String>,
    /// Project mode: "owner/name" of the repository drawn instead of an account.
    pub repo: Option<String>,
    /// Project mode: PRs merged in the 7 days ending yesterday, exact even past the 1000 PRs
    /// that `merged_prs` can list.
    pub merged_prs_7d: Option<i64>,
    /// Project mode: the repository's stars.
    #[serde(default, deserialize_with = "nullable")]
    pub stars: i64,
}

/// A GitHub login: 1 to 39 letters, digits or single hyphens, not starting or ending with one.
/// Logins end up in GraphQL queries and file names, so anything else is refused.
pub fn valid_login(login: &str) -> bool {
    let bytes = login.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 39
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
        && !login.contains("--")
}

/// A repository written `owner/name`: a valid login, a slash, then 1 to 100 letters, digits,
/// `-`, `_` or `.` (not `.` or `..`). It ends up in queries and search strings, like a login.
pub fn valid_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else { return false };
    valid_login(owner)
        && !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Folder of the loaded data.json: files it names (avatars) are relative to it.
static BASE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// A path written in data.json: next to data.json first, else from the working directory.
pub fn resolve(relative: &str) -> std::path::PathBuf {
    if let Some(found) = BASE.get().map(|base| base.join(relative)).filter(|path| path.exists()) {
        return found;
    }
    std::path::PathBuf::from(relative)
}

impl Data {
    pub fn load(path: &std::path::Path) -> Result<Data, String> {
        if let Some(parent) = path.parent() {
            let _ = BASE.set(parent.to_path_buf());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn year_days(&self) -> &[Day] {
        self.year_days.as_deref().unwrap_or(&[])
    }

    /// True when data.json describes a repository rather than an account.
    pub fn is_project(&self) -> bool {
        self.repo.is_some()
    }

    /// What one unit of activity is called on the widgets: contributions for an account,
    /// commits for a repository (its default branch).
    pub fn unit(&self, count: i64) -> &'static str {
        match (self.is_project(), count == 1) {
            (false, true) => "CONTRIBUTION",
            (false, false) => "CONTRIBUTIONS",
            (true, true) => "COMMIT",
            (true, false) => "COMMITS",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_login;

    #[test]
    fn nulls_read_as_defaults() {
        let data: super::Data = serde_json::from_str(
            r#"{"login": null, "following": [{"login": "a", "total": null}], "merged_prs": [{"merged_at": null, "repo": "x"}]}"#,
        )
        .unwrap();
        assert_eq!(data.login, "");
        assert_eq!(data.following.unwrap()[0].total, 0);
        assert_eq!(data.merged_prs[0].merged_at, "");
    }

    #[test]
    fn repos() {
        for ok in ["eliorpom-cmd/lgtm.md", "BurntSushi/ripgrep", "a/b_c-d.e", "a/.github"] {
            assert!(super::valid_repo(ok), "{ok}");
        }
        for bad in ["", "a", "a/", "/b", "a/b/c", "a/..", "a/.", "-a/b", "a/b c", "a/b\"", "a/é"] {
            assert!(!super::valid_repo(bad), "{bad}");
        }
    }

    #[test]
    fn logins() {
        for ok in ["torvalds", "a", "4ever", "octo-cat", "A-b-C"] {
            assert!(valid_login(ok), "{ok}");
        }
        for bad in ["", "-a", "a-", "a--b", "../x", "a/b", "a b", "a\"b", "é", &"a".repeat(40)] {
            assert!(!valid_login(bad), "{bad}");
        }
    }
}
