//! Game files by path, as a build reads them.
//!
//! Records name models and tris under `meshes` and textures under
//! `textures`, without that folder and with `\`. A path here is under the
//! Data folder, in lower case with `/`: `meshes/actors/character/character
//! assets/malehead.nif`. Windows compares paths without case and takes
//! either slash, and so do these.

use std::collections::HashMap;

/// A path as this compares them: lower case, `/` between parts, no empty
/// parts, no leading `data/`.
pub fn key(path: &str) -> String {
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    let mut out = parts.join("/").to_ascii_lowercase();
    if let Some(rest) = out.strip_prefix("data/") {
        out = rest.to_string();
    }
    out
}

/// A path a record gives under `folder` (`meshes` or `textures`), as a
/// [`key`]. A record that already starts with the folder keeps it.
pub fn under(folder: &str, path: &str) -> String {
    let k = key(path);
    if k.starts_with(&format!("{folder}/")) {
        k
    } else {
        format!("{folder}/{k}")
    }
}

/// Where a build reads files from, by [`key`].
pub trait Source {
    fn read(&self, key: &str) -> Option<&[u8]>;
}

/// Files held in memory, found by path without case or slash mattering.
#[derive(Debug, Clone, Default)]
pub struct Files {
    files: HashMap<String, Vec<u8>>,
}

impl Files {
    pub fn new() -> Self {
        Files::default()
    }

    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        self.files.insert(key(path), bytes);
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl Source for Files {
    fn read(&self, path: &str) -> Option<&[u8]> {
        self.files.get(&key(path)).map(Vec::as_slice)
    }
}

impl Source for HashMap<String, Vec<u8>> {
    fn read(&self, path: &str) -> Option<&[u8]> {
        self.get(path)
            .or_else(|| self.get(&key(path)))
            .map(Vec::as_slice)
    }
}

/// Files the caller keeps, borrowed by path: the add-on's buffers fetched
/// over IPC.
impl Source for HashMap<String, &[u8]> {
    fn read(&self, path: &str) -> Option<&[u8]> {
        self.get(path).or_else(|| self.get(&key(path))).copied()
    }
}

/// The model the game loads at a weight from a model path a record gives.
/// Found from the files: Skyrim.esm's skin addons name `MaleBody_1.NIF`,
/// and the archives hold `malebody_0.nif` and `malebody_1.nif`, the
/// lightest and heaviest body. A path that ends in neither is the one file.
pub fn weighted(path: &str) -> Option<(String, String)> {
    let k = key(path);
    let stem = k.strip_suffix(".nif")?;
    let base = stem
        .strip_suffix("_1")
        .or_else(|| stem.strip_suffix("_0"))?;
    Some((format!("{base}_0.nif"), format!("{base}_1.nif")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_case_and_slashes() {
        assert_eq!(key("Actors\\Character\\X.NIF"), "actors/character/x.nif");
        assert_eq!(key("/Data//meshes\\a/b"), "meshes/a/b");
        assert_eq!(under("meshes", "Actors\\A.nif"), "meshes/actors/a.nif");
        assert_eq!(
            under("meshes", "Meshes\\Actors\\A.nif"),
            "meshes/actors/a.nif"
        );
    }

    #[test]
    fn files_found_either_way() {
        let mut f = Files::new();
        f.insert("Meshes\\A\\B.nif", vec![1]);
        assert_eq!(f.read("meshes/a/b.nif"), Some(&[1u8][..]));
        assert_eq!(f.read("MESHES/A\\B.NIF"), Some(&[1u8][..]));
        assert_eq!(f.read("meshes/a/c.nif"), None);
    }

    #[test]
    fn weighted_pairs() {
        assert_eq!(
            weighted("Actors\\Character\\Character Assets\\MaleBody_1.NIF"),
            Some((
                "actors/character/character assets/malebody_0.nif".into(),
                "actors/character/character assets/malebody_1.nif".into()
            ))
        );
        assert_eq!(weighted("a/tail.nif"), None);
        assert_eq!(weighted("a/b_0.nif").unwrap().1, "a/b_1.nif");
    }
}
