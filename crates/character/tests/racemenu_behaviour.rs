//! What `apoc_character::racemenu` and `plan_with` promise for RaceMenu's
//! named sliders, proved through the public API only (step 3 of
//! `docs/characters.md`).
//!
//! Every input is written here by hand: the ini files, the records, the
//! meshes and the tris. No byte comes from a third-party file. Generated
//! inputs come from a seeded generator, so a failure names its seed.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use apoc_bodyslide::headtri::{self, HeadMorph, HeadTri};
use apoc_character::build::{build, Built, Options};
use apoc_character::files::Source;
use apoc_character::racemenu::{
    parse_morphs, parse_races, parse_sliders, weights, SliderDef, SliderFile, SliderKind,
    SliderNote, FOLDER, MAX_CUSTOM, MAX_NOTES, MAX_TEXT, MORPHS,
};
use apoc_character::{plan, plan_with, Files, Look, RaceMenuMorphs, Records};
use apoc_nif::mesh;
use apoc_nif::node::{AvObject, Node, Transform};
use apoc_nif::{Block, Header, Nif, StreamHeader, VERSION_20_2_0_7};
use apoc_plugin::records::{
    Armor, ArmorAddon, Colour, HeadPart, HeadPartKind, Npc, PartFile, PartFileKind, Race,
    TextureSet,
};
use apoc_plugin::FormRef;

// ---------------------------------------------------------------------------
// A seeded generator (splitmix64).

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0x5DEE_CE66)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// 0 to `n`, both included.
    fn upto(&mut self, n: usize) -> usize {
        self.below(n as u64 + 1) as usize
    }
    fn one_in(&mut self, n: u64) -> bool {
        self.below(n) == 0
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

fn recase(r: &mut Rng, s: &str) -> String {
    s.chars()
        .map(|c| {
            if r.one_in(2) {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Writing valid ini files at random, with what they must read as.

/// Spaces and tabs.
fn pad(r: &mut Rng) -> String {
    (0..r.upto(3)).map(|_| *r.pick(&[' ', '\t'])).collect()
}

fn eol(r: &mut Rng) -> &'static str {
    if r.one_in(2) {
        "\r\n"
    } else {
        "\n"
    }
}

/// A trailing comment, or nothing. Comments hold the characters the lines
/// are split on, which must not count.
fn comment(r: &mut Rng) -> String {
    if r.one_in(3) {
        let text = *r.pick(&["", "note", "a, b = c", "[Male]", "x # y", "None, Slider"]);
        format!("{}#{text}", pad(r))
    } else {
        String::new()
    }
}

/// Blank lines, lines of padding and comment lines.
fn filler(r: &mut Rng, out: &mut String) {
    for _ in 0..r.upto(2) {
        match r.below(3) {
            0 => {}
            1 => out.push_str(&pad(r)),
            _ => {
                let p = pad(r);
                let c = r.pick(&["# a comment", "#", "#[Female]", "# x = 1, Slider, A, B"]);
                out.push_str(&format!("{p}{c}"));
            }
        }
        out.push_str(eol(r));
    }
}

/// A name: a letter, then letters, digits and `_`, perhaps with one space
/// inside. Never `None`, which means no morph.
fn word(r: &mut Rng, space: bool) -> String {
    const FIRST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    const REST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_";
    loop {
        let mut s = String::new();
        s.push(*r.pick(FIRST) as char);
        for _ in 0..r.upto(12) {
            s.push(*r.pick(REST) as char);
        }
        if space && r.one_in(3) {
            s.push(' ');
            s.push(*r.pick(FIRST) as char);
            for _ in 0..r.upto(5) {
                s.push(*r.pick(REST) as char);
            }
        }
        if !s.eq_ignore_ascii_case("none") && !s.eq_ignore_ascii_case("data") {
            return s;
        }
    }
}

fn none_word(r: &mut Rng) -> &'static str {
    r.pick(&["None", "none", "NONE", "nOnE"])
}

/// A slider's data after the `=`, and what it reads as.
fn slider_data(r: &mut Rng) -> (String, i32, SliderKind) {
    let category = if r.one_in(2) {
        *r.pick(&[8, 16, 32, 128, 1024, -1, 0])
    } else {
        r.below(1 << 20) as i32 - (1 << 19)
    };
    let (p1, p2, p3, p4, p5, p6) = (pad(r), pad(r), pad(r), pad(r), pad(r), pad(r));
    match r.below(3) {
        0 => {
            let side = |r: &mut Rng| {
                if r.one_in(3) {
                    (none_word(r).to_string(), None)
                } else {
                    let w = word(r, true);
                    (w.clone(), Some(w))
                }
            };
            let (lt, low) = side(r);
            let (ht, high) = side(r);
            let ty = recase(r, "Slider");
            (
                format!("{p1}{category}{p2},{p3}{ty}{p4},{p5}{lt}{p6},{p1}{ht}{p2}"),
                category,
                SliderKind::Slider { low, high },
            )
        }
        1 => {
            let prefix = word(r, true);
            let count = if r.one_in(2) {
                r.below(40) as u32
            } else {
                r.next() as u32
            };
            let ty = recase(r, "Preset");
            (
                format!("{p1}{category}{p2},{p3}{ty}{p4},{p5}{prefix}{p6},{p1}{count}{p2}"),
                category,
                SliderKind::Preset { prefix, count },
            )
        }
        _ => {
            let kind = r.next() as u32;
            let ty = recase(r, "HeadPart");
            (
                format!("{p1}{category}{p2},{p3}{ty}{p4},{p5}{kind}{p6}"),
                category,
                SliderKind::HeadPart { kind },
            )
        }
    }
}

/// A slider file, what it reads as, and how many notes it must give (one
/// for each name given again in its section and each section not known).
fn gen_sliders(r: &mut Rng) -> (String, SliderFile, usize) {
    let mut s = String::new();
    if r.one_in(4) {
        s.push('\u{FEFF}');
    }
    filler(r, &mut s);
    let mut exp = SliderFile::default();
    let mut notes = 0;
    for _ in 0..r.upto(5) {
        let which = r.below(5);
        let title = match which {
            0 | 1 => recase(r, "Male"),
            2 | 3 => recase(r, "Female"),
            _ => r.pick(&["Other", "Males", "Body", "Female2"]).to_string(),
        };
        let (a, b, c, d) = (pad(r), pad(r), pad(r), pad(r));
        let cm = comment(r);
        s.push_str(&format!("{a}[{b}{title}{c}]{d}{cm}{}", eol(r)));
        if which == 4 {
            notes += 1;
        }
        for _ in 0..r.upto(6) {
            filler(r, &mut s);
            let list = match which {
                0 | 1 => Some(&mut exp.male),
                2 | 3 => Some(&mut exp.female),
                _ => None,
            };
            let have: Vec<String> = list
                .as_ref()
                .map(|l| l.iter().map(|d| d.name.clone()).collect())
                .unwrap_or_default();
            let again = !have.is_empty() && r.one_in(5);
            let name = if again {
                let n = r.pick(&have).clone();
                recase(r, &n)
            } else {
                loop {
                    let w = word(r, true);
                    if !have.iter().any(|h| h.eq_ignore_ascii_case(&w)) {
                        break w;
                    }
                }
            };
            let (data, category, kind) = slider_data(r);
            let (a, b) = (pad(r), pad(r));
            let cm = comment(r);
            s.push_str(&format!("{a}{name}{b}={data}{cm}{}", eol(r)));
            if let Some(list) = list {
                if again {
                    notes += 1;
                } else {
                    list.push(SliderDef {
                        name,
                        category,
                        kind,
                    });
                }
            }
        }
    }
    filler(r, &mut s);
    (s, exp, notes)
}

/// A path of 1 to 4 parts, written with either slash (doubled at times) and
/// any case, and its key.
fn gen_path(r: &mut Rng, ext: &str) -> (Vec<String>, String) {
    let n = 1 + r.upto(3);
    let mut parts: Vec<String> = (0..n)
        .map(|_| {
            let w = word(r, true);
            if r.one_in(6) {
                format!("!{w}")
            } else {
                w
            }
        })
        .collect();
    parts.last_mut().unwrap().push_str(ext);
    let k = parts.join("/").to_ascii_lowercase();
    (parts, k)
}

fn write_path(r: &mut Rng, parts: &[String]) -> String {
    // Never one in front: an absolute path is refused.
    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            s.push_str(r.pick(&["\\", "/", "\\\\", "//", "\\/"]));
        }
        s.push_str(&recase(r, p));
    }
    s
}

type Extensions = Vec<(String, Vec<String>)>;

/// A `morphs.ini` and what it reads as.
fn gen_morphs(r: &mut Rng) -> (String, Extensions) {
    let mut s = String::new();
    if r.one_in(4) {
        s.push('\u{FEFF}');
    }
    // The sources, each with the parts it is written from and its key.
    let mut sources: Vec<(Vec<String>, String)> = Vec::new();
    for _ in 0..1 + r.upto(3) {
        let (mut parts, _) = gen_path(r, ".tri");
        // At times written under `Meshes\`, which is where it is anyway.
        if r.one_in(3) {
            parts.insert(0, "Meshes".into());
        }
        let k = parts.join("/").to_ascii_lowercase();
        let k = if k.starts_with("meshes/") {
            k
        } else {
            format!("meshes/{k}")
        };
        if !sources.iter().any(|(_, have)| *have == k) {
            sources.push((parts, k));
        }
    }
    let mut exp: Extensions = Vec::new();
    for _ in 0..r.upto(6) {
        filler(r, &mut s);
        let (parts, k) = sources[r.below(sources.len() as u64) as usize].clone();
        let src = write_path(r, &parts);
        let mut line_extras: Vec<String> = Vec::new();
        let mut text = String::new();
        let mut written: Vec<(Vec<String>, String)> = Vec::new();
        for _ in 0..1 + r.upto(4) {
            let (eparts, ek) = if !written.is_empty() && r.one_in(4) {
                written[r.below(written.len() as u64) as usize].clone()
            } else {
                gen_path(r, ".tri")
            };
            written.push((eparts.clone(), ek.clone()));
            let e = write_path(r, &eparts);
            let (a, b) = (pad(r), pad(r));
            text.push_str(&format!(",{a}{e}{b}"));
            // An empty field between two commas is passed over.
            if r.one_in(6) {
                text.push_str(&format!(",{}", pad(r)));
            }
            let ek = format!("{MORPHS}/{ek}");
            if !line_extras.contains(&ek) {
                line_extras.push(ek);
            }
        }
        let kw = recase(r, "extension");
        let (a, b, c, d) = (pad(r), pad(r), pad(r), pad(r));
        let cm = comment(r);
        s.push_str(&format!("{a}{kw}{b}={c}{src}{d}{text}{cm}{}", eol(r)));
        match exp.iter_mut().find(|(have, _)| *have == k) {
            Some((_, extras)) => {
                for e in line_extras {
                    if !extras.contains(&e) {
                        extras.push(e);
                    }
                }
            }
            None => exp.push((k, line_extras)),
        }
    }
    filler(r, &mut s);
    (s, exp)
}

/// A `races.ini`, what it reads as, and how many races it gives again.
fn gen_races(r: &mut Rng) -> (String, Vec<(String, String)>, usize) {
    let mut s = String::new();
    let mut exp: Vec<(String, String)> = Vec::new();
    let mut again = 0;
    for _ in 0..r.upto(8) {
        filler(r, &mut s);
        let repeat = !exp.is_empty() && r.one_in(5);
        let race = if repeat {
            let have = exp[r.below(exp.len() as u64) as usize].0.clone();
            recase(r, &have)
        } else {
            loop {
                let w = word(r, false);
                if !exp.iter().any(|(h, _)| h.eq_ignore_ascii_case(&w)) {
                    break w;
                }
            }
        };
        let (parts, _) = gen_path(r, ".ini");
        let path = format!("sliders\\{}", write_path(r, &parts));
        let (a, b, c, d) = (pad(r), pad(r), pad(r), pad(r));
        let cm = comment(r);
        s.push_str(&format!("{a}{race}{b}={c}{path}{d}{cm}{}", eol(r)));
        if repeat {
            again += 1;
        } else {
            exp.push((race, path));
        }
    }
    (s, exp, again)
}

/// Notes are held to [`MAX_NOTES`], and past it the last says how many
/// more there were.
fn check_notes(notes: &[String], what: &str) {
    assert!(
        notes.len() <= MAX_NOTES + 1,
        "{what}: {} notes",
        notes.len()
    );
    for (i, n) in notes.iter().enumerate() {
        let more = n.starts_with("And ") && n.ends_with(" more.");
        if more {
            assert_eq!(i, MAX_NOTES, "{what}: a count of more before the end");
        }
    }
    if notes.len() == MAX_NOTES + 1 {
        let last = &notes[MAX_NOTES];
        assert!(
            last.starts_with("And ") && last.ends_with(" more."),
            "{what}: {last}"
        );
    }
}

// ---------------------------------------------------------------------------
// 1. Parsers.

#[test]
fn every_slider_of_a_valid_slider_file_is_read_with_its_fields() {
    // What the seeds covered, so a generator gone quiet is caught.
    let (mut defs, mut nones, mut presets, mut parts, mut spaced, mut noted, mut crlf) =
        (0, 0, 0, 0, 0, 0, 0);
    for seed in 0..400 {
        let mut r = Rng::new(seed);
        let (text, exp, notes) = gen_sliders(&mut r);
        let (got, got_notes) = parse_sliders(text.as_bytes(), "s.ini");
        assert_eq!(got, exp, "seed {seed}:\n{text}");
        assert_eq!(got_notes.len(), notes, "seed {seed}: {got_notes:?}\n{text}");
        for d in exp.male.iter().chain(&exp.female) {
            defs += 1;
            spaced += d.name.contains(' ') as usize;
            match &d.kind {
                SliderKind::Slider { low, high } => {
                    nones += low.is_none() as usize + high.is_none() as usize
                }
                SliderKind::Preset { .. } => presets += 1,
                SliderKind::HeadPart { .. } => parts += 1,
            }
        }
        noted += notes;
        crlf += text.contains("\r\n") as usize;
    }
    for (what, n) in [
        ("sliders", defs),
        ("None bounds", nones),
        ("presets", presets),
        ("head parts", parts),
        ("names with spaces", spaced),
        ("notes", noted),
        ("CRLF files", crlf),
    ] {
        assert!(n >= 20, "only {n} {what} generated");
    }
}

#[test]
fn every_extension_of_a_valid_morphs_ini_is_read_once_with_its_paths() {
    let (mut extras, mut merged, mut deduped) = (0, 0, 0);
    for seed in 0..400 {
        let mut r = Rng::new(seed);
        let (text, exp) = gen_morphs(&mut r);
        let (got, notes) = parse_morphs(text.as_bytes(), "morphs.ini");
        assert_eq!(got, exp, "seed {seed}:\n{text}");
        assert!(notes.is_empty(), "seed {seed}: {notes:?}\n{text}");
        // A source on more lines than one is merged into one entry.
        let lines = text
            .lines()
            .filter(|l| l.split('#').next().unwrap_or_default().contains('='))
            .count();
        extras += exp.iter().map(|(_, e)| e.len()).sum::<usize>();
        merged += (lines > exp.len()) as usize;
        deduped += exp.iter().any(|(_, e)| e.len() >= 2) as usize;
    }
    assert!(
        extras >= 200 && merged >= 20 && deduped >= 20,
        "{extras} {merged} {deduped}"
    );
}

#[test]
fn every_race_of_a_valid_races_ini_is_read_with_its_file_as_written() {
    let (mut races, mut repeats) = (0, 0);
    for seed in 0..400 {
        let mut r = Rng::new(seed);
        let (text, exp, again) = gen_races(&mut r);
        let (got, notes) = parse_races(text.as_bytes(), "races.ini");
        assert_eq!(got, exp, "seed {seed}:\n{text}");
        assert_eq!(notes.len(), again, "seed {seed}: {notes:?}\n{text}");
        races += exp.len();
        repeats += again;
    }
    assert!(races >= 200 && repeats >= 20, "{races} {repeats}");
}

/// A fixed file with each feature at once, so a failure reads plainly.
#[test]
fn a_slider_file_with_every_feature_reads_as_written_by_hand() {
    let text = "\u{FEFF}# header, with = and [Female]\r\n\
        \r\n\
        \t \r\n\
        [ male ]   # men\r\n\
        Jaw Width\t=\t8 ,\tslider , JawNarrow ,\tNONE\r\n\
        [Female]\n\
        \tAB Lip  =  -128,SLIDER,None,none\r\n\
        Eyes=32,preset,Eye Type,0\n\
        part = 4 , HEADPART , 4294967295 # last\r\n\
        [Male]\r\n\
        jaw width = 8, Slider, A, B\r\n\
        Brow = 16, Slider, Brow Down, Brow Up";
    let (s, notes) = parse_sliders(text.as_bytes(), "s.ini");
    let def = |name: &str, category: i32, kind: SliderKind| SliderDef {
        name: name.into(),
        category,
        kind,
    };
    assert_eq!(
        s,
        SliderFile {
            male: vec![
                def(
                    "Jaw Width",
                    8,
                    SliderKind::Slider {
                        low: Some("JawNarrow".into()),
                        high: None
                    }
                ),
                def(
                    "Brow",
                    16,
                    SliderKind::Slider {
                        low: Some("Brow Down".into()),
                        high: Some("Brow Up".into())
                    }
                ),
            ],
            female: vec![
                def(
                    "AB Lip",
                    -128,
                    SliderKind::Slider {
                        low: None,
                        high: None
                    }
                ),
                def(
                    "Eyes",
                    32,
                    SliderKind::Preset {
                        prefix: "Eye Type".into(),
                        count: 0
                    }
                ),
                def("part", 4, SliderKind::HeadPart { kind: u32::MAX }),
            ],
        }
    );
    // `jaw width` again in the men's section.
    assert_eq!(notes.len(), 1, "{notes:?}");
}

/// The bytes of every kind a hostile or broken file might hold.
fn garbage(r: &mut Rng) -> Vec<u8> {
    const TOKENS: &[&str] = &[
        "=",
        ",",
        "[",
        "]",
        "#",
        "\r\n",
        "\n",
        "\r",
        "\t",
        " ",
        "None",
        "Slider",
        "Preset",
        "HeadPart",
        "Male",
        "Female",
        "[Female]\n",
        "[Male]\r\n",
        "extension",
        "..",
        "\\",
        "/",
        ":",
        "C:",
        "-1",
        "99999999999999999999",
        "0",
        "7",
        "NaN",
        "é",
        "\u{FEFF}",
        "\u{0}",
        "x",
        "A b",
        "a.tri",
    ];
    let n = r.upto(600);
    let mut out = Vec::new();
    for _ in 0..n {
        if r.one_in(6) {
            out.push(r.next() as u8);
        } else {
            out.extend_from_slice(r.pick(TOKENS).as_bytes());
        }
    }
    out
}

/// What any parse returns stays within its bounds.
fn check_parsed(bytes: &[u8], what: &str) {
    let (s, notes) = parse_sliders(bytes, what);
    check_notes(&notes, what);
    for d in s.male.iter().chain(&s.female) {
        assert!(!d.name.is_empty() && d.name.len() <= MAX_TEXT, "{what}");
        assert_eq!(d.name.trim(), d.name, "{what}");
        if let SliderKind::Slider { low, high } = &d.kind {
            for m in [low, high].into_iter().flatten() {
                assert!(!m.is_empty() && m.len() <= MAX_TEXT, "{what}");
                assert!(!m.eq_ignore_ascii_case("none"), "{what}");
            }
        }
    }
    let (m, notes) = parse_morphs(bytes, what);
    check_notes(&notes, what);
    for (source, extras) in &m {
        assert!(source.starts_with("meshes/"), "{what}: {source}");
        assert!(!extras.is_empty(), "{what}");
        let mut keys = vec![source.clone()];
        for e in extras {
            assert!(e.starts_with(&format!("{MORPHS}/")), "{what}: {e}");
            keys.push(e.clone());
        }
        for k in keys {
            assert!(
                k.split('/')
                    .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(':')),
                "{what}: {k}"
            );
        }
    }
    let (races, notes) = parse_races(bytes, what);
    check_notes(&notes, what);
    for (race, path) in &races {
        assert!(!race.is_empty() && race.len() <= MAX_TEXT, "{what}");
        assert!(!path.is_empty() && path.len() <= MAX_TEXT, "{what}");
    }
}

#[test]
fn garbage_never_panics_and_what_it_gives_stays_in_bounds() {
    for seed in 0..1500 {
        let mut r = Rng::new(seed);
        let bytes = garbage(&mut r);
        check_parsed(&bytes, &format!("seed {seed}"));
    }
    // Pure noise.
    for seed in 0..300 {
        let mut r = Rng::new(seed ^ 0xABCD);
        let bytes: Vec<u8> = (0..r.upto(3000)).map(|_| r.next() as u8).collect();
        check_parsed(&bytes, &format!("noise {seed}"));
    }
}

/// A list cut short is the full one's first entries, but for its last
/// (which may be cut too).
fn prefix_but_last<T: PartialEq + std::fmt::Debug>(cut: &[T], full: &[T], what: &str) {
    if let Some((_, head)) = cut.split_last() {
        assert!(head.len() <= full.len(), "{what}");
        assert_eq!(head, &full[..head.len()], "{what}");
    }
}

#[test]
fn a_valid_file_cut_anywhere_reads_as_its_start() {
    for seed in 0..60 {
        let mut r = Rng::new(seed);
        let (text, full, _) = gen_sliders(&mut r);
        let bytes = text.as_bytes();
        let step = (bytes.len() / 80).max(1);
        for cut in (0..=bytes.len()).step_by(step) {
            let what = format!("seed {seed} cut {cut}");
            let (s, notes) = parse_sliders(&bytes[..cut], &what);
            check_notes(&notes, &what);
            prefix_but_last(&s.male, &full.male, &what);
            prefix_but_last(&s.female, &full.female, &what);
            // Only the last line is cut, and it is in one section.
            let exact = |a: &[SliderDef], b: &[SliderDef]| a.len() <= b.len() && a == &b[..a.len()];
            assert!(
                exact(&s.male, &full.male) || exact(&s.female, &full.female),
                "{what}"
            );
        }

        let (text, full) = gen_morphs(&mut r);
        let pairs = |m: &Extensions| -> HashSet<(String, String)> {
            m.iter()
                .flat_map(|(s, es)| es.iter().map(move |e| (s.clone(), e.clone())))
                .collect()
        };
        let all = pairs(&full);
        let bytes = text.as_bytes();
        let step = (bytes.len() / 80).max(1);
        for cut in (0..=bytes.len()).step_by(step) {
            let what = format!("seed {seed} morphs cut {cut}");
            let (m, notes) = parse_morphs(&bytes[..cut], &what);
            check_notes(&notes, &what);
            let new: Vec<_> = pairs(&m).difference(&all).cloned().collect();
            assert!(new.len() <= 1, "{what}: {new:?}");
        }

        let (text, full, _) = gen_races(&mut r);
        let bytes = text.as_bytes();
        for cut in 0..=bytes.len() {
            let what = format!("seed {seed} races cut {cut}");
            let (got, notes) = parse_races(&bytes[..cut], &what);
            check_notes(&notes, &what);
            prefix_but_last(&got, &full, &what);
        }
    }
}

#[test]
fn notes_are_capped_and_count_what_was_left_out() {
    for (n, what) in [
        (MAX_NOTES - 1, "under"),
        (MAX_NOTES, "at"),
        (MAX_NOTES + 37, "over"),
    ] {
        let bad: String = (0..n).map(|_| "junk\r\n").collect();
        for notes in [
            parse_sliders(format!("[Male]\r\n{bad}").as_bytes(), "s").1,
            parse_morphs(bad.as_bytes(), "m").1,
            parse_races(bad.as_bytes(), "r").1,
        ] {
            check_notes(&notes, what);
            if n <= MAX_NOTES {
                assert_eq!(notes.len(), n, "{what}");
            } else {
                assert_eq!(notes.len(), MAX_NOTES + 1, "{what}");
                assert_eq!(notes[MAX_NOTES], "And 37 more.", "{what}");
            }
        }
    }
}

/// Notes from many folders together are held to the same most, and the
/// count of those left out is right.
#[test]
fn notes_across_folders_are_capped_and_counted() {
    let mut files = Files::new();
    let plugins: Vec<String> = (0..50).map(|i| format!("Mod{i}.esp")).collect();
    for p in &plugins {
        // Ten lines it can't read, each a note.
        let bad: String = (0..10).map(|_| "junk\r\n").collect();
        files.insert(&format!("{FOLDER}/{p}/morphs.ini"), bad.into_bytes());
    }
    let names: Vec<&str> = plugins.iter().map(String::as_str).collect();
    let rm = RaceMenuMorphs::read(&names, &files);
    check_notes(&rm.notes, "folders");
    assert_eq!(rm.notes.len(), MAX_NOTES + 1);
    assert_eq!(rm.notes[MAX_NOTES], "And 300 more.");
    assert!(rm.is_empty());
}

#[test]
fn one_file_over_the_cap_is_counted_right_when_read_in_its_folder() {
    let mut files = Files::new();
    let bad: String = (0..MAX_NOTES + 100).map(|_| "junk\r\n").collect();
    files.insert(&format!("{FOLDER}/mod.esp/morphs.ini"), bad.into_bytes());
    let rm = RaceMenuMorphs::read(&["Mod.esp"], &files);
    check_notes(&rm.notes, "one file");
    // MAX_NOTES + 100 notes, MAX_NOTES kept: 100 were left out.
    assert_eq!(rm.notes[MAX_NOTES], "And 100 more.");
}

// ---------------------------------------------------------------------------
// 2. `weights`, each expectation worked out by hand.

fn slider(name: &str, low: Option<&str>, high: Option<&str>) -> SliderDef {
    SliderDef {
        name: name.into(),
        category: 16,
        kind: SliderKind::Slider {
            low: low.map(Into::into),
            high: high.map(Into::into),
        },
    }
}

fn preset(name: &str, prefix: &str, count: u32) -> SliderDef {
    SliderDef {
        name: name.into(),
        category: 32,
        kind: SliderKind::Preset {
            prefix: prefix.into(),
            count,
        },
    }
}

fn custom(list: &[(&str, f32)]) -> Vec<(String, f32)> {
    list.iter().map(|(n, v)| (n.to_string(), *v)).collect()
}

fn map(list: &[(&str, f32)]) -> HashMap<String, f32> {
    list.iter().map(|(n, v)| (n.to_string(), *v)).collect()
}

fn defs() -> Vec<SliderDef> {
    vec![
        slider("Lip Width", Some("LIP_Narrow"), Some("LIP_Wide")),
        slider("OnlyUp", None, Some("Brow_Up")),
        slider("OnlyDown", Some("Brow_Down"), None),
        preset("Eyes", "EyeType", 4),
        preset("Never", "Nothing", 0),
        SliderDef {
            name: "Makeup".into(),
            category: 8,
            kind: SliderKind::HeadPart { kind: 3 },
        },
    ]
}

#[test]
fn a_slider_turns_on_its_low_morph_below_nought_and_its_high_above() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    for (v, want) in [
        (-1.0, map(&[("lip_narrow", 1.0)])),
        (-0.5, map(&[("lip_narrow", 0.5)])),
        (-0.0625, map(&[("lip_narrow", 0.0625)])),
        (-3.0, map(&[("lip_narrow", 3.0)])),
        (0.25, map(&[("lip_wide", 0.25)])),
        (1.0, map(&[("lip_wide", 1.0)])),
        (2.5, map(&[("lip_wide", 2.5)])),
        (0.0, map(&[])),
        (-0.0, map(&[])),
    ] {
        let (w, notes) = weights(&custom(&[("Lip Width", v)]), &s);
        assert_eq!(w, want, "{v}");
        assert!(notes.is_empty(), "{v}: {notes:?}");
    }
}

#[test]
fn a_slider_with_no_morph_on_its_side_says_so() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    type Case = (&'static str, f32, HashMap<String, f32>, Vec<SliderNote>);
    let cases: [Case; 6] = [
        (
            "OnlyUp",
            -0.5,
            map(&[]),
            vec![SliderNote::NoMorphThatWay {
                name: "OnlyUp".into(),
                value: -0.5,
            }],
        ),
        ("OnlyUp", 0.5, map(&[("brow_up", 0.5)]), vec![]),
        (
            "OnlyDown",
            0.75,
            map(&[]),
            vec![SliderNote::NoMorphThatWay {
                name: "OnlyDown".into(),
                value: 0.75,
            }],
        ),
        ("OnlyDown", -0.75, map(&[("brow_down", 0.75)]), vec![]),
        // At nought neither side is needed.
        ("OnlyUp", 0.0, map(&[]), vec![]),
        ("OnlyDown", 0.0, map(&[]), vec![]),
    ];
    for (name, v, want, want_notes) in cases {
        let (w, notes) = weights(&custom(&[(name, v)]), &s);
        assert_eq!(w, want, "{name} {v}");
        assert_eq!(notes, want_notes, "{name} {v}");
    }
}

#[test]
fn a_preset_turns_on_its_choice_and_nought_is_none() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    let (w, notes) = weights(&custom(&[("Eyes", 0.0)]), &s);
    assert!(w.is_empty() && notes.is_empty());
    for n in 1..=4u32 {
        let (w, notes) = weights(&custom(&[("Eyes", n as f32)]), &s);
        assert_eq!(w, map(&[(&format!("eyetype{n}"), 1.0)]), "{n}");
        assert!(notes.is_empty(), "{n}");
    }
    for v in [5.0, 6.0, 100.0, -1.0, -4.0, 0.5, 2.25, 3.9, -0.5, 1e9] {
        let (w, notes) = weights(&custom(&[("Eyes", v)]), &s);
        assert!(w.is_empty(), "{v}: {w:?}");
        assert_eq!(
            notes,
            [SliderNote::NotAChoice {
                name: "Eyes".into(),
                value: v,
                count: 4
            }],
            "{v}"
        );
    }
    // A preset with no choices has only nought.
    let (w, notes) = weights(&custom(&[("Never", 0.0)]), &s);
    assert!(w.is_empty() && notes.is_empty());
    let (w, notes) = weights(&custom(&[("Never", 1.0)]), &s);
    assert!(w.is_empty());
    assert_eq!(
        notes,
        [SliderNote::NotAChoice {
            name: "Never".into(),
            value: 1.0,
            count: 0
        }]
    );
}

#[test]
fn a_head_part_slider_and_an_unknown_name_say_so_and_shape_nothing() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    let (w, notes) = weights(&custom(&[("Makeup", 2.0), ("Nobody", 0.5)]), &s);
    assert!(w.is_empty());
    assert_eq!(
        notes,
        [
            SliderNote::HeadPart {
                name: "Makeup".into(),
                value: 2.0
            },
            SliderNote::Unknown {
                name: "Nobody".into()
            }
        ]
    );
    // With no sliders at all, each name is unknown.
    let (w, notes) = weights(&custom(&[("Lip Width", 0.5), ("Eyes", 1.0)]), &[]);
    assert!(w.is_empty());
    assert_eq!(
        notes,
        [
            SliderNote::Unknown {
                name: "Lip Width".into()
            },
            SliderNote::Unknown {
                name: "Eyes".into()
            }
        ]
    );
}

#[test]
fn names_compare_without_case_and_morphs_come_out_in_lower_case() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    let (w, notes) = weights(
        &custom(&[("LIP WIDTH", -0.25), ("eyes", 3.0), ("onlyup", 0.5)]),
        &s,
    );
    assert_eq!(
        w,
        map(&[("lip_narrow", 0.25), ("eyetype3", 1.0), ("brow_up", 0.5)])
    );
    assert!(notes.is_empty());
    // A name differing by more than case is another name.
    let (w, notes) = weights(&custom(&[("LipWidth", 0.5)]), &s);
    assert!(w.is_empty());
    assert_eq!(
        notes,
        [SliderNote::Unknown {
            name: "LipWidth".into()
        }]
    );
}

#[test]
fn a_name_given_twice_keeps_its_first_even_when_the_first_draws_nothing() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    let (w, notes) = weights(&custom(&[("Lip Width", 0.5), ("lip width", -0.7)]), &s);
    assert_eq!(w, map(&[("lip_wide", 0.5)]));
    assert_eq!(
        notes,
        [SliderNote::Repeated {
            name: "lip width".into()
        }]
    );
    for first in [0.0, f32::NAN] {
        let (w, notes) = weights(&custom(&[("Lip Width", first), ("LIP WIDTH", 0.5)]), &s);
        assert!(w.is_empty(), "{first}: {w:?}");
        assert_eq!(
            notes.last(),
            Some(&SliderNote::Repeated {
                name: "LIP WIDTH".into()
            }),
            "{first}"
        );
    }
    // Two sliders of one name for the race: the first is the name's.
    let a = slider("Twice", None, Some("First"));
    let b = slider("TWICE", None, Some("Second"));
    let (w, notes) = weights(&custom(&[("twice", 1.0)]), &[&a, &b]);
    assert_eq!(w, map(&[("first", 1.0)]));
    assert!(notes.is_empty());
}

#[test]
fn a_value_that_is_not_a_number_shapes_nothing_and_says_so() {
    let d = defs();
    let s: Vec<&SliderDef> = d.iter().collect();
    for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for name in ["Lip Width", "Eyes", "Makeup", "OnlyUp"] {
            let (w, notes) = weights(&custom(&[(name, v)]), &s);
            assert!(w.is_empty(), "{name} {v}");
            assert_eq!(
                notes,
                [SliderNote::NotANumber { name: name.into() }],
                "{name} {v}"
            );
        }
    }
    // The largest finite values are numbers.
    let (w, notes) = weights(&custom(&[("Lip Width", f32::MAX)]), &s);
    assert_eq!(w, map(&[("lip_wide", f32::MAX)]));
    assert!(notes.is_empty());
}

#[test]
fn sliders_on_one_morph_add_up_and_notes_follow_the_looks_order() {
    let a = slider("A", Some("Shared"), None);
    let b = slider("B", None, Some("shared"));
    let c = slider("C", None, Some("Other"));
    let (w, notes) = weights(
        &custom(&[("C", 0.5), ("Gone", 1.0), ("A", -0.25), ("B", 0.5)]),
        &[&a, &b, &c],
    );
    assert_eq!(w, map(&[("shared", 0.75), ("other", 0.5)]));
    assert_eq!(
        notes,
        [SliderNote::Unknown {
            name: "Gone".into()
        }]
    );
}

/// A slider and a preset that turn on one morph: the result does not
/// depend on which the look gives first.
#[test]
fn a_slider_and_a_preset_on_one_morph_do_not_depend_on_order() {
    let s = slider("Extra", None, Some("EyeType2"));
    let p = preset("Eyes", "EyeType", 4);
    let (one, _) = weights(&custom(&[("Extra", 0.5), ("Eyes", 2.0)]), &[&s, &p]);
    let (two, _) = weights(&custom(&[("Eyes", 2.0), ("Extra", 0.5)]), &[&s, &p]);
    assert_eq!(one, two);
}

#[test]
fn past_the_most_named_sliders_the_rest_are_counted_not_drawn() {
    let defs: Vec<SliderDef> = (0..MAX_CUSTOM + 3)
        .map(|i| slider(&format!("S{i}"), None, Some(&format!("M{i}"))))
        .collect();
    let s: Vec<&SliderDef> = defs.iter().collect();
    let look: Vec<(String, f32)> = (0..MAX_CUSTOM + 3)
        .map(|i| (format!("S{i}"), 1.0))
        .collect();
    let (w, notes) = weights(&look, &s);
    assert_eq!(w.len(), MAX_CUSTOM);
    assert!(w.contains_key(&format!("m{}", MAX_CUSTOM - 1)));
    assert!(!w.contains_key(&format!("m{MAX_CUSTOM}")));
    assert_eq!(notes, [SliderNote::TooMany(3)]);
}

// ---------------------------------------------------------------------------
// 3. `RaceMenuMorphs`.

fn folder(plugin: &str) -> String {
    format!("{FOLDER}/{}", plugin.to_ascii_lowercase())
}

/// Files inserted as a mod would ship them: Windows paths, any case.
fn win(plugin: &str, rest: &str) -> String {
    format!("Meshes\\Actors\\Character\\FaceGenMorphs\\{plugin}\\{rest}")
}

fn names(list: &[&SliderDef]) -> Vec<String> {
    list.iter().map(|d| d.name.clone()).collect()
}

fn high_of(d: &SliderDef) -> Option<String> {
    match &d.kind {
        SliderKind::Slider { high, .. } => high.clone(),
        _ => None,
    }
}

#[test]
fn folders_of_plugins_not_loaded_are_never_asked_for_or_read() {
    let mut f = Files::new();
    f.insert(
        &win("Loaded.esp", "morphs.ini"),
        b"extension = Head\\Chargen.tri, Loaded\\One.tri\r\n".to_vec(),
    );
    f.insert(
        &win("Loaded.esp", "races.ini"),
        b"NordRace = sliders\\a.ini\r\n".to_vec(),
    );
    f.insert(
        &win("Loaded.esp", "sliders\\a.ini"),
        b"[Female]\r\nS = 1, Slider, None, Mine\r\n".to_vec(),
    );
    f.insert(
        &win("NotLoaded.esp", "morphs.ini"),
        b"extension = Head\\Chargen.tri, NotLoaded\\Two.tri\r\n".to_vec(),
    );
    f.insert(
        &win("NotLoaded.esp", "races.ini"),
        b"NordRace = sliders\\b.ini\r\nElfRace = sliders\\b.ini\r\n".to_vec(),
    );
    f.insert(
        &win("NotLoaded.esp", "sliders\\b.ini"),
        b"[Female]\r\nS = 1, Slider, None, Theirs\r\nT = 1, Slider, None, X\r\n".to_vec(),
    );
    // The plugin as the load order spells it, in another case.
    let plugins = ["Skyrim.esm", "LOADED.ESP"];
    let (needed, _) = RaceMenuMorphs::needed(&plugins, &Files::new());
    assert!(
        needed.iter().all(|k| !k.contains("notloaded")),
        "{needed:?}"
    );
    // With the loaded folder's races.ini fetched, its slider file is asked
    // for, and nothing of the other folder.
    let mut got = Files::new();
    got.insert(
        &win("Loaded.esp", "races.ini"),
        f.read(&win("Loaded.esp", "races.ini")).unwrap().to_vec(),
    );
    let (needed, _) = RaceMenuMorphs::needed(&plugins, &got);
    assert!(
        needed.iter().all(|k| !k.contains("notloaded")),
        "{needed:?}"
    );
    assert!(needed.contains(&format!("{}/sliders/a.ini", folder("loaded.esp"))));

    let rm = RaceMenuMorphs::read(&plugins, &f);
    assert!(rm.notes.is_empty(), "{:?}", rm.notes);
    let s = rm.sliders_for("nordrace", true);
    assert_eq!(names(&s), ["S"]);
    assert_eq!(high_of(s[0]).as_deref(), Some("Mine"));
    assert_eq!(rm.races(), ["NordRace"]);
    assert!(rm.sliders_for("ElfRace", true).is_empty());
    assert_eq!(
        rm.extras_for("head/chargen.tri"),
        [format!("{MORPHS}/loaded/one.tri")]
    );

    // With no loaded plugin's folder there, nothing.
    let rm = RaceMenuMorphs::read(&["Skyrim.esm", "Update.esm"], &f);
    assert!(rm.is_empty() && rm.notes.is_empty());
    assert!(rm.extras_for("head/chargen.tri").is_empty());
}

fn two_folders() -> Files {
    let mut f = Files::new();
    f.insert(
        &win("A.esp", "morphs.ini"),
        b"extension = Head\\Chargen.tri, A\\One.tri, Both\\Same.tri\r\n".to_vec(),
    );
    f.insert(
        &win("A.esp", "races.ini"),
        b"NordRace = sliders\\a.ini\r\nImperialRace = sliders\\a.ini\r\n".to_vec(),
    );
    f.insert(
        &win("A.esp", "sliders\\a.ini"),
        b"[Female]\r\nShared = 1, Slider, None, FromA\r\nOnlyA = 1, Slider, None, X\r\n".to_vec(),
    );
    f.insert(
        &win("B.esl", "morphs.ini"),
        b"extension = head/chargen.TRI, B\\Two.tri, both/same.tri\r\n".to_vec(),
    );
    f.insert(
        &win("B.esl", "races.ini"),
        b"nordrace = sliders\\b.ini\r\n".to_vec(),
    );
    f.insert(
        &win("B.esl", "sliders\\b.ini"),
        b"[Female]\r\nSHARED = 1, Slider, None, FromB\r\nOnlyB = 1, Preset, P, 2\r\n".to_vec(),
    );
    f
}

#[test]
fn the_load_order_decides_which_of_two_folders_sliders_wins() {
    let f = two_folders();
    let rm = RaceMenuMorphs::read(&["A.esp", "B.esl"], &f);
    assert!(rm.notes.is_empty(), "{:?}", rm.notes);
    let s = rm.sliders_for("NordRace", true);
    assert_eq!(names(&s), ["Shared", "OnlyA", "OnlyB"]);
    assert_eq!(high_of(s[0]).as_deref(), Some("FromA"));
    assert_eq!(
        rm.extras_for("Head\\Chargen.tri"),
        [
            format!("{MORPHS}/a/one.tri"),
            format!("{MORPHS}/both/same.tri"),
            format!("{MORPHS}/b/two.tri"),
        ]
    );

    let rm = RaceMenuMorphs::read(&["B.esl", "A.esp"], &f);
    let s = rm.sliders_for("NordRace", true);
    assert_eq!(names(&s), ["SHARED", "OnlyB", "OnlyA"]);
    assert_eq!(high_of(s[0]).as_deref(), Some("FromB"));
    assert_eq!(
        rm.extras_for("Head\\Chargen.tri"),
        [
            format!("{MORPHS}/b/two.tri"),
            format!("{MORPHS}/both/same.tri"),
            format!("{MORPHS}/a/one.tri"),
        ]
    );
    // A race only one folder names takes that one's file; the other sex has
    // nothing.
    assert_eq!(
        names(&rm.sliders_for("IMPERIALRACE", true)),
        ["Shared", "OnlyA"]
    );
    assert!(rm.sliders_for("NordRace", false).is_empty());

    // And the winner is what `weights` draws.
    let rm = RaceMenuMorphs::read(&["B.esl", "A.esp"], &f);
    let (w, _) = weights(
        &custom(&[("shared", 1.0)]),
        &rm.sliders_for("NordRace", true),
    );
    assert_eq!(w, map(&[("fromb", 1.0)]));
}

#[test]
fn extras_are_found_by_the_chargen_tri_however_its_path_is_written() {
    let mut f = Files::new();
    f.insert(
        &win("Mod.esp", "morphs.ini"),
        b"extension = Actors\\Character\\Character Assets\\FemaleHeadChargen.tri, Mine\\Extra.tri\r\n\
          extension = Meshes\\Other\\Prefixed.tri, Mine\\Second.tri\r\n"
            .to_vec(),
    );
    let rm = RaceMenuMorphs::read(&["Mod.esp"], &f);
    assert!(rm.notes.is_empty(), "{:?}", rm.notes);
    let want = [format!("{MORPHS}/mine/extra.tri")];
    for q in [
        "Actors\\Character\\Character Assets\\FemaleHeadChargen.tri",
        "actors/character/character assets/femaleheadchargen.tri",
        "ACTORS\\CHARACTER\\CHARACTER ASSETS\\FEMALEHEADCHARGEN.TRI",
        "Meshes\\Actors\\Character\\Character Assets\\FemaleHeadChargen.tri",
        "meshes/actors/character/character assets/femaleheadchargen.tri",
        "MESHES/Actors\\Character/Character Assets\\FemaleHeadCharGen.tri",
        "\\Actors\\\\Character\\Character Assets\\FemaleHeadChargen.tri",
        "Data\\Meshes\\Actors\\Character\\Character Assets\\FemaleHeadChargen.tri",
    ] {
        assert_eq!(rm.extras_for(q), want, "{q}");
    }
    let second = [format!("{MORPHS}/mine/second.tri")];
    for q in ["Other\\Prefixed.tri", "meshes/other/prefixed.tri"] {
        assert_eq!(rm.extras_for(q), second, "{q}");
    }
    for q in [
        "Actors\\Character\\Character Assets\\FemaleHead.tri",
        "Actors\\Character\\FemaleHeadChargen.tri",
        "Character Assets\\FemaleHeadChargen.tri",
        "Actors\\Character\\Character Assets\\FemaleHeadChargen.tri.bak",
        "",
    ] {
        assert!(rm.extras_for(q).is_empty(), "{q}");
    }
}

/// A source that keeps every path it was asked for.
struct Logged<'a> {
    files: &'a Files,
    asked: RefCell<Vec<String>>,
}

impl Source for Logged<'_> {
    fn read(&self, key: &str) -> Option<&[u8]> {
        self.asked.borrow_mut().push(key.to_string());
        self.files.read(key)
    }
}

/// A path that stays in one loaded plugin's folder: no `..`, `.` or drive
/// part, in the folder of a plugin that is loaded.
fn assert_inside(k: &str, loaded: &[String]) {
    let rest = k
        .strip_prefix(&format!("{FOLDER}/"))
        .unwrap_or_else(|| panic!("{k} is not under the folders"));
    let parts: Vec<&str> = rest.split('/').collect();
    assert!(parts.len() >= 2, "{k}");
    assert!(loaded.contains(&parts[0].to_string()), "{k}");
    for p in parts {
        assert!(
            !p.is_empty() && p != "." && p != ".." && !p.contains(':') && !p.contains('\\'),
            "{k}"
        );
    }
}

/// Every file of every folder, as the game folder holds them.
fn hostile_disk() -> Files {
    let mut f = Files::new();
    f.insert(
        &win("Mod.esp", "races.ini"),
        b"R1 = ..\\..\\..\\..\\..\\Skyrim.ini\r\n\
          R2 = sliders\\..\\..\\..\\x.ini\r\n\
          R3 = C:\\Windows\\win.ini\r\n\
          R4 = D:sliders.ini\r\n\
          R5 = sliders\\.\\a.ini\r\n\
          R6 = sliders\\..\\sliders\\good.ini\r\n\
          R7 = Data\\..\\..\\x.ini\r\n\
          R8 = \\\\server\\share\\x.ini\r\n\
          R9 = /etc/passwd\r\n\
          R10 = sliders/good.ini\r\n\
          R11 = .\r\n\
          R12 = ..\r\n"
            .to_vec(),
    );
    f.insert(
        &win("Mod.esp", "morphs.ini"),
        b"extension = ..\\..\\x.tri, a.tri\r\nextension = h.tri, ..\\..\\..\\y.tri, C:\\z.tri, ok.tri\r\n"
            .to_vec(),
    );
    f.insert(
        &win("Mod.esp", "sliders\\good.ini"),
        b"[Male]\r\nGood = 1, Slider, None, G\r\n".to_vec(),
    );
    f.insert(
        &win("Other.esl", "races.ini"),
        b"OtherRace = sliders\\o.ini\r\n".to_vec(),
    );
    f.insert(
        &win("Other.esl", "sliders\\o.ini"),
        b"[Female]\r\nO = 1, Preset, P, 3\r\n".to_vec(),
    );
    // Files a path that escapes would reach, so a read of one would show.
    f.insert(
        "skyrim.ini",
        b"[Male]\r\nEscaped = 1, Slider, None, E\r\n".to_vec(),
    );
    f.insert(
        "meshes/actors/x.ini",
        b"[Male]\r\nEscaped = 1, Slider, None, E\r\n".to_vec(),
    );
    f
}

#[test]
fn asking_and_fetching_settles_within_a_few_rounds_and_stays_in_the_folders() {
    let disk = hostile_disk();
    let plugins = [
        "Skyrim.esm",
        "Mod.esp",
        "..",
        ".",
        "...",
        "",
        "../Mod.esp",
        "..\\Mod.esp",
        "C:Mod.esp",
        "Other.esl",
        "Mod.esp",
    ];
    let loaded: Vec<String> = ["skyrim.esm", "mod.esp", "other.esl"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut fetched = Files::new();
    let mut asked: Vec<String> = Vec::new();
    let mut rounds = 0;
    loop {
        let (needed, notes) = RaceMenuMorphs::needed(&plugins, &fetched);
        check_notes(&notes, "needed");
        for k in &needed {
            assert_inside(k, &loaded);
            assert!(!asked.contains(k), "{k} asked twice");
        }
        if needed.is_empty() {
            break;
        }
        rounds += 1;
        assert!(
            rounds <= 3,
            "still asking after {rounds} rounds: {needed:?}"
        );
        // What isn't there is given as nothing.
        for k in &needed {
            fetched.insert(k, disk.read(k).unwrap_or_default().to_vec());
        }
        asked.extend(needed);
    }
    assert!(asked.contains(&format!("{}/sliders/good.ini", folder("mod.esp"))));
    assert!(asked.contains(&format!("{}/sliders/o.ini", folder("other.esl"))));
    // Absolute paths are refused, not read.
    for k in &asked {
        assert!(!k.contains("windows") && !k.ends_with("/skyrim.ini"), "{k}");
    }

    // Reading touches nothing outside the folders either, even given every
    // file of the game folder.
    let logged = Logged {
        files: &disk,
        asked: RefCell::new(Vec::new()),
    };
    let rm = RaceMenuMorphs::read(&plugins, &logged);
    for k in logged.asked.borrow().iter() {
        assert_inside(k, &loaded);
    }
    let s = rm.sliders_for("R10", false);
    assert_eq!(names(&s), ["Good"]);
    assert_eq!(names(&rm.sliders_for("R6", false)), Vec::<String>::new());
    assert!(rm
        .races()
        .iter()
        .all(|r| ["R8", "R9", "R10", "OtherRace"].contains(r)));
    assert!(rm.sliders_for("R1", false).is_empty());
    assert_eq!(
        rm.extras_for("h.tri"),
        [format!("{MORPHS}/ok.tri")],
        "{:?}",
        rm.notes
    );
    assert!(!rm.notes.is_empty());
    check_notes(&rm.notes, "hostile");
}

/// Random folders: whatever the files say, the loop settles and every path
/// asked or read is inside a loaded plugin's folder.
#[test]
fn random_folders_settle_and_stay_inside() {
    const PATHS: &[&str] = &[
        "sliders\\a.ini",
        "..\\a.ini",
        "C:\\a.ini",
        "\\a.ini",
        "/a.ini",
        "sliders/../../b.ini",
        "a/./b.ini",
        "morphs.ini",
        "races.ini",
        "Data\\..\\c.ini",
        "x:y",
        "s\u{0}.ini",
    ];
    for seed in 0..200 {
        let mut r = Rng::new(seed);
        let plugins = ["A.esp", "B.esm", "C.esl"];
        let loaded: Vec<String> = plugins.iter().map(|p| p.to_ascii_lowercase()).collect();
        let mut disk = Files::new();
        for p in plugins {
            if r.one_in(3) {
                continue;
            }
            let mut races = String::new();
            for i in 0..r.upto(5) {
                let path = *r.pick(PATHS);
                races.push_str(&format!("Race{i} = {path}\r\n"));
                if !path.contains("..") {
                    disk.insert(&win(p, path), gen_sliders(&mut r).0.into_bytes());
                }
            }
            if r.one_in(3) {
                races.push_str(&String::from_utf8_lossy(&garbage(&mut r)));
            }
            disk.insert(&win(p, "races.ini"), races.into_bytes());
            disk.insert(&win(p, "morphs.ini"), gen_morphs(&mut r).0.into_bytes());
        }
        let mut fetched = Files::new();
        let mut asked: Vec<String> = Vec::new();
        let mut rounds = 0;
        loop {
            let (needed, _) = RaceMenuMorphs::needed(&plugins, &fetched);
            for k in &needed {
                assert_inside(k, &loaded);
            }
            if needed.is_empty() {
                break;
            }
            rounds += 1;
            assert!(rounds <= 3, "seed {seed}: {needed:?}");
            for k in &needed {
                fetched.insert(k, disk.read(k).unwrap_or_default().to_vec());
            }
            asked.extend(needed);
        }
        let logged = Logged {
            files: &fetched,
            asked: RefCell::new(Vec::new()),
        };
        let rm = RaceMenuMorphs::read(&plugins, &logged);
        for k in logged.asked.borrow().iter() {
            assert_inside(k, &loaded);
            // Everything read was asked for first.
            assert!(asked.contains(k), "seed {seed}: read {k} without asking");
        }
        check_notes(&rm.notes, &format!("seed {seed}"));
    }
}

// ---------------------------------------------------------------------------
// 4. Builds with meshes and tris made here.

const STREAM: u32 = 100;

/// A Special Edition NIF of a root node and one unskinned `BSTriShape` of
/// `positions`, made byte by byte.
fn shape_nif(name: &str, positions: &[[f32; 3]]) -> Vec<u8> {
    let av = |name: i32| AvObject {
        name,
        extra_data: Vec::new(),
        controller: -1,
        flags: 14,
        transform: Transform::IDENTITY,
        properties: None,
        collision: -1,
    };
    let root = Node {
        av: av(0),
        children: vec![1],
        effects: Some(Vec::new()),
        rest: Vec::new(),
    }
    .write(STREAM);
    let n = positions.len();
    let triangles: Vec<[u16; 3]> = (0..n.saturating_sub(2) as u16)
        .map(|i| [0, i + 1, i + 2])
        .collect();
    let mut s: Vec<u8> = Vec::new();
    s.extend(1i32.to_le_bytes()); // name
    s.extend(0u32.to_le_bytes()); // extra data
    s.extend((-1i32).to_le_bytes()); // controller
    s.extend(14u32.to_le_bytes()); // flags
    for v in [
        0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
    ] {
        s.extend(v.to_le_bytes()); // transform
    }
    s.extend((-1i32).to_le_bytes()); // collision
    for v in [0.0f32; 4] {
        s.extend(v.to_le_bytes()); // bounding sphere
    }
    for link in [-1i32; 3] {
        s.extend(link.to_le_bytes()); // skin, shader, alpha
    }
    // Positions only, full floats: 16 bytes a vertex.
    let desc: u64 = 4 | (1u64 << 44);
    s.extend(desc.to_le_bytes());
    s.extend((triangles.len() as u16).to_le_bytes());
    s.extend((n as u16).to_le_bytes());
    s.extend(((n * 16 + triangles.len() * 6) as u32).to_le_bytes());
    for p in positions {
        for v in [p[0], p[1], p[2], 0.0] {
            s.extend(v.to_le_bytes());
        }
    }
    for t in &triangles {
        for i in t {
            s.extend(i.to_le_bytes());
        }
    }
    s.extend(0u32.to_le_bytes()); // particle data
    Nif {
        header: Header {
            line: b"Gamebryo File Format, Version 20.2.0.7".to_vec(),
            version: VERSION_20_2_0_7,
            endian: 1,
            user_version: 12,
            stream: StreamHeader {
                version: STREAM,
                author: Vec::new(),
                unknown: None,
                process_script: Some(Vec::new()),
                export_script: Vec::new(),
                max_filepath: None,
            },
            block_types: vec![b"NiNode".to_vec(), b"BSTriShape".to_vec()],
            strings: vec![b"Root".to_vec(), name.as_bytes().to_vec()],
            max_string_len: name.len().max(4) as u32,
            groups: Vec::new(),
        },
        blocks: vec![
            Block {
                kind: 0,
                data: root,
            },
            Block { kind: 1, data: s },
        ],
        roots: vec![0],
        trailing: Vec::new(),
    }
    .write()
}

fn morph(name: &str, scale: f32, offsets: Vec<[i16; 3]>) -> HeadMorph {
    HeadMorph {
        name: name.into(),
        scale,
        offsets,
    }
}

/// A morph moving only the vertices given, of a tri of `n`.
fn sparse(name: &str, scale: f32, n: usize, at: &[(usize, [i16; 3])]) -> HeadMorph {
    let mut offsets = vec![[0i16; 3]; n];
    for (v, o) in at {
        offsets[*v] = *o;
    }
    morph(name, scale, offsets)
}

fn tri(n: usize, morphs: Vec<HeadMorph>) -> Vec<u8> {
    headtri::write(&HeadTri {
        vertices: vec![[0.0; 3]; n],
        triangles: Vec::new(),
        uvs: vec![[0.0; 2]; n],
        morphs,
    })
}

const FACE: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [1.0, 1.0, 0.0],
    [0.5, 0.5, 1.0],
    [2.0, 2.0, 2.0],
];
const HAIR: [[f32; 3]; 4] = [
    [0.0, 0.0, 10.0],
    [1.0, 0.0, 10.0],
    [0.0, 1.0, 10.0],
    [1.0, 1.0, 10.0],
];

const FACE_TRI: &str = "meshes/test/facechargen.tri";
const HAIR_TRI: &str = "meshes/test/hairchargen.tri";

fn extra_key(name: &str) -> String {
    format!("{MORPHS}/mine/{name}.tri")
}

fn form(id: u32) -> FormRef {
    FormRef::new("Test.esm", id)
}

/// A race whose women and men default to a face and a hair, each with a
/// chargen tri.
#[derive(Default)]
struct World {
    races: HashMap<FormRef, Race>,
    parts: HashMap<FormRef, HeadPart>,
}

impl Records for World {
    fn kind(&self, f: &FormRef) -> Option<[u8; 4]> {
        if self.races.contains_key(f) {
            Some(*b"RACE")
        } else if self.parts.contains_key(f) {
            Some(*b"HDPT")
        } else {
            None
        }
    }
    fn npc(&self, _: &FormRef) -> Option<Npc> {
        None
    }
    fn race(&self, f: &FormRef) -> Option<Race> {
        self.races.get(f).cloned()
    }
    fn head_part(&self, f: &FormRef) -> Option<HeadPart> {
        self.parts.get(f).cloned()
    }
    fn texture_set(&self, _: &FormRef) -> Option<TextureSet> {
        None
    }
    fn armor(&self, _: &FormRef) -> Option<Armor> {
        None
    }
    fn armor_addon(&self, _: &FormRef) -> Option<ArmorAddon> {
        None
    }
    fn colour(&self, _: &FormRef) -> Option<Colour> {
        None
    }
}

fn world() -> World {
    let mut w = World::default();
    let part = |name: &str, kind, model: &str, chargen: &str| HeadPart {
        editor_id: Some(name.into()),
        kind: Some(kind),
        model: Some(model.into()),
        files: vec![PartFile {
            kind: PartFileKind::ChargenMorph,
            path: chargen.into(),
        }],
        ..HeadPart::default()
    };
    w.parts.insert(
        form(1),
        part(
            "Face",
            HeadPartKind::Face,
            "Test\\Face.nif",
            "Test\\FaceChargen.tri",
        ),
    );
    w.parts.insert(
        form(2),
        part(
            "Hair",
            HeadPartKind::Hair,
            "Test\\Hair.nif",
            "TEST\\HairChargen.tri",
        ),
    );
    let mut race = Race {
        editor_id: Some("TestRace".into()),
        ..Race::default()
    };
    for sex in 0..2 {
        race.sexes[sex].head.parts = vec![(0, form(1)), (1, form(2))];
    }
    w.races.insert(form(0x100), race);
    w
}

/// RaceMenu's folder for `Sliders.esp`: the face's chargen tri extended
/// by two extras, and women's sliders for `TestRace`.
fn racemenu() -> RaceMenuMorphs {
    let mut f = Files::new();
    f.insert(
        &win("Sliders.esp", "morphs.ini"),
        b"extension = Test\\FaceChargen.tri, Mine\\Extra.tri, Mine\\Extra2.tri\r\n".to_vec(),
    );
    f.insert(
        &win("Sliders.esp", "races.ini"),
        b"TestRace = sliders\\test.ini\r\n".to_vec(),
    );
    f.insert(
        &win("Sliders.esp", "sliders\\test.ini"),
        b"[Female]\r\nWidth = 16, Slider, Narrow, Wide\r\nShape = 32, Preset, Shape, 3\r\n"
            .to_vec(),
    );
    let rm = RaceMenuMorphs::read(&["Skyrim.esm", "Sliders.esp"], &f);
    assert!(rm.notes.is_empty(), "{:?}", rm.notes);
    rm
}

fn look(sliders: &[(&str, f32)]) -> Look {
    Look {
        race: Some(form(0x100)),
        female: true,
        weight: 100.0,
        custom_morphs: custom(sliders),
        ..Look::default()
    }
}

/// The models and chargen tris; the extras are added per test.
fn base_files(face_tri: Vec<HeadMorph>, hair_tri: Vec<HeadMorph>) -> Files {
    let mut f = Files::new();
    f.insert("Meshes\\Test\\Face.nif", shape_nif("FaceShape", &FACE));
    f.insert("Meshes\\Test\\Hair.nif", shape_nif("HairShape", &HAIR));
    f.insert(FACE_TRI, tri(FACE.len(), face_tri));
    f.insert(HAIR_TRI, tri(HAIR.len(), hair_tri));
    f
}

const STILL: Options = Options {
    face_tint: None,
    keep_head_space: true,
};

fn positions(built: &Built, name: &str) -> Vec<[f32; 3]> {
    let s = built
        .shapes
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no shape {name} in {:?}", built.shapes));
    mesh::read(&built.nif, s.block).unwrap().positions
}

/// `base` with `by[v]` added to vertex `v`.
fn moved(base: &[[f32; 3]], by: &[(usize, [f32; 3])]) -> Vec<[f32; 3]> {
    let mut out = base.to_vec();
    for (v, d) in by {
        for (o, d) in out[*v].iter_mut().zip(d) {
            *o += d;
        }
    }
    out
}

#[test]
fn a_hand_made_nif_reads_back_as_made() {
    let nif = Nif::read(&shape_nif("FaceShape", &FACE)).unwrap();
    let shapes = mesh::shapes(&nif);
    assert_eq!(shapes.len(), 1);
    let m = mesh::read(&nif, shapes[0]).unwrap();
    assert_eq!(m.name, "FaceShape");
    assert_eq!(m.positions, FACE);
}

#[test]
fn a_named_slider_moves_exactly_its_morphs_vertices_by_weight_times_delta() {
    let w = world();
    let rm = racemenu();
    let n = FACE.len();
    // The face's chargen tri has none of the sliders' morphs; the hair's
    // has `Narrow`, which a part's own chargen tri takes too.
    let mut f = base_files(
        vec![sparse("Unrelated", 1.0, n, &[(0, [100, 100, 100])])],
        vec![sparse("NARROW", 0.5, HAIR.len(), &[(2, [2, 2, 2])])],
    );
    f.insert(
        &extra_key("extra"),
        tri(
            n,
            vec![
                sparse("Narrow", 0.25, n, &[(1, [4, -8, 2]), (4, [0, 12, 0])]),
                morph("Wide", 0.25, vec![[4, 4, 4]; n]),
            ],
        ),
    );
    f.insert(
        &extra_key("extra2"),
        tri(
            n,
            vec![
                sparse("Shape1", 1.0, n, &[(5, [9, 9, 9])]),
                sparse("Shape2", 0.5, n, &[(0, [8, 0, 0])]),
                sparse("Shape3", 1.0, n, &[(3, [9, 9, 9])]),
            ],
        ),
    );

    // Width at -0.5: Narrow at 0.5. Shape at 2: Shape2 at 1.
    let p = plan_with(&look(&[("Width", -0.5), ("Shape", 2.0)]), &w, Some(&rm)).unwrap();
    assert!(p.notes.is_empty(), "{:?}", p.notes);
    assert_eq!(p.custom, ["narrow", "shape2"]);
    let face = p.head.iter().find(|h| h.name == "Face").unwrap();
    assert_eq!(face.extras, [extra_key("extra"), extra_key("extra2")]);
    let built = build(&p, &f, &STILL).unwrap();
    assert!(built.missing.is_empty(), "{:?}", built.missing);
    assert!(built.notes.is_empty(), "{:?}", built.notes);
    assert_eq!(
        positions(&built, "Face"),
        moved(
            &FACE,
            &[
                // 0.25 × 0.5 × (4, -8, 2) and × (0, 12, 0).
                (1, [0.5, -1.0, 0.25]),
                (4, [0.0, 1.5, 0.0]),
                // 0.5 × 1 × (8, 0, 0).
                (0, [4.0, 0.0, 0.0]),
            ]
        )
    );
    // 0.5 × 0.5 × (2, 2, 2).
    assert_eq!(
        positions(&built, "Hair"),
        moved(&HAIR, &[(2, [0.5, 0.5, 0.5])])
    );

    // Width at 0.75: Wide at 0.75, every vertex by 0.25 × 0.75 × 4; the
    // hair has no `Wide`.
    let p = plan_with(&look(&[("Width", 0.75)]), &w, Some(&rm)).unwrap();
    let built = build(&p, &f, &STILL).unwrap();
    let all: Vec<(usize, [f32; 3])> = (0..n).map(|v| (v, [0.75; 3])).collect();
    assert_eq!(positions(&built, "Face"), moved(&FACE, &all));
    assert_eq!(positions(&built, "Hair"), HAIR);
}

#[test]
fn an_extra_of_another_vertex_count_leaves_the_part_unmoved_by_it_and_says_so() {
    let w = world();
    let rm = racemenu();
    let n = FACE.len();
    let mut f = base_files(Vec::new(), Vec::new());
    // One vertex short.
    f.insert(
        &extra_key("extra"),
        tri(n - 1, vec![morph("Narrow", 1.0, vec![[50, 50, 50]; n - 1])]),
    );
    f.insert(
        &extra_key("extra2"),
        tri(n, vec![sparse("Shape3", 0.5, n, &[(2, [0, 0, 4])])]),
    );

    let p = plan_with(&look(&[("Width", -1.0)]), &w, Some(&rm)).unwrap();
    let built = build(&p, &f, &STILL).unwrap();
    assert_eq!(positions(&built, "Face"), FACE);
    assert_eq!(positions(&built, "Hair"), HAIR);
    let bad = extra_key("extra");
    assert!(
        built
            .notes
            .iter()
            .any(|m| m.starts_with("Face: ") && m.contains(&bad) && m.contains("vertices")),
        "{:?}",
        built.notes
    );
    // Nothing else drew `narrow`, which is said too.
    assert!(
        built.notes.iter().any(|m| m.contains("narrow")),
        "{:?}",
        built.notes
    );
    assert_eq!(built.notes.len(), 2, "{:?}", built.notes);

    // The good extra after it still shapes the part.
    let p = plan_with(&look(&[("Width", -1.0), ("Shape", 3.0)]), &w, Some(&rm)).unwrap();
    let built = build(&p, &f, &STILL).unwrap();
    assert_eq!(
        positions(&built, "Face"),
        moved(&FACE, &[(2, [0.0, 0.0, 2.0])])
    );
    assert!(built.notes.iter().any(|m| m.contains(&bad)));
}

#[test]
fn without_named_sliders_the_build_is_byte_for_byte_the_plain_one() {
    let w = world();
    let rm = racemenu();
    let n = FACE.len();
    let mut f = base_files(vec![morph("NoseLong", 0.5, vec![[2, 0, 0]; n])], Vec::new());
    f.insert(
        &extra_key("extra"),
        tri(n, vec![morph("Narrow", 1.0, vec![[3, 3, 3]; n])]),
    );
    f.insert(
        &extra_key("extra2"),
        tri(n, vec![morph("Shape1", 1.0, vec![[3, 3, 3]; n])]),
    );
    // A game slider is set, so the chargen tri moves the face either way.
    let mut l = look(&[]);
    l.face_morphs = vec![0.5];

    let plain = plan(&l, &w).unwrap();
    let with = plan_with(&l, &w, Some(&rm)).unwrap();
    assert_eq!(with, plain);
    let a = build(&plain, &f, &STILL).unwrap();
    let b = build(&with, &f, &STILL).unwrap();
    assert_eq!(a.nif.write(), b.nif.write());
    assert_eq!((a.notes, a.missing), (b.notes, b.missing));

    // Sliders all at nought turn nothing on: the same again.
    let mut z = l.clone();
    z.custom_morphs = custom(&[("Width", 0.0), ("Shape", 0.0)]);
    let zero = plan_with(&z, &w, Some(&rm)).unwrap();
    assert_eq!(zero, plain);
    assert_eq!(build(&zero, &f, &STILL).unwrap().nif.write(), b.nif.write());

    // And the comparison can fail: a slider set changes the bytes.
    let mut s = l.clone();
    s.custom_morphs = custom(&[("Width", -0.5)]);
    let set = plan_with(&s, &w, Some(&rm)).unwrap();
    assert_ne!(build(&set, &f, &STILL).unwrap().nif.write(), b.nif.write());
}

#[test]
fn a_morph_in_the_chargen_tri_and_an_extra_is_applied_once() {
    let w = world();
    let rm = racemenu();
    let n = FACE.len();
    // The chargen tri has `Narrow`: neither extra's (in any case) counts.
    let mut f = base_files(
        vec![sparse("Narrow", 0.5, n, &[(2, [0, 0, 8])])],
        Vec::new(),
    );
    f.insert(
        &extra_key("extra"),
        tri(n, vec![morph("NARROW", 1.0, vec![[100, 100, 100]; n])]),
    );
    f.insert(
        &extra_key("extra2"),
        tri(n, vec![morph("narrow", 1.0, vec![[-100, 0, 0]; n])]),
    );
    let p = plan_with(&look(&[("Width", -0.5)]), &w, Some(&rm)).unwrap();
    let built = build(&p, &f, &STILL).unwrap();
    assert!(built.notes.is_empty(), "{:?}", built.notes);
    // 0.5 × 0.5 × (0, 0, 8).
    assert_eq!(
        positions(&built, "Face"),
        moved(&FACE, &[(2, [0.0, 0.0, 2.0])])
    );

    // Not in the chargen tri: the first extra's, and of two in it, the
    // first.
    let mut f = base_files(Vec::new(), Vec::new());
    f.insert(
        &extra_key("extra"),
        tri(
            n,
            vec![
                sparse("Narrow", 1.0, n, &[(3, [2, 0, 0])]),
                sparse("NARROW", 1.0, n, &[(3, [0, 50, 0])]),
            ],
        ),
    );
    f.insert(
        &extra_key("extra2"),
        tri(n, vec![sparse("Narrow", 1.0, n, &[(3, [0, 0, 50])])]),
    );
    let p = plan_with(&look(&[("Width", -0.5)]), &w, Some(&rm)).unwrap();
    let built = build(&p, &f, &STILL).unwrap();
    assert!(built.notes.is_empty(), "{:?}", built.notes);
    assert_eq!(
        positions(&built, "Face"),
        moved(&FACE, &[(3, [1.0, 0.0, 0.0])])
    );
}
