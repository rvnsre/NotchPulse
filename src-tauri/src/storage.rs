use serde::{de::DeserializeOwned, Serialize};
use std::{fs, io::{Read, Write}, path::Path};

const MAX_BYTES: u64 = 1_048_576;

pub fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES { return None; }
    // Bound the actual read: a file may grow after metadata() returns.
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES { return None; }
    serde_json::from_slice(&bytes).ok()
}

pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES { return Err("Data exceeds the 1 MiB persistence limit".into()); }
    let parent = path.parent().ok_or("Storage path has no parent")?;
    // Exclusively created, unique, same-volume files avoid shared .tmp races.
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(&bytes).and_then(|_| file.as_file().sync_all()).map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    fs::File::open(parent).and_then(|dir| dir.sync_all()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaces_preferences_and_rejects_invalid_or_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write(&path, &vec![1, 2]).unwrap();
        write(&path, &vec![3, 4]).unwrap();
        assert_eq!(read::<Vec<u8>>(&path).unwrap(), [3, 4]);
        fs::write(&path, b"not JSON").unwrap();
        assert!(read::<Vec<u8>>(&path).is_none());
        fs::File::create(&path).unwrap().set_len(MAX_BYTES + 1).unwrap();
        assert!(read::<Vec<u8>>(&path).is_none());
    }
    #[test]
    fn failed_write_preserves_previous_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write(&path, &vec![42]).unwrap();
        assert!(write(&path, &"x".repeat(MAX_BYTES as usize + 1)).is_err());
        assert_eq!(read::<Vec<u8>>(&path).unwrap(), [42]);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
