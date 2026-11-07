//! One module per widget, each a port of a Python prototype.

use crate::data::Data;

pub mod cherry_tree;
pub mod counter;
pub mod departures;
pub mod donut;
pub mod duel;
pub mod ecg;
pub mod fireworks;
pub mod flame;
pub mod forest;
pub mod grass;
pub mod hourglass;
pub mod median;
pub mod records;

/// Widgets that compare accounts: refused for a repository (as are duels).
pub const ACCOUNT_ONLY: [&str; 1] = ["median"];

/// (output name, renderer). Gallery order.
pub type Renderer = fn(&Data) -> String;

pub fn catalog() -> Vec<(&'static str, Renderer)> {
    vec![
        ("flame", flame::render),
        ("counter", counter::render),
        ("hourglass", hourglass::render),
        ("ecg", ecg::render),
        ("departures", departures::render),
        ("donut", donut::render),
        ("grass", grass::render),
        ("cherry-tree", cherry_tree::render),
        ("forest", forest::render),
        ("fireworks", fireworks::render),
        ("fireworks-commits", fireworks::render_commits),
        ("records", records::render),
        ("median", median::render),
    ]
}

/// `duel-<login>` (30 days), `duel-<login>-7`, `duel-<login>-30`, `duel-<login>-365` ->
/// (login, days). The explicit `-30` reaches a login that itself ends in `-7` or `-365`.
pub fn parse_duel(name: &str) -> Option<(String, i64)> {
    let rest = name.strip_prefix("duel-")?;
    let (login, days) = [("-365", 365), ("-30", 30), ("-7", 7)]
        .iter()
        .find_map(|(suffix, days)| rest.strip_suffix(suffix).map(|login| (login, *days)))
        .unwrap_or((rest, 30));
    crate::data::valid_login(login).then(|| (login.to_string(), days))
}

/// A catalog entry, or a duel against any friend collected in data.json (see `parse_duel`).
pub fn find(catalog: &[(&'static str, Renderer)], name: &str) -> Option<Box<dyn Fn(&Data) -> String>> {
    if let Some((_, render)) = catalog.iter().find(|(key, _)| *key == name) {
        let render = *render;
        return Some(Box::new(render));
    }
    let (login, days) = parse_duel(name)?;
    let name = name.to_string();
    Some(Box::new(move |data| duel::render_as(data, &login, days, &name)))
}

#[cfg(test)]
mod tests {
    use super::parse_duel;

    #[test]
    fn duel_names() {
        let parsed = parse_duel;
        assert_eq!(parsed("duel-torvalds"), Some(("torvalds".into(), 30)));
        assert_eq!(parsed("duel-torvalds-7"), Some(("torvalds".into(), 7)));
        assert_eq!(parsed("duel-torvalds-365"), Some(("torvalds".into(), 365)));
        assert_eq!(parsed("duel-dev-7-30"), Some(("dev-7".into(), 30)));
        assert_eq!(parsed("duel-../x"), None);
        assert_eq!(parsed("flame"), None);
    }
}
