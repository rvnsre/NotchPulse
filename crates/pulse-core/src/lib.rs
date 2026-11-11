pub mod geometry;
pub mod recipes;
pub mod schedule;
pub mod spring;

use serde::{Deserialize, Serialize};

pub const STAR_FLOOR: u64 = 20_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Repository {
    pub id: u64,
    pub full_name: String,
    pub description: Option<String>,
    pub stargazers_count: u64,
    pub language: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub topics: Vec<String>,
}

impl Repository {
    pub fn eligible(&self) -> bool {
        self.id != 0
            && self.stargazers_count >= STAR_FLOOR
            && !self.archived
            && !self.fork
            && valid_repo(&self.full_name)
    }
    pub fn url(&self) -> String {
        format!("https://github.com/{}", self.full_name)
    }
}

pub fn valid_repo(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 100
                && !p.starts_with(['-', '.'])
                && p.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
}

pub fn curate(mut items: Vec<Repository>, languages: &[String]) -> Vec<Repository> {
    items.retain(Repository::eligible);
    // Stable, local relevance ranking; star count breaks ties.
    items.sort_by_key(|r| {
        let matched = r.language.as_ref().is_some_and(|l| languages.contains(l));
        std::cmp::Reverse((matched, r.stargazers_count))
    });
    let mut seen = std::collections::HashSet::new();
    items.retain(|r| seen.insert(r.id));
    items.truncate(12);
    items
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub paused: bool,
    pub habits_enabled: bool,
    pub project_root: String,
    pub monitor: Option<String>,
    pub camera_offset_x: f64,
    pub top_inset: f64,
    pub reduced_motion: bool,
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.camera_offset_x.is_finite() || self.camera_offset_x.abs() > 4000.0 {
            return Err("Camera offset must be between -4000 and 4000 points".into());
        }
        if !self.top_inset.is_finite() || !(0.0..=400.0).contains(&self.top_inset) {
            return Err("Top inset must be between 0 and 400 points".into());
        }
        if self.project_root.len() > 4096 || self.monitor.as_ref().is_some_and(|m| m.len() > 256) {
            return Err("Setting is too long".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub label: String,
    pub primary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub repositories: Vec<Repository>,
    pub host: recipes::Host,
    pub settings: Settings,
    pub monitors: Vec<MonitorInfo>,
    pub languages: Vec<String>,
    pub status: String,
    pub updated_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_shell_and_url_injection() {
        for s in ["a/b;curl", "a/b\n", "-x/b", "a/../b", "a/b?x", "a/b/c", "a/$(id)"] {
            assert!(!valid_repo(s), "{s}");
        }
        assert!(valid_repo("sharkdp/bat"));
    }
    #[test]
    fn filters_ranks_and_deduplicates() {
        let r = |id, name: &str, stars, lang: &str| Repository {
            id, full_name: name.into(), stargazers_count: stars,
            language: Some(lang.into()), ..Default::default()
        };
        let list = curate(vec![r(1,"a/a",19_999,"Rust"),r(2,"b/b",90_000,"Go"),
            r(3,"c/c",20_000,"Rust"),r(3,"c/c",20_000,"Rust")], &["Rust".into()]);
        assert_eq!(list.iter().map(|r| r.id).collect::<Vec<_>>(), [3,2]);
    }
    #[test]
    fn validates_non_finite_settings() {
        assert!(Settings { top_inset: f64::NAN, ..Default::default() }.validate().is_err());
    }
}

