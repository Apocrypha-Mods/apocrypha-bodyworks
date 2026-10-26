//! Real saves: `scratch/samples/saves`, when it is there (it is not in the
//! repository). These check properties, never values read from the saves.

use apoc_save::{header_only, Compression, Save, MAGIC};
use std::collections::HashMap;
use std::path::PathBuf;

fn saves() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/saves");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no {}", dir.display());
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ess"))
        .collect();
    v.sort();
    v
}

/// The character id the game writes as the file name's second field.
fn character(p: &std::path::Path) -> String {
    p.file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .split('_')
        .nth(1)
        .unwrap_or("")
        .to_string()
}

#[test]
fn every_save_reads_and_its_look_agrees_with_its_header() {
    for p in saves() {
        let bytes = std::fs::read(&p).unwrap();
        let l = header_only(&bytes).unwrap();
        let s = Save::read(&bytes).unwrap();
        assert_eq!(&l.header, s.header());
        assert_eq!(s.screenshot(), l.screenshot);
        let h = s.header();
        assert_eq!(h.version, 12);
        assert_eq!(
            l.screenshot.pixels.len(),
            (h.shot_width * h.shot_height * 4) as usize
        );
        assert!(s.form_version() >= 78);
        assert!(!s.plugins().is_empty());
        for f in s.form_ids() {
            s.form_of(*f).unwrap();
        }
        let look = s.player().unwrap();
        assert_eq!(look.sex as u16, h.sex, "{}", p.display());
        assert!(look.race.is_some());
        assert!(!look.head_parts.is_empty());
        assert!(!look.tints.is_empty(), "{}", p.display());
        assert!(look.tints.iter().all(|t| (0.0..=1.0).contains(&t.strength)));
        assert!(look.morphs.iter().all(|m| m.is_finite()));
    }
}

#[test]
fn one_characters_saves_agree() {
    let mut seen: HashMap<String, (apoc_save::Form, u8, usize, usize)> = HashMap::new();
    let mut compared = 0;
    for p in saves() {
        let s = Save::read(&std::fs::read(&p).unwrap()).unwrap();
        let l = s.player().unwrap();
        let key = (
            l.race.clone().unwrap(),
            l.sex,
            l.morphs.len(),
            l.presets.len(),
        );
        if let Some(first) = seen.get(&character(&p)) {
            assert_eq!(first, &key, "{}", p.display());
            compared += 1;
        } else {
            seen.insert(character(&p), key);
        }
    }
    if !seen.is_empty() {
        assert!(compared > 0);
    }
}

/// The save rebuilt with its body stored as it is (compression type 0), so
/// that damage reaches the body's own structure rather than LZ4's.
fn uncompressed(bytes: &[u8], s: &Save) -> Vec<u8> {
    let size = u32::from_le_bytes(bytes[13..17].try_into().unwrap()) as usize;
    let header_end = MAGIC.len() + 4 + size;
    let h = s.header();
    let shot = (h.shot_width * h.shot_height) as usize * h.bytes_per_pixel();
    let mut out = bytes[..header_end + shot].to_vec();
    out[header_end - 2..header_end].copy_from_slice(&0u16.to_le_bytes());
    out.extend(s.body());
    out
}

#[test]
fn a_save_stored_uncompressed_reads_the_same() {
    for p in saves().iter().step_by(11) {
        let bytes = std::fs::read(p).unwrap();
        let s = Save::read(&bytes).unwrap();
        let u = uncompressed(&bytes, &s);
        let t = Save::read(&u).unwrap();
        assert_eq!(t.header().compression, Compression::None);
        assert_eq!(t.plugins(), s.plugins());
        assert_eq!(t.light_plugins(), s.light_plugins());
        assert_eq!(t.form_ids(), s.form_ids());
        assert_eq!(t.change_forms(), s.change_forms());
        assert_eq!(t.player().unwrap(), s.player().unwrap());
    }
}

/// A small, fixed pseudo-random sequence, so failures repeat.
fn next(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

#[test]
fn cut_saves_are_errors() {
    for p in saves().iter().step_by(29) {
        let bytes = std::fs::read(p).unwrap();
        let s = Save::read(&bytes).unwrap();
        let u = uncompressed(&bytes, &s);
        for b in [&bytes, &u] {
            // Every cut in the first few kilobytes, then evenly through.
            let cuts = (0..4096).chain((4096..b.len()).step_by(b.len() / 150 + 1));
            for n in cuts.filter(|&n| n < b.len()) {
                let r = Save::read(&b[..n]);
                if b.as_ptr() == bytes.as_ptr() {
                    // A compressed body cut anywhere never reads.
                    assert!(r.is_err(), "cut at {n}");
                } else if let Ok(s) = r {
                    // One stored as it is may, when the cut is past all it
                    // indexes; the look must still not panic.
                    let _ = s.player();
                }
            }
        }
    }
}

#[test]
fn damaged_saves_never_panic() {
    let mut x = 0x2545_F491_4F6C_DD1D;
    for p in saves().iter().step_by(17) {
        let bytes = std::fs::read(p).unwrap();
        let s = Save::read(&bytes).unwrap();
        let u = uncompressed(&bytes, &s);
        let body_at = u.len() - s.body().len();
        // The player's two change forms, where the look is read from.
        let hot: Vec<usize> = s
            .change_forms()
            .iter()
            .filter(|c| {
                s.player().is_ok()
                    && (c.kind == 1 || c.kind == 9)
                    && c.ref_id.kind() == 1
                    && (c.ref_id.value() == 7 || c.ref_id.value() == 0x14)
            })
            .map(|c| body_at + c.offset)
            .collect();
        for round in 0..150 {
            let mut b = if round % 3 == 0 {
                bytes.clone()
            } else {
                u.clone()
            };
            for _ in 0..1 + next(&mut x) % 4 {
                let at = match next(&mut x) % 4 {
                    0 => (next(&mut x) % 200) as usize,
                    1 if !hot.is_empty() && b.len() == u.len() => {
                        hot[(next(&mut x) as usize) % hot.len()] + (next(&mut x) % 4000) as usize
                    }
                    2 if b.len() == u.len() => body_at + (next(&mut x) % 6000) as usize,
                    _ => (next(&mut x) as usize) % b.len(),
                };
                if let Some(v) = b.get_mut(at) {
                    *v ^= 1 << (next(&mut x) % 8);
                }
            }
            if let Ok(s) = Save::read(&b) {
                let _ = s.player();
                for c in s.change_forms().iter().take(50) {
                    let _ = s.data(c);
                    let _ = s.resolve(c.ref_id);
                }
            }
            let _ = header_only(&b);
        }
    }
}
