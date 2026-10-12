//! A project packed as one file for sharing: a zip archive of its files,
//! each stored as it is, laid out as a mod is (`CalienteTools/BodySlide/…`)
//! so that it installs like one.

use crate::build::Output;

/// The check a zip keeps of each file: CRC-32, as every zip reader works it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    !bytes.iter().fold(!0u32, |c, b| {
        table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8)
    })
}

/// `files` as a zip archive, each under `folder`, stored and in the order
/// given. The same files always make the same archive. None when there are
/// more or larger files than this plain kind of zip can hold.
pub fn zip(folder: &str, files: &[Output]) -> Option<Vec<u8>> {
    if files.len() > u16::MAX as usize {
        return None;
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for file in files {
        let name = format!("{folder}/{}", file.path);
        let (size, at) = (
            u32::try_from(file.bytes.len()).ok()?,
            u32::try_from(out.len()).ok()?,
        );
        let name_len = u16::try_from(name.len()).ok()?;
        let crc = crc32(&file.bytes);
        // What both records say of the file: version, flags (names are
        // UTF-8), stored, no time, check, sizes, name length.
        let mut common = Vec::new();
        common.extend(20u16.to_le_bytes());
        common.extend(0x0800u16.to_le_bytes());
        common.extend(0u16.to_le_bytes());
        common.extend(0u16.to_le_bytes());
        common.extend(0x21u16.to_le_bytes());
        common.extend(crc.to_le_bytes());
        common.extend(size.to_le_bytes());
        common.extend(size.to_le_bytes());
        common.extend(name_len.to_le_bytes());
        common.extend(0u16.to_le_bytes());
        out.extend(b"PK\x03\x04");
        out.extend(&common);
        out.extend(name.as_bytes());
        out.extend(&file.bytes);
        central.extend(b"PK\x01\x02");
        central.extend(20u16.to_le_bytes());
        central.extend(&common);
        // No comment, first disk, no attributes of either kind.
        central.extend([0u8; 2 + 2 + 2 + 4]);
        central.extend(at.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let (start, length) = (
        u32::try_from(out.len()).ok()?,
        u32::try_from(central.len()).ok()?,
    );
    out.extend(&central);
    out.extend(b"PK\x05\x06");
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend(length.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_is_the_one_every_zip_reader_works() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn files_are_stored_whole_and_listed_at_the_end() {
        let files = [
            Output {
                path: "SliderSets/A.osp".into(),
                bytes: b"<x/>".to_vec(),
            },
            Output {
                path: "ShapeData/A/A.nif".into(),
                bytes: vec![7; 300],
            },
        ];
        let zip = zip("CalienteTools/BodySlide", &files).unwrap();
        assert!(zip.starts_with(b"PK\x03\x04"));
        // The end record: two files, and where the list of them starts.
        let end = &zip[zip.len() - 22..];
        assert_eq!(&end[..4], b"PK\x05\x06");
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 2);
        let start = u32::from_le_bytes([end[16], end[17], end[18], end[19]]) as usize;
        assert_eq!(&zip[start..start + 4], b"PK\x01\x02");
        // The first file's name and bytes follow its record.
        let name = b"CalienteTools/BodySlide/SliderSets/A.osp";
        assert_eq!(&zip[30..30 + name.len()], name);
        assert_eq!(&zip[30 + name.len()..30 + name.len() + 4], b"<x/>");
        assert_eq!(zip, super::zip("CalienteTools/BodySlide", &files).unwrap());
    }
}
