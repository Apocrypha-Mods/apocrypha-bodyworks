//! Real saves: `scratch/samples/saves`, when it is there (it is not in the
//! repository). These check properties, never values read from the saves.

use apoc_save::{header_only, look, Compression, Form, FormRef, Save, KIND_ACHR, KIND_NPC, MAGIC};
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

/// Runs `f` on every item, spread over a few threads (decompressing a body
/// takes a tenth of a second in a debug build), and gives back what each
/// gave, in order.
fn each<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(6));
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let f = &f;
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|c| s.spawn(move || c.iter().map(f).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

/// What one save is checked by, read once for every test that needs it.
struct Read {
    path: PathBuf,
    header: apoc_save::Header,
    look: apoc_save::Look,
    /// The NPC change form's length, what `read_npc` needs of it, and the
    /// outfits its flags say follow.
    npc: (usize, usize, usize),
    /// The player block, found in the placed-actor change form, and the
    /// u32 between its tints and the texture set.
    block: (apoc_save::look::PlayerBlock, u32),
}

fn me(s: &Save, id: u32) -> Form {
    Form::Plugin(FormRef {
        plugin: s.plugins()[0].clone(),
        id,
    })
}

/// The shortest prefix of an NPC change form that reads the same as all
/// of it.
fn npc_needs(d: &[u8], flags: u32) -> usize {
    let full = look::read_npc(d, flags).unwrap();
    // Each part is read whole, so every shorter prefix fails or reads less;
    // the search runs back from the end, where the answer is.
    let mut n = d.len();
    while n > 0 && look::read_npc(&d[..n - 1], flags).as_ref() == Ok(&full) {
        n -= 1;
    }
    n
}

fn read_one(p: &PathBuf) -> Read {
    let bytes = std::fs::read(p).unwrap();
    let l = header_only(&bytes).unwrap();
    let s = Save::read(&bytes).unwrap();
    assert_eq!(&l.header, s.header(), "{}", p.display());
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
    let npc = s.change_form(&me(&s, 7), KIND_NPC).unwrap();
    let d = s.data(npc).unwrap();
    let outfits = 3 * ((npc.flags >> 12) & 1) as usize + 3 * ((npc.flags >> 13) & 1) as usize;
    let needs = npc_needs(&d, npc.flags);
    let face = look::read_npc(&d, npc.flags).unwrap().face.unwrap();
    let achr = s.change_form(&me(&s, 0x14), KIND_ACHR).unwrap();
    let a = s.data(achr).unwrap();
    let block = look::find_player_block(&a, face.skin.unwrap()).unwrap();
    let end = block.offset + 9 + 8 + 10 * block.tints.len();
    let zero = u32::from_le_bytes(a[end..end + 4].try_into().unwrap());
    Read {
        path: p.clone(),
        header: h.clone(),
        look,
        npc: (d.len(), needs, outfits),
        block: (block, zero),
    }
}

/// Every save, read once.
fn read_all() -> &'static [Read] {
    static ALL: std::sync::OnceLock<Vec<Read>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| each(&saves(), read_one))
}

#[test]
fn every_save_reads_and_its_look_agrees_with_its_header() {
    for r in read_all() {
        let (h, look, p) = (&r.header, &r.look, r.path.display());
        assert_eq!(look.sex as u16, h.sex, "{p}");
        assert!(look.race.is_some());
        assert!(!look.head_parts.is_empty());
        assert!(!look.tints.is_empty(), "{p}");
        assert!(look.tints.iter().all(|t| (0.0..=1.0).contains(&t.strength)));
        assert!(look.morphs.iter().all(|m| m.is_finite()));
        assert_eq!(look.weight, None);
    }
}

/// What the module says of the face and the player block, found from these
/// files, held to.
#[test]
fn every_save_has_the_face_and_player_block_the_module_describes() {
    for r in read_all() {
        let (look, p) = (&r.look, r.path.display());
        // UESP's layout accounts for every byte of the NPC change form but
        // the outfits after the sex.
        let (len, needs, outfits) = r.npc;
        assert_eq!(needs + outfits, len, "{p}");
        assert_eq!((look.morphs.len(), look.presets.len()), (19, 4), "{p}");
        assert_eq!(look.skin_tone[3], 0, "{p}");
        // That one tint layer is painted the face's skin tone is not held
        // to: in all six saves of one character none is.
        assert!(look.tints.iter().all(|t| matches!(t.colour[3], 0 | 255)));
        let (block, zero) = &r.block;
        assert_eq!(block.races[1], block.races[2], "{p}");
        assert_eq!(block.sex, r.header.sex as u32, "{p}");
        assert_eq!(block.tints, look.tints);
        assert_eq!(*zero, 0, "{p}");
    }
}

#[test]
fn one_characters_saves_agree() {
    let mut seen: HashMap<String, (apoc_save::Form, u8, usize, usize)> = HashMap::new();
    let mut compared = 0;
    for r in read_all() {
        let l = &r.look;
        let key = (
            l.race.clone().unwrap(),
            l.sex,
            l.morphs.len(),
            l.presets.len(),
        );
        if let Some(first) = seen.get(&character(&r.path)) {
            assert_eq!(first, &key, "{}", r.path.display());
            compared += 1;
        } else {
            seen.insert(character(&r.path), key);
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
    let picked: Vec<(u64, PathBuf)> = saves()
        .into_iter()
        .step_by(17)
        .enumerate()
        .map(|(i, p)| (0x2545_F491_4F6C_DD1D ^ (i as u64 + 1), p))
        .collect();
    each(&picked, |(seed, p)| damage(*seed, p));
}

/// 150 copies of one save with a few bits flipped, read as far as they go.
fn damage(mut x: u64, p: &PathBuf) {
    let bytes = std::fs::read(p).unwrap();
    let s = Save::read(&bytes).unwrap();
    let u = uncompressed(&bytes, &s);
    let body_at = u.len() - s.body().len();
    // The player's two change forms, where the look is read from.
    let look_reads = s.player().is_ok();
    let hot: Vec<usize> = s
        .change_forms()
        .iter()
        .filter(|c| {
            look_reads
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
