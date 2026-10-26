//! Driving a [`Walk`] from a file on disk, for tools and tests (the add-on
//! feeds it over IPC instead).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::{Plugin, PluginError, Stats, Walk};

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Plugin(#[from] PluginError),
}

/// Reads the groups of `wanted` from the plugin at `path`, by ranges.
pub fn read_file(path: &Path, wanted: &[[u8; 4]]) -> Result<(Plugin, Stats), FileError> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut walk = Walk::new(&name, len, wanted);
    let mut buf = Vec::new();
    while let Some((at, n)) = walk.need() {
        buf.resize(n, 0);
        file.seek(SeekFrom::Start(at))?;
        file.read_exact(&mut buf)?;
        walk.feed(at, &buf)?;
    }
    let stats = walk.stats();
    Ok((walk.finish()?, stats))
}
