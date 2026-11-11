use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform { Windows, MacOS, Linux }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Host {
    pub platform: Platform,
    pub arch: String,
    pub shell: String,
    pub package_manager: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub display: String,
    pub note: String,
}

// Never infer executable code from README text, topics, descriptions or API fields.
// Only these reviewed upstream identities have installation recipes.
pub fn recipe(repo: &str, host: &Host) -> Result<Recipe, String> {
    if !crate::valid_repo(repo) { return Err("Invalid repository identity".into()); }
    let known = match repo {
        "BurntSushi/ripgrep" => Some(("BurntSushi.ripgrep.MSVC", "ripgrep", "ripgrep", "ripgrep")),
        "sharkdp/bat" => Some(("sharkdp.bat", "bat", "bat", "bat")),
        "sharkdp/fd" => Some(("sharkdp.fd", "fd", "fd-find", "fd-find")),
        "starship/starship" => Some(("Starship.Starship", "starship", "", "starship")),
        "helix-editor/helix" => Some(("Helix.Helix", "helix", "", "")),
        "nushell/nushell" => Some(("Nushell.Nushell", "nushell", "", "nu")),
        _ => None,
    };
    let mut note = "Runs in a visible terminal. Review before executing.".to_string();
    let (program, args) = if let Some((winget, brew, deb, cargo)) = known {
        match (&host.platform, host.package_manager.as_str()) {
            (Platform::Windows, "winget") => ("winget", vec!["install".into(), "--exact".into(), "--id".into(), winget.into()]),
            (Platform::MacOS, "brew") => ("brew", vec!["install".into(), brew.into()]),
            (Platform::Linux, "apt") if !deb.is_empty() => ("sudo", vec!["apt".into(), "install".into(), deb.into()]),
            (Platform::Linux, "pacman") => ("sudo", vec!["pacman".into(), "-S".into(), brew.into()]),
            (Platform::Linux, "dnf") if !deb.is_empty() => ("sudo", vec!["dnf".into(), "install".into(), deb.into()]),
            (_, "cargo") if !cargo.is_empty() => ("cargo", vec!["install".into(), "--locked".into(), cargo.into()]),
            _ => ("git", vec!["clone".into(), "--".into(), format!("https://github.com/{repo}.git")]),
        }
    } else {
        ("git", vec!["clone".into(), "--".into(), format!("https://github.com/{repo}.git")])
    };
    if program == "git" {
        note = "No reviewed install recipe. Clones source into your home directory; Git is required.".into();
    }
    if host.platform == Platform::MacOS && host.arch == "aarch64" {
        note.push_str(" Uses the native Apple Silicon environment; no Rosetta wrapper.");
    }
    let display = std::iter::once(program.to_string()).chain(args.iter().cloned())
        .map(|s| quote(&s, &host.platform)).collect::<Vec<_>>().join(" ");
    Ok(Recipe { label: if program == "git" { "Clone source" } else { "Install" }.into(), program: program.into(), args, display, note })
}

pub fn quote(value: &str, platform: &Platform) -> String {
    if !value.is_empty() && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.:/".contains(&c)) {
        return value.to_owned();
    }
    match platform {
        Platform::Windows => format!("'{}'", value.replace('\'', "''")),
        _ => format!("'{}'", value.replace('\'', "'\"'\"'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host(platform: Platform, pm: &str) -> Host { Host { platform, arch: "aarch64".into(), shell: "zsh".into(), package_manager: pm.into() } }
    #[test]
    fn uses_exact_windows_package_and_linux_alias() {
        assert!(recipe("sharkdp/bat", &host(Platform::Windows,"winget")).unwrap().display.contains("--exact --id sharkdp.bat"));
        assert_eq!(recipe("sharkdp/fd", &host(Platform::Linux,"apt")).unwrap().args, ["apt","install","fd-find"]);
    }
    #[test]
    fn unknown_repositories_only_clone() {
        assert_eq!(recipe("a/b", &host(Platform::MacOS,"brew")).unwrap().program,"git");
    }
    #[test]
    fn quotes_shell_metacharacters() {
        assert_eq!(quote("$(id)", &Platform::Linux), "'$(id)'");
        assert_eq!(quote("a'b", &Platform::Windows), "'a''b'");
        assert_eq!(quote("", &Platform::Windows), "''");
        assert_eq!(quote("", &Platform::Linux), "''");
    }
}
