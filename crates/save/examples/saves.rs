//! Reads every save in a folder and prints, per character, how the player
//! looks, then how many saves read and how long it took.
//!
//! `cargo run --release -p apoc-save --example saves -- [saves] [Data]`
//!
//! The saves default to `scratch/samples/saves`; the game's `Data` folder
//! (read only) defaults to Steam's Skyrim Special Edition. When the
//! plugins are there, the references are checked against them: a head part
//! should land on an `HDPT` record, the race on a `RACE` whose editor id is
//! the header's, every tint mask on one of that race's `TINI` numbers. The
//! plugin scan here is a throwaway, enough for those checks.
//!
//! Characters are numbered, not named: the saves are someone's own.

use apoc_save::{header_only, look, Form, FormRef, Look, Save, KIND_NPC};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A save read: its path, the header's race editor id and sex, the look.
type Read = (String, String, u16, Look, Save);
/// A plugin indexed: its master count, each record's offset and kind by
/// form id, and its bytes.
type Indexed = (u32, HashMap<u32, (usize, [u8; 4])>, Vec<u8>);

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "scratch/samples/saves".into()),
    );
    let data = PathBuf::from(args.next().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data")
    }));
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ess")))
        .collect();
    paths.sort();

    let mut plugins = Plugins::new(&data);
    let mut by_char: BTreeMap<String, Vec<Read>> = BTreeMap::new();
    let (mut t_head, mut t_full, mut t_look) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let mut failures = Vec::new();
    let mut ok = 0;
    for p in &paths {
        let bytes = std::fs::read(p).unwrap();
        let t = Instant::now();
        let listing = header_only(&bytes);
        t_head += t.elapsed();
        let t = Instant::now();
        let save = Save::read(&bytes);
        t_full += t.elapsed();
        let save = match (listing, save) {
            (Ok(_), Ok(s)) => s,
            (Err(e), _) | (_, Err(e)) => {
                failures.push(format!("{}: {e}", p.display()));
                continue;
            }
        };
        let t = Instant::now();
        let look = save.player();
        t_look += t.elapsed();
        match look {
            Ok(look) => {
                ok += 1;
                let h = save.header().clone();
                // Grouped by the character id the game puts in the file
                // name (the second field), which two characters of the same
                // name do not share.
                let id = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.split('_').nth(1))
                    .map(str::to_string)
                    .unwrap_or_else(|| h.name.clone());
                by_char.entry(id).or_default().push((
                    p.display().to_string(),
                    h.race_editor_id,
                    h.sex,
                    look,
                    save,
                ));
            }
            Err(e) => failures.push(format!("{}: look: {e}", p.display())),
        }
    }

    let mut problems = 0;
    for (n, (_, saves)) in by_char.iter().enumerate() {
        let (_, race_id, sex, look, _) = &saves[0];
        println!(
            "character {} ({} saves): {race_id}, sex {sex}",
            n + 1,
            saves.len()
        );
        println!("  race {}", show(&look.race.clone().unwrap_or(Form::None)));
        println!(
            "  face texture set {}  hair colour {}  skin tone {:02x?}",
            show(&look.face_texture_set),
            show(&look.hair_colour),
            look.skin_tone
        );
        let parts: Vec<String> = look.head_parts.iter().map(show).collect();
        println!("  head parts {}", parts.join(" "));
        println!(
            "  morphs {:?}",
            look.morphs
                .iter()
                .map(|m| (m * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>()
        );
        println!("  presets {:?}", look.presets);
        println!(
            "  tints {} (strength > 0: {})",
            look.tints.len(),
            look.tints.iter().filter(|t| t.strength > 0.0).count()
        );
        println!("  weight {:?}", look.weight);

        let mut bad = Vec::new();
        for (path, race_id, sex, l, s) in saves {
            if l.sex as u16 != *sex {
                bad.push("sex differs from the header".to_string());
            }
            let what: Vec<&str> = [
                (
                    sorted(&l.head_parts) != sorted(&look.head_parts),
                    "head parts (as a set)",
                ),
                (l.head_parts != look.head_parts, "head parts order"),
                (l.tints != look.tints, "tints"),
                (l.face_texture_set != look.face_texture_set, "texture set"),
                (l.morphs != look.morphs, "morphs"),
                (l.skin_tone != look.skin_tone, "skin tone"),
            ]
            .iter()
            .filter(|w| w.0)
            .map(|w| w.1)
            .collect();
            if !what.is_empty() {
                bad.push(format!(
                    "{} differ from the first save in {path}",
                    what.join(", ")
                ));
            }
            if !l.tints.iter().any(|t| t.colour[..3] == l.skin_tone[..3]) {
                bad.push("no tint has the face's skin tone".into());
            }
            if l.tints.is_empty() {
                bad.push("no player block".into());
            }
            npc_fully_read(s, &mut bad);
            if let Some(race) = &l.race {
                match plugins.record(race, s) {
                    Some(r) if r.kind == *b"RACE" => {
                        if r.edid.as_deref() != Some(race_id.as_str()) {
                            bad.push(format!("race editor id {:?}", r.edid));
                        }
                        for t in &l.tints {
                            if !r.tini.contains(&t.mask) {
                                bad.push(format!("tint mask {} not in the race", t.mask));
                            }
                        }
                    }
                    Some(r) => bad.push(format!("race is a {}", String::from_utf8_lossy(&r.kind))),
                    None => {}
                }
            }
            let mut want = vec![(&l.face_texture_set, *b"TXST"), (&l.hair_colour, *b"CLFM")];
            want.extend(l.head_parts.iter().map(|h| (h, *b"HDPT")));
            for (f, kind) in want {
                if let Some(r) = plugins.record(f, s) {
                    if r.kind != kind {
                        bad.push(format!(
                            "{} is a {}",
                            show(f),
                            String::from_utf8_lossy(&r.kind)
                        ));
                    }
                }
            }
        }
        let mut versions: Vec<Vec<String>> =
            saves.iter().map(|s| sorted(&s.3.head_parts)).collect();
        versions.sort();
        versions.dedup();
        let mut tint_versions: Vec<String> =
            saves.iter().map(|s| format!("{:?}", s.3.tints)).collect();
        tint_versions.sort();
        tint_versions.dedup();
        println!(
            "  versions over its saves: head parts {}, tints {}",
            versions.len(),
            tint_versions.len()
        );
        if versions.len() > 1 {
            let all: Vec<&String> = versions.iter().flatten().collect();
            let differ: Vec<&&String> = all
                .iter()
                .filter(|h| !versions.iter().all(|v| v.contains(h)))
                .collect();
            let mut differ: Vec<String> = differ.iter().map(|s| s.to_string()).collect();
            differ.sort();
            differ.dedup();
            println!("  head parts not in every save: {}", differ.join(" "));
        }
        bad.retain(|b| !b.contains("differ from the first save"));
        bad.sort();
        bad.dedup();
        problems += bad.len();
        for b in bad {
            println!("  CHECK FAILED: {b}");
        }
    }
    println!(
        "\n{ok} of {} saves read; {} failures; {problems} failed checks; {} references checked against plugins, {} in plugins not here",
        paths.len(),
        failures.len(),
        plugins.checked,
        plugins.absent
    );
    for f in &failures {
        println!("  {f}");
    }
    let n = paths.len().max(1) as u32;
    println!(
        "per save: header only {:?}; full read (decompress, lists, change form index) {:?}; player look {:?}",
        t_head / n,
        t_full / n,
        t_look / n
    );
}

fn show(f: &Form) -> String {
    match f {
        Form::None => "none".into(),
        Form::Plugin(r) => r.to_string(),
        Form::Created(id) => format!("created:{id:08X}"),
    }
}

/// Every byte of the player's NPC change form is accounted for by UESP's
/// layout: what follows the sex is the outfits the flags name, or nothing.
fn npc_fully_read(s: &Save, bad: &mut Vec<String>) {
    let me = Form::Plugin(FormRef {
        plugin: s.plugins()[0].clone(),
        id: 7,
    });
    let c = s.change_form(&me, KIND_NPC).unwrap();
    let d = s.data(c).unwrap();
    let outfits = 3 * ((c.flags >> 12) & 1) as usize + 3 * ((c.flags >> 13) & 1) as usize;
    let used = consumed(&d, c.flags);
    if used + outfits != d.len() {
        bad.push(format!(
            "NPC change form: {} bytes left over",
            d.len() as isize - (used + outfits) as isize
        ));
    }
}

fn consumed(d: &[u8], flags: u32) -> usize {
    // The shortest prefix that still reads the same.
    let full = look::read_npc(d, flags).unwrap();
    (0..=d.len())
        .find(|&n| look::read_npc(&d[..n], flags).as_ref() == Ok(&full))
        .unwrap()
}

struct Rec {
    kind: [u8; 4],
    edid: Option<String>,
    tini: Vec<u16>,
}

/// Just enough of the plugins to say what kind of record a form is.
struct Plugins {
    dir: PathBuf,
    files: HashMap<String, Option<Indexed>>,
    checked: usize,
    absent: usize,
}

impl Plugins {
    fn new(dir: &Path) -> Self {
        Plugins {
            dir: dir.to_path_buf(),
            files: HashMap::new(),
            checked: 0,
            absent: 0,
        }
    }

    fn record(&mut self, f: &Form, _s: &Save) -> Option<Rec> {
        let Form::Plugin(r) = f else { return None };
        let dir = self.dir.clone();
        let file = self
            .files
            .entry(r.plugin.to_lowercase())
            .or_insert_with(|| std::fs::read(dir.join(&r.plugin)).ok().map(index));
        let Some((masters, recs, bytes)) = file else {
            self.absent += 1;
            return None;
        };
        self.checked += 1;
        let &(at, kind) = recs.get(&((*masters << 24) | r.id))?;
        let (edid, tini) = if &kind == b"RACE" {
            race(&bytes[at..])
        } else {
            (None, vec![])
        };
        Some(Rec { kind, edid, tini })
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn index(bytes: Vec<u8>) -> Indexed {
    let mut recs = HashMap::new();
    let mut masters = 0;
    let mut p = 0;
    while p + 24 <= bytes.len() {
        let kind: [u8; 4] = bytes[p..p + 4].try_into().unwrap();
        if &kind == b"GRUP" {
            p += 24;
            continue;
        }
        let size = u32_at(&bytes, p + 4) as usize;
        if &kind == b"TES4" {
            let mut q = p + 24;
            while q + 6 <= p + 24 + size {
                if &bytes[q..q + 4] == b"MAST" {
                    masters += 1;
                }
                q += 6 + u16::from_le_bytes([bytes[q + 4], bytes[q + 5]]) as usize;
            }
        }
        recs.insert(u32_at(&bytes, p + 12), (p, kind));
        p += 24 + size;
    }
    (masters, recs, bytes)
}

fn race(rec: &[u8]) -> (Option<String>, Vec<u16>) {
    let size = u32_at(rec, 4) as usize;
    let flags = u32_at(rec, 8);
    let body = &rec[24..24 + size];
    let owned;
    let body = if flags & 0x40000 != 0 {
        owned = miniz_oxide::inflate::decompress_to_vec_zlib(&body[4..]).unwrap();
        &owned[..]
    } else {
        body
    };
    let (mut edid, mut tini) = (None, vec![]);
    let mut q = 0;
    while q + 6 <= body.len() {
        let n = u16::from_le_bytes([body[q + 4], body[q + 5]]) as usize;
        let v = &body[q + 6..q + 6 + n];
        match &body[q..q + 4] {
            b"EDID" => {
                edid =
                    Some(String::from_utf8_lossy(v.split(|c| *c == 0).next().unwrap()).into_owned())
            }
            b"TINI" => tini.push(u16::from_le_bytes([v[0], v[1]])),
            _ => {}
        }
        q += 6 + n;
    }
    (edid, tini)
}

fn sorted(v: &[Form]) -> Vec<String> {
    let mut s: Vec<String> = v.iter().map(show).collect();
    s.sort();
    s
}
