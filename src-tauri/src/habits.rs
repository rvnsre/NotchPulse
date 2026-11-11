use std::{collections::BTreeMap, fs, path::Path};

/// Scan names of manifests in an explicitly selected folder and its immediate
/// children. No recursive crawling, source reads, shell history, or symlinks.
pub fn languages(root: &str) -> Result<Vec<String>, String> {
    let root = Path::new(root);
    if !root.is_absolute() || !root.is_dir() {
        return Err("Choose an existing absolute project directory".into());
    }
    if fs::symlink_metadata(root).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("Select a directory, not a symbolic link".into());
    }
    let mut counts = BTreeMap::<String, u32>::new();
    inspect(root, &mut counts);
    for entry in fs::read_dir(root).map_err(|e| e.to_string())?.take(128).flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()) {
            inspect(&entry.path(), &mut counts);
        }
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    Ok(counts.into_iter().take(3).map(|(l, _)| l).collect())
}

fn inspect(path: &Path, counts: &mut BTreeMap<String, u32>) {
    for (file, lang) in [("Cargo.toml","Rust"),("go.mod","Go"),("pyproject.toml","Python"),
        ("package.json","TypeScript"),("pom.xml","Java"),("CMakeLists.txt","C++"),("Package.swift","Swift")] {
        if fs::symlink_metadata(path.join(file)).is_ok_and(|m| m.file_type().is_file()) {
            *counts.entry(lang.into()).or_default() += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scans_manifest_names_at_one_level_only() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir_all(dir.join("project/deeper")).unwrap();
        fs::write(dir.join("Cargo.toml"), b"contents are never parsed").unwrap();
        fs::write(dir.join("project/Cargo.toml"), b"not source").unwrap();
        fs::write(dir.join("project/deeper/go.mod"), b"module hidden").unwrap();
        assert_eq!(languages(dir.to_str().unwrap()).unwrap(), ["Rust"]);
    }
    #[test]
    fn refuses_relative_roots() { assert!(languages("relative-projects").is_err()); }
}

