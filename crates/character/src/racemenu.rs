//! RaceMenu's named sliders: which morphs each turns on, and the extra tris
//! that hold them.
//!
//! Worked out from the data files mods ship for RaceMenu, under
//! `Meshes\Actors\Character\FaceGenMorphs\<plugin file name>\`, one folder
//! per plugin (`UBE_AllRace.esp\`, `Expressive Facegen Morphs.esl\`), and
//! from the sample preset. Not derived from RaceMenu's scripts, library or
//! source.
//!
//! - **`morphs.ini`**: lines `extension = <source tri>, <extra tri>, …`.
//!   The source is relative to `Meshes\` and is a head part's chargen tri
//!   (`!UBE\Head\FemaleHeadCharGen.tri`); the extras are relative to
//!   `FaceGenMorphs\morphs\` (`John\LIP_FemaleHead.tri`), as the file's own
//!   header comment says. UBE's lists `EYES_EyeFemaleLashes.tri` twice in one
//!   line; an extra is taken once.
//! - **`races.ini`**: `<race editor id> = <slider file>`, the file relative
//!   to the plugin's folder (`00UBE_NordRace = sliders\JohnWick.ini`).
//! - **Slider files**: `[Male]` and `[Female]` sections of lines
//!   `<name> = <category>, <type>, <data>…`, padded with spaces and tabs.
//!   Names may hold spaces (`AN_Alinah Rin`). The types:
//!   - `Slider, <low morph>, <high morph>`, either `None`.
//!   - `Preset, <morph prefix>, <count>`.
//!   - `HeadPart, <n>`.
//! - `#` starts a comment, in each file. Lines end in `\r\n` in every file
//!   seen.
//!
//! A preset's `morphs.custom` keeps each slider by name with a signed value.
//! 15 of the sample's 59 are negative, such as `AF_FACE_Width` at -0.71,
//! whose morphs are `FACE_WidthIn` and `FACE_WidthOut`. So here, as
//! [`weights`] does it:
//!
//! - a `Slider` below nought turns on its low morph at minus the value, and
//!   above nought its high morph at the value. The slider files' header
//!   comment writes the data as "UpperBound,LowerBound", but its own example
//!   (`EarSize=8, Slider, EarSmall, EarLarge`), every installed line
//!   (`FACE_WidthIn, FACE_WidthOut`, `EFM_Nose_Short, EFM_Nose_Long`) and
//!   the sample's signs read the first as the low end;
//! - a `Preset` at *n* turns on `<prefix><n>` at 1, as the game's own
//!   presets turn on `NoseType<n>` ([`crate::morph`]). The header comment
//!   says there is no `Type0`, "internally assumed as no preset", so 0 is
//!   none and 1 to the count are the choices;
//! - a `HeadPart` slider picks a head part, which this does not draw.
//!
//! None of it is checked in game yet.
//!
//! Where a name is given twice (UBE's `JohnWick.ini` gives `EXPR2_Sad` twice
//! in each section), the first is the name's, as with two morphs of one name
//! in a tri. Slider names compare without case, as morph names do.
//!
//! Morphs from two sliders, or from a slider and a preset, on one name are
//! added, so their order changes nothing.
//!
//! The library reads no files: [`RaceMenuMorphs::needed`] says which to
//! fetch, and [`RaceMenuMorphs::read`] is given them. A path or plugin name
//! that could leave its folder is refused and noted, and every list is
//! held to a most.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Mutex;

use crate::files::{key, under, Source};

/// Where the plugins' folders are, as a [`key`].
pub const FOLDER: &str = "meshes/actors/character/facegenmorphs";
/// Where the extra tris are, as a [`key`].
pub const MORPHS: &str = "meshes/actors/character/facegenmorphs/morphs";

/// The most bytes of one ini file read: UBE's slider file is 60 KiB.
pub const MAX_FILE: usize = 4 << 20;
/// The most lines of one file looked at.
pub const MAX_LINES: usize = 65_536;
/// The most entries kept from one file.
pub const MAX_ENTRIES: usize = 16_384;
/// The most sliders kept from all the folders together: UBE's file has 424
/// a sex.
pub const MAX_SLIDERS: usize = 16_384;
/// The most extras one source tri is given.
pub const MAX_EXTRAS: usize = 256;
/// The longest name or path kept, in bytes.
pub const MAX_TEXT: usize = 1024;
/// The most plugin folders looked in.
pub const MAX_FOLDERS: usize = 4096;
/// The most notes kept; past it, one more says how many were left out.
pub const MAX_NOTES: usize = 200;
/// The most named sliders of a look turned into weights.
pub const MAX_CUSTOM: usize = 4096;

/// What one slider does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SliderKind {
    /// Turns on `low` below nought and `high` above it.
    Slider {
        low: Option<String>,
        high: Option<String>,
    },
    /// Turns on `<prefix><n>` at *n*, from 1 to `count`.
    Preset { prefix: String, count: u32 },
    /// Picks a head part; not drawn.
    HeadPart { kind: u32 },
}

impl SliderKind {
    /// The morph a `Slider` at `value` turns on, if any, and how strongly:
    /// below nought its low morph at minus the value, above nought its high
    /// morph at the value. The one place that sign rule is kept (see the
    /// module); not checked in game. `None` for any other kind, or at nought.
    pub fn morph_at(&self, value: f32) -> Option<(Option<&str>, f32)> {
        let SliderKind::Slider { low, high } = self else {
            return None;
        };
        if value == 0.0 || !value.is_finite() {
            return None;
        }
        Some(if value < 0.0 {
            (low.as_deref(), -value)
        } else {
            (high.as_deref(), value)
        })
    }
}

/// One slider of a slider file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliderDef {
    pub name: String,
    /// The category flag, the menu it is shown in (8 head, 16 face…).
    pub category: i32,
    pub kind: SliderKind,
}

/// A slider file's two sections.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SliderFile {
    pub male: Vec<SliderDef>,
    pub female: Vec<SliderDef>,
}

/// One plugin's folder, read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Folder {
    /// The source tri, as a [`key`] under `meshes`, and its extras, as keys.
    extensions: Vec<(String, Vec<String>)>,
    /// A race's editor id and its slider file, by its place in
    /// [`Folder::sliders`].
    races: Vec<(String, usize)>,
    /// Each slider file once, by key.
    sliders: Vec<(String, SliderFile)>,
}

/// Where a slider is: its folder, its file there, its sex and its place.
type At = (usize, usize, bool, usize);

/// [`RaceMenuMorphs::sliders_for`]'s answers, worked out once for each race
/// (lower case) and sex. Worked out from the folders, so two sets of the
/// same folders are equal whatever each has worked out.
#[derive(Debug, Default)]
struct Cache(Mutex<HashMap<(String, bool), Vec<At>>>);

impl Clone for Cache {
    fn clone(&self) -> Self {
        Cache(Mutex::new(
            self.0.lock().map(|c| c.clone()).unwrap_or_default(),
        ))
    }
}

impl PartialEq for Cache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Cache {}

/// The sliders and extra tris of the loaded plugins' folders.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RaceMenuMorphs {
    folders: Vec<Folder>,
    /// What could not be read, each a sentence.
    pub notes: Vec<String>,
    cache: Cache,
}

/// Notes, held to [`MAX_NOTES`].
#[derive(Default)]
struct Notes {
    kept: Vec<String>,
    more: usize,
}

impl Notes {
    /// Another list's notes, its count of those left out added to this
    /// one's.
    fn merge(&mut self, other: Notes) {
        for n in other.kept {
            self.push(|| n);
        }
        self.more += other.more;
    }

    fn push(&mut self, n: impl FnOnce() -> String) {
        if self.kept.len() < MAX_NOTES {
            self.kept.push(n());
        } else {
            self.more += 1;
        }
    }

    fn done(mut self) -> Vec<String> {
        if self.more > 0 {
            self.kept.push(format!("And {} more.", self.more));
        }
        self.kept
    }
}

/// A file's lines, with comments and padding taken off and empty ones left
/// out, numbered from 1.
fn lines(file: &[u8], what: &str, notes: &mut Notes) -> Vec<(usize, String)> {
    let file = if file.len() > MAX_FILE {
        notes.push(|| format!("{what} is longer than {MAX_FILE} bytes; the rest is not read."));
        &file[..MAX_FILE]
    } else {
        file
    };
    let text = String::from_utf8_lossy(file);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let mut out = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if i == MAX_LINES {
            notes.push(|| format!("{what} has more than {MAX_LINES} lines; the rest is not read."));
            break;
        }
        let line = line.split('#').next().unwrap_or_default().trim();
        if !line.is_empty() {
            out.push((i + 1, line.to_string()));
        }
    }
    out
}

/// Whether one part of a path or a file's name stands for no file: only
/// dots and spaces, which Windows takes off a name's end, so `. .` and
/// `...` are `.` and `..` to it.
fn no_name(part: &str) -> bool {
    part.trim_matches([' ', '.']).is_empty()
}

/// A path a file gives, as a [`key`], if it stays where it is put: not
/// absolute (no leading slash, no drive), no part that is `.` or `..` once
/// Windows has taken spaces and dots off it, no control characters, nothing
/// past [`MAX_TEXT`].
fn safe_path(path: &str) -> Option<String> {
    if path.starts_with(['/', '\\']) || path.chars().any(char::is_control) {
        return None;
    }
    let k = key(path);
    let ok = !k.is_empty()
        && k.len() <= MAX_TEXT
        && !k.contains(':')
        && path
            .split(['/', '\\'])
            .filter(|p| !p.is_empty())
            .all(|p| !no_name(p));
    ok.then_some(k)
}

/// A plugin's folder, as a [`key`], if its name is one plain file's name:
/// no slashes, drive, control characters or characters Windows keeps out
/// of names, and not only dots and spaces.
fn folder_of(plugin: &str) -> Option<String> {
    let ok = !plugin.is_empty()
        && plugin.len() <= MAX_TEXT
        && !plugin.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        && !plugin.chars().any(char::is_control)
        && !no_name(plugin);
    ok.then(|| format!("{FOLDER}/{}", plugin.to_ascii_lowercase()))
}

fn text_ok(t: &str) -> bool {
    !t.is_empty() && t.len() <= MAX_TEXT
}

type Extensions = Vec<(String, Vec<String>)>;

fn morphs_in(file: &[u8], what: &str) -> (Extensions, Notes) {
    let mut notes = Notes::default();
    let mut out: Extensions = Vec::new();
    // Each source's place in `out`, and each source's extras, to find them
    // in one look.
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut have: Vec<HashSet<String>> = Vec::new();
    for (n, line) in lines(file, what, &mut notes) {
        let Some((k, v)) = line.split_once('=') else {
            notes.push(|| format!("{what} line {n} is not `name = value`."));
            continue;
        };
        if !k.trim().eq_ignore_ascii_case("extension") {
            notes.push(|| format!("{what} line {n} is not an `extension`."));
            continue;
        }
        let mut parts = v.split(',').map(str::trim).filter(|p| !p.is_empty());
        let Some(source) = parts.next().and_then(safe_path) else {
            notes.push(|| format!("{what} line {n} names no source tri it can use."));
            continue;
        };
        let source = under("meshes", &source);
        let i = match at.get(&source) {
            Some(i) => *i,
            None => {
                if out.len() == MAX_ENTRIES {
                    notes.push(|| format!("{what} has more than {MAX_ENTRIES} entries."));
                    break;
                }
                at.insert(source.clone(), out.len());
                out.push((source, Vec::new()));
                have.push(HashSet::new());
                out.len() - 1
            }
        };
        let mut given = 0usize;
        let mut dropped = 0usize;
        for p in parts {
            given += 1;
            match safe_path(p) {
                Some(k) => {
                    let k = format!("{MORPHS}/{k}");
                    if have[i].contains(&k) {
                        continue;
                    }
                    if out[i].1.len() == MAX_EXTRAS {
                        dropped += 1;
                        continue;
                    }
                    have[i].insert(k.clone());
                    out[i].1.push(k);
                }
                None => notes.push(|| format!("{what} line {n}: an extra it can't use.")),
            }
        }
        if given == 0 {
            notes.push(|| format!("{what} line {n} names a source tri but no extras."));
        }
        if dropped > 0 {
            notes.push(|| {
                format!("{what} line {n}: {dropped} extras past the most a tri has ({MAX_EXTRAS}) are left out.")
            });
        }
    }
    // A source no line gave an extra it could use.
    out.retain(|(_, e)| !e.is_empty());
    (out, notes)
}

/// The `extension` lines of a `morphs.ini`: each source tri as a [`key`]
/// under `meshes`, with its extras as keys, each once. A source given on
/// two lines keeps both lines' extras.
pub fn parse_morphs(file: &[u8], what: &str) -> (Vec<(String, Vec<String>)>, Vec<String>) {
    let (out, notes) = morphs_in(file, what);
    (out, notes.done())
}

fn races_in(file: &[u8], what: &str) -> (Vec<(String, String)>, Notes) {
    let mut notes = Notes::default();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (n, line) in lines(file, what, &mut notes) {
        let Some((race, path)) = line.split_once('=') else {
            notes.push(|| format!("{what} line {n} is not `race = file`."));
            continue;
        };
        let (race, path) = (race.trim(), path.trim());
        if !text_ok(race) || !text_ok(path) {
            notes.push(|| format!("{what} line {n} is not `race = file`."));
            continue;
        }
        if seen.contains(&race.to_ascii_lowercase()) {
            notes.push(|| format!("{what} line {n} gives {race} again; the first is kept."));
            continue;
        }
        if out.len() == MAX_ENTRIES {
            notes.push(|| format!("{what} has more than {MAX_ENTRIES} entries."));
            break;
        }
        seen.insert(race.to_ascii_lowercase());
        out.push((race.to_string(), path.to_string()));
    }
    (out, notes)
}

/// The lines of a `races.ini`: each race's editor id and its slider file,
/// as written (relative to the plugin's folder, checked by
/// [`RaceMenuMorphs`]). A race given twice keeps its first.
pub fn parse_races(file: &[u8], what: &str) -> (Vec<(String, String)>, Vec<String>) {
    let (out, notes) = races_in(file, what);
    (out, notes.done())
}

/// A slider line's data, after the name.
fn slider_def(name: &str, data: &str) -> Option<SliderDef> {
    let f: Vec<&str> = data.split(',').map(str::trim).collect();
    let category = f.first()?.parse::<i32>().ok()?;
    let morph = |s: &str| {
        if s.eq_ignore_ascii_case("none") {
            Some(None)
        } else if text_ok(s) {
            Some(Some(s.to_string()))
        } else {
            None
        }
    };
    let kind = match f.get(1)?.to_ascii_lowercase().as_str() {
        "slider" if f.len() == 4 => SliderKind::Slider {
            low: morph(f[2])?,
            high: morph(f[3])?,
        },
        "preset" if f.len() == 4 && text_ok(f[2]) => SliderKind::Preset {
            prefix: f[2].to_string(),
            count: f[3].parse().ok()?,
        },
        "headpart" if f.len() == 3 => SliderKind::HeadPart {
            kind: f[2].parse().ok()?,
        },
        _ => return None,
    };
    Some(SliderDef {
        name: name.to_string(),
        category,
        kind,
    })
}

fn sliders_in(file: &[u8], what: &str) -> (SliderFile, Notes) {
    let mut notes = Notes::default();
    let mut out = SliderFile::default();
    // The names each section has, in lower case.
    let mut seen: [HashSet<String>; 2] = Default::default();
    // Which section the lines are in: male, female, or one this does not
    // know (`None`), or none yet.
    let mut section: Option<Option<bool>> = None;
    let mut count = 0usize;
    for (n, line) in lines(file, what, &mut notes) {
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let s = s.trim();
            section = Some(if s.eq_ignore_ascii_case("male") {
                Some(false)
            } else if s.eq_ignore_ascii_case("female") {
                Some(true)
            } else {
                notes.push(|| format!("{what} line {n}: a section [{s}] this does not know."));
                None
            });
            continue;
        }
        let Some(female) = section else {
            notes.push(|| format!("{what} line {n} is before any section."));
            continue;
        };
        let Some(female) = female else { continue };
        let Some((name, data)) = line.split_once('=') else {
            notes.push(|| format!("{what} line {n} is not `name = category, type, data`."));
            continue;
        };
        let name = name.trim();
        if !text_ok(name) {
            notes.push(|| format!("{what} line {n} has no name it can use."));
            continue;
        }
        let Some(def) = slider_def(name, data) else {
            notes.push(|| format!("{what} line {n} ({name}) is a slider this does not read."));
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if seen[female as usize].contains(&lower) {
            notes.push(|| format!("{what} line {n} gives {name} again; the first is kept."));
            continue;
        }
        if count == MAX_ENTRIES {
            notes.push(|| format!("{what} has more than {MAX_ENTRIES} sliders."));
            break;
        }
        count += 1;
        seen[female as usize].insert(lower);
        if female {
            out.female.push(def);
        } else {
            out.male.push(def);
        }
    }
    (out, notes)
}

/// A slider file. A name given twice in a section keeps its first.
pub fn parse_sliders(file: &[u8], what: &str) -> (SliderFile, Vec<String>) {
    let (out, notes) = sliders_in(file, what);
    (out, notes.done())
}

impl RaceMenuMorphs {
    /// The files still to fetch for `plugins` (the loaded plugins' file
    /// names, in load order), by [`key`], and what was passed over, each a
    /// sentence: each folder's `morphs.ini` and `races.ini`, then the slider
    /// files the `races.ini` files in `files` name, but none `files`
    /// already has. Most plugins have no folder: a host gives a file that
    /// is not there as no bytes (an empty file reads as nothing), so that
    /// asking, fetching and asking again ends with nothing asked.
    pub fn needed(plugins: &[&str], files: &dyn Source) -> (Vec<String>, Vec<String>) {
        let mut notes = Notes::default();
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut want = |k: String, out: &mut Vec<String>| {
            if files.read(&k).is_none() && seen.insert(k.clone()) {
                out.push(k);
            }
        };
        if plugins.len() > MAX_FOLDERS {
            notes.push(|| format!("More than {MAX_FOLDERS} plugins; the rest are not looked at."));
        }
        for p in plugins.iter().take(MAX_FOLDERS) {
            let Some(folder) = folder_of(p) else {
                notes.push(|| format!("{p:?} is not a name a folder can have."));
                continue;
            };
            want(format!("{folder}/morphs.ini"), &mut out);
            let races = format!("{folder}/races.ini");
            want(races.clone(), &mut out);
            if let Some(bytes) = files.read(&races) {
                // Most races share a file: each path is looked at once.
                let mut paths: HashSet<String> = HashSet::new();
                for (race, path) in races_in(bytes, &races).0 {
                    if !paths.insert(path.clone()) {
                        continue;
                    }
                    match safe_path(&path) {
                        Some(k) => want(format!("{folder}/{k}"), &mut out),
                        None => notes.push(|| {
                            format!("{races}: {race}'s slider file is a path it can't use.")
                        }),
                    }
                }
            }
        }
        (out, notes.done())
    }

    /// Read the folders of `plugins` from `files` (as [`Self::needed`]
    /// asked for them). A folder or file that is not there is passed over;
    /// what is there and can't be read is noted.
    pub fn read(plugins: &[&str], files: &dyn Source) -> RaceMenuMorphs {
        let mut notes = Notes::default();
        let mut out = RaceMenuMorphs::default();
        let mut total = 0usize;
        if plugins.len() > MAX_FOLDERS {
            notes.push(|| format!("More than {MAX_FOLDERS} plugins; the rest are not looked at."));
        }
        for p in plugins.iter().take(MAX_FOLDERS) {
            let Some(folder) = folder_of(p) else {
                notes.push(|| format!("{p:?} is not a name a folder can have."));
                continue;
            };
            let mut f = Folder::default();
            let morphs = format!("{folder}/morphs.ini");
            if let Some(bytes) = files.read(&morphs) {
                let (e, n) = morphs_in(bytes, &morphs);
                f.extensions = e;
                notes.merge(n);
            }
            let races = format!("{folder}/races.ini");
            if let Some(bytes) = files.read(&races) {
                let (r, n) = races_in(bytes, &races);
                notes.merge(n);
                let mut file_at: HashMap<String, usize> = HashMap::new();
                // Each path as written, once worked out.
                let mut by_path: HashMap<String, usize> = HashMap::new();
                for (race, path) in r {
                    if let Some(at) = by_path.get(&path) {
                        f.races.push((race, *at));
                        continue;
                    }
                    let Some(k) = safe_path(&path) else {
                        notes.push(|| {
                            format!("{races}: {race}'s slider file is a path it can't use.")
                        });
                        continue;
                    };
                    let k = format!("{folder}/{k}");
                    let at = match file_at.get(&k) {
                        Some(at) => *at,
                        None => {
                            let Some(bytes) = files.read(&k) else {
                                notes.push(|| format!("{races} names {k}, which is not there."));
                                continue;
                            };
                            let (mut s, n) = sliders_in(bytes, &k);
                            notes.merge(n);
                            let room = MAX_SLIDERS - total;
                            let have = s.male.len() + s.female.len();
                            if have > room {
                                notes.push(|| {
                                    format!("{k}: past the most sliders kept from all folders ({MAX_SLIDERS}), {} are left out.", have - room)
                                });
                                let male = s.male.len().min(room);
                                s.male.truncate(male);
                                s.female.truncate(room - male);
                            }
                            total += s.male.len() + s.female.len();
                            file_at.insert(k.clone(), f.sliders.len());
                            f.sliders.push((k, s));
                            f.sliders.len() - 1
                        }
                    };
                    by_path.insert(path, at);
                    f.races.push((race, at));
                }
            }
            if f != Folder::default() {
                out.folders.push(f);
            }
        }
        out.notes = notes.done();
        out
    }

    /// The sliders a race (by editor id) and sex has, from every folder that
    /// names the race, in load order; a name already given by an earlier
    /// folder keeps that one. Worked out once for each race and sex.
    pub fn sliders_for(&self, race_editor_id: &str, female: bool) -> Vec<&SliderDef> {
        let k = (race_editor_id.to_ascii_lowercase(), female);
        let cached = self.cache.0.lock().ok().and_then(|c| c.get(&k).cloned());
        let at = match cached {
            Some(at) => at,
            None => {
                let at = self.work_out(&k.0, female);
                if let Ok(mut c) = self.cache.0.lock() {
                    c.insert(k, at.clone());
                }
                at
            }
        };
        at.into_iter()
            .map(|(f, s, female, i)| {
                let file = &self.folders[f].sliders[s].1;
                if female {
                    &file.female[i]
                } else {
                    &file.male[i]
                }
            })
            .collect()
    }

    fn work_out(&self, race: &str, female: bool) -> Vec<At> {
        let mut out: Vec<At> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (fi, f) in self.folders.iter().enumerate() {
            let Some((_, s)) = f.races.iter().find(|(r, _)| r.eq_ignore_ascii_case(race)) else {
                continue;
            };
            let file = &f.sliders[*s].1;
            let list = if female { &file.female } else { &file.male };
            for (i, d) in list.iter().enumerate() {
                if seen.insert(d.name.to_ascii_lowercase()) {
                    out.push((fi, *s, female, i));
                }
            }
        }
        out
    }

    /// The extra tris for a head part's chargen tri, by [`key`], each once,
    /// in load order. `source` is as a record gives it or as a key, with or
    /// without `meshes`; case and slashes do not matter.
    pub fn extras_for(&self, source: &str) -> Vec<String> {
        let source = under("meshes", source);
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for f in &self.folders {
            for (s, extras) in &f.extensions {
                if *s == source {
                    for e in extras {
                        if seen.insert(e) {
                            out.push(e.clone());
                        }
                    }
                }
            }
        }
        out
    }

    /// The editor ids of the races some folder gives sliders, each once.
    pub fn races(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for f in &self.folders {
            for (r, _) in &f.races {
                if seen.insert(r.to_ascii_lowercase()) {
                    out.push(r);
                }
            }
        }
        out
    }

    /// Whether no folder gave anything.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }
}

/// Why a named slider of a look shapes nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum SliderNote {
    /// The race has no slider files, so none of the look's are drawn.
    NoSliders { race: Option<String> },
    /// No slider of this name for the race and sex.
    Unknown { name: String },
    /// Given twice in the look; the first is used.
    Repeated { name: String },
    /// Its value is not a number.
    NotANumber { name: String },
    /// A slider with no morph on the side its value is.
    NoMorphThatWay { name: String, value: f32 },
    /// A preset at a value that is not one of its choices.
    NotAChoice {
        name: String,
        value: f32,
        count: u32,
    },
    /// It picks a head part, which is not drawn from it.
    HeadPart { name: String, value: f32 },
    /// More than [`MAX_CUSTOM`] sliders; the rest are not looked at.
    TooMany(usize),
}

impl fmt::Display for SliderNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SliderNote::NoSliders { race: Some(r) } => {
                write!(f, "No RaceMenu slider file is for {r}, so its named sliders aren't drawn.")
            }
            SliderNote::NoSliders { race: None } => {
                write!(f, "The race has no editor id, so its named sliders aren't drawn.")
            }
            SliderNote::Unknown { name } => {
                write!(f, "The slider {name} isn't one the race has, so it isn't drawn.")
            }
            SliderNote::Repeated { name } => {
                write!(f, "The slider {name} is given twice; the first is drawn.")
            }
            SliderNote::NotANumber { name } => {
                write!(f, "The slider {name} isn't a number, so it isn't drawn.")
            }
            SliderNote::NoMorphThatWay { name, value } => {
                write!(f, "The slider {name} has no morph for {value}, so it isn't drawn.")
            }
            SliderNote::NotAChoice { name, value, count } => write!(
                f,
                "The slider {name} is at {value}, not one of its choices 0 to {count}, so it isn't drawn."
            ),
            SliderNote::HeadPart { name, value } => write!(
                f,
                "The slider {name} ({value}) picks a head part, which the preview doesn't draw from it."
            ),
            SliderNote::TooMany(n) => write!(f, "{n} more named sliders weren't looked at."),
        }
    }
}

/// How far from a whole number a preset's value may be.
const WHOLE: f32 = 1e-3;

/// A look's named sliders (`custom`, as [`crate::Look::custom_morphs`])
/// as chargen morph weights, names in lower case, by the race's `sliders`
/// ([`RaceMenuMorphs::sliders_for`]); and why any of them shapes nothing.
/// A slider at nought shapes nothing and says nothing.
pub fn weights(
    custom: &[(String, f32)],
    sliders: &[&SliderDef],
) -> (HashMap<String, f32>, Vec<SliderNote>) {
    let mut out: HashMap<String, f32> = HashMap::new();
    let mut notes: Vec<SliderNote> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // The first of each name, as the list keeps it.
    let mut by_name: HashMap<String, &SliderDef> = HashMap::new();
    for d in sliders {
        by_name.entry(d.name.to_ascii_lowercase()).or_insert(d);
    }
    if custom.len() > MAX_CUSTOM {
        notes.push(SliderNote::TooMany(custom.len() - MAX_CUSTOM));
    }
    for (name, v) in custom.iter().take(MAX_CUSTOM) {
        let lower = name.to_ascii_lowercase();
        if !seen.insert(lower.clone()) {
            notes.push(SliderNote::Repeated { name: name.clone() });
            continue;
        }
        let Some(def) = by_name.get(&lower) else {
            notes.push(SliderNote::Unknown { name: name.clone() });
            continue;
        };
        if !v.is_finite() {
            notes.push(SliderNote::NotANumber { name: name.clone() });
            continue;
        }
        if *v == 0.0 {
            continue;
        }
        match &def.kind {
            SliderKind::Slider { .. } => match def.kind.morph_at(*v) {
                Some((Some(m), amount)) => {
                    *out.entry(m.to_ascii_lowercase()).or_default() += amount
                }
                _ => notes.push(SliderNote::NoMorphThatWay {
                    name: name.clone(),
                    value: *v,
                }),
            },
            SliderKind::Preset { prefix, count } => {
                let n = v.round();
                if (v - n).abs() > WHOLE || n < 0.0 || n > *count as f32 {
                    notes.push(SliderNote::NotAChoice {
                        name: name.clone(),
                        value: *v,
                        count: *count,
                    });
                } else if n >= 1.0 {
                    // Added, as a slider's value is, so the order sliders
                    // come in changes nothing.
                    *out.entry(format!("{prefix}{}", n as u32).to_ascii_lowercase())
                        .or_default() += 1.0;
                }
            }
            SliderKind::HeadPart { .. } => notes.push(SliderNote::HeadPart {
                name: name.clone(),
                value: *v,
            }),
        }
    }
    (out, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Files;

    const SLIDERS: &str = "# a header\r\n\
        stray = 1, Slider, A, B\r\n\
        [Male]\r\n\
        M_Only   =  8, Slider, None, MaleMorph\r\n\
        [Female]\r\n\
        UBE_Makeup        =  8, HeadPart, 69\r\n\
        AB_LIP_Width             = 128, Slider, LIP_Narrow,   LIP_Wide \r\n\
        DQ_EAR\t= 16, Slider, None, EAR_ElfEarType3 # trailing\r\n\
        AN_Alinah Rin   = -1, Slider, None,  Alinah Rin \r\n\
        CME_EyesType = 32, Preset, ece_HumEyesType, 25\r\n\
        EXPR2_Sad = 1024, Slider, None, EXPR2_Sad\r\n\
        EXPR2_Sad = 1024, Slider, None, Second\r\n\
        Broken = 8, Wobble, 3\r\n\
        NoEquals\r\n\
        [Other]\r\n\
        Hidden = 8, Slider, None, X\r\n";

    #[test]
    fn slider_files_read_by_section_with_padding_and_spaces() {
        let (s, notes) = parse_sliders(SLIDERS.as_bytes(), "s.ini");
        assert_eq!(s.male.len(), 1);
        assert_eq!(s.male[0].name, "M_Only");
        let names: Vec<&str> = s.female.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "UBE_Makeup",
                "AB_LIP_Width",
                "DQ_EAR",
                "AN_Alinah Rin",
                "CME_EyesType",
                "EXPR2_Sad"
            ]
        );
        assert_eq!(s.female[0].kind, SliderKind::HeadPart { kind: 69 });
        assert_eq!(
            s.female[1].kind,
            SliderKind::Slider {
                low: Some("LIP_Narrow".into()),
                high: Some("LIP_Wide".into())
            }
        );
        assert_eq!(s.female[1].category, 128);
        assert_eq!(
            s.female[3].kind,
            SliderKind::Slider {
                low: None,
                high: Some("Alinah Rin".into())
            }
        );
        assert_eq!(s.female[3].category, -1);
        assert_eq!(
            s.female[4].kind,
            SliderKind::Preset {
                prefix: "ece_HumEyesType".into(),
                count: 25
            }
        );
        // The first of two of one name.
        assert_eq!(
            s.female[5].kind,
            SliderKind::Slider {
                low: None,
                high: Some("EXPR2_Sad".into())
            }
        );
        // Before any section, given again, a type it doesn't know, no `=`,
        // and a section it doesn't know.
        assert_eq!(notes.len(), 5, "{notes:?}");
    }

    #[test]
    fn morphs_and_races_read_with_their_paths() {
        let morphs = "# source TRI, additional TRI\r\n\
            extension = !UBE\\Head\\FemaleHeadCharGen.tri, John\\EXPR_FemaleHead.tri,  John\\Nose_FemaleHead.tri, John\\EXPR_FemaleHead.tri\r\n\
            extension = !ube/head/femaleheadchargen.tri, LRO\\EXPR2\\FemaleHead.tri\r\n\
            extension = a.tri, ..\\..\\..\\secret.tri, c:\\x.tri\r\n\
            other = x.tri, y.tri\r\n";
        let (m, notes) = parse_morphs(morphs.as_bytes(), "m.ini");
        assert_eq!(
            m,
            [(
                "meshes/!ube/head/femaleheadchargen.tri".to_string(),
                vec![
                    format!("{MORPHS}/john/expr_femalehead.tri"),
                    format!("{MORPHS}/john/nose_femalehead.tri"),
                    format!("{MORPHS}/lro/expr2/femalehead.tri"),
                ]
            )]
        );
        assert_eq!(notes.len(), 3, "{notes:?}");

        let races = "# races\r\n00UBE_NordRace          = sliders\\JohnWick.ini\r\n\
            00ube_nordrace = sliders\\Other.ini\r\nnothing\r\n";
        let (r, notes) = parse_races(races.as_bytes(), "r.ini");
        assert_eq!(
            r,
            [(
                "00UBE_NordRace".to_string(),
                "sliders\\JohnWick.ini".to_string()
            )]
        );
        assert_eq!(notes.len(), 2);
    }

    fn folder_files() -> Files {
        let mut f = Files::new();
        let d = "Meshes\\Actors\\Character\\FaceGenMorphs";
        f.insert(
            &format!("{d}\\Mod.esp\\morphs.ini"),
            b"extension = Head\\HeadCharGen.tri, A\\One.tri\r\n".to_vec(),
        );
        f.insert(
            &format!("{d}\\Mod.esp\\races.ini"),
            b"NordRace = sliders\\a.ini\r\nElfRace = sliders\\..\\..\\x.ini\r\nOrcRace = sliders\\gone.ini\r\n"
                .to_vec(),
        );
        f.insert(
            &format!("{d}\\Mod.esp\\sliders\\a.ini"),
            b"[Female]\r\nS = 1, Slider, Lo, Hi\r\nT = 1, Slider, None, Mine\r\n".to_vec(),
        );
        f.insert(
            &format!("{d}\\Second.esl\\morphs.ini"),
            b"extension = head/headchargen.tri, B\\Two.tri, A\\One.tri\r\n".to_vec(),
        );
        f.insert(
            &format!("{d}\\Second.esl\\races.ini"),
            b"nordrace = sliders\\b.ini\r\n".to_vec(),
        );
        f.insert(
            &format!("{d}\\Second.esl\\sliders\\b.ini"),
            b"[Female]\r\nt = 1, Slider, None, Theirs\r\nU = 1, Preset, Pre, 3\r\n".to_vec(),
        );
        f.insert(
            &format!("{d}\\NotLoaded.esp\\races.ini"),
            b"NordRace = sliders\\c.ini\r\n".to_vec(),
        );
        f
    }

    #[test]
    fn folders_of_loaded_plugins_only_in_load_order() {
        let files = folder_files();
        let plugins = ["Skyrim.esm", "Mod.esp", "Second.esl", "../evil"];
        // Before anything is fetched: each folder's two files.
        let (needed, notes) = RaceMenuMorphs::needed(&plugins, &Files::new());
        assert_eq!(needed.len(), 6, "{needed:?}");
        assert!(needed.contains(&format!("{FOLDER}/mod.esp/races.ini")));
        assert_eq!(notes.len(), 1, "{notes:?}");
        // With the folders there: only what is still not, the slider file
        // that isn't there among it, and no path that leaves the folder.
        let (needed, notes) = RaceMenuMorphs::needed(&plugins, &files);
        assert_eq!(
            needed,
            [
                format!("{FOLDER}/skyrim.esm/morphs.ini"),
                format!("{FOLDER}/skyrim.esm/races.ini"),
                format!("{FOLDER}/mod.esp/sliders/gone.ini"),
            ]
        );
        assert_eq!(notes.len(), 2, "{notes:?}");
        // A host gives what isn't there as nothing, and is then done.
        let mut all = files.clone();
        for k in &needed {
            all.insert(k, Vec::new());
        }
        assert!(RaceMenuMorphs::needed(&plugins, &all).0.is_empty());

        let rm = RaceMenuMorphs::read(&plugins, &files);
        let s: Vec<&str> = rm
            .sliders_for("NORDRACE", true)
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(s, ["S", "T", "U"]);
        // T is the first folder's.
        assert_eq!(
            rm.sliders_for("NordRace", true)[1].kind,
            SliderKind::Slider {
                low: None,
                high: Some("Mine".into())
            }
        );
        assert!(rm.sliders_for("NordRace", false).is_empty());
        assert!(rm.sliders_for("ImperialRace", true).is_empty());
        assert_eq!(
            rm.extras_for("Meshes\\HEAD\\HeadCharGen.tri"),
            [format!("{MORPHS}/a/one.tri"), format!("{MORPHS}/b/two.tri")]
        );
        assert_eq!(rm.extras_for("head/headchargen.tri").len(), 2);
        assert!(rm.extras_for("head/other.tri").is_empty());
        assert_eq!(rm.races(), ["NordRace"]);
        // The bad path, the missing file and the plugin name.
        assert_eq!(rm.notes.len(), 3, "{:?}", rm.notes);
    }

    #[test]
    fn weights_by_sign_presets_by_number_and_what_is_not_drawn() {
        let (file, _) = parse_sliders(SLIDERS.as_bytes(), "s.ini");
        let sliders: Vec<&SliderDef> = file.female.iter().collect();
        let custom: Vec<(String, f32)> = [
            ("AB_LIP_Width", -0.5),
            ("ab_lip_width", 0.9),
            ("DQ_EAR", 0.25),
            ("AN_Alinah Rin", -0.3),
            ("CME_EyesType", 7.0),
            ("UBE_Makeup", 2.0),
            ("Nobody", 0.0),
            ("EXPR2_Sad", f32::NAN),
            ("M_Only", 1.0),
        ]
        .into_iter()
        .map(|(n, v)| (n.to_string(), v))
        .collect();
        let (w, notes) = weights(&custom, &sliders);
        assert_eq!(
            w,
            HashMap::from([
                ("lip_narrow".to_string(), 0.5),
                ("ear_elfeartype3".to_string(), 0.25),
                ("ece_humeyestype7".to_string(), 1.0),
            ])
        );
        assert_eq!(
            notes,
            [
                SliderNote::Repeated {
                    name: "ab_lip_width".into()
                },
                SliderNote::NoMorphThatWay {
                    name: "AN_Alinah Rin".into(),
                    value: -0.3
                },
                SliderNote::HeadPart {
                    name: "UBE_Makeup".into(),
                    value: 2.0
                },
                SliderNote::Unknown {
                    name: "Nobody".into()
                },
                SliderNote::NotANumber {
                    name: "EXPR2_Sad".into()
                },
                SliderNote::Unknown {
                    name: "M_Only".into()
                },
            ]
        );
        for v in [26.0, -1.0, 2.5] {
            let (w, notes) = weights(&[("CME_EyesType".into(), v)], &sliders);
            assert!(w.is_empty());
            assert!(matches!(notes[..], [SliderNote::NotAChoice { .. }]), "{v}");
        }
        for (v, name) in [(1.0, "ece_humeyestype1"), (25.0, "ece_humeyestype25")] {
            let (w, notes) = weights(&[("CME_EyesType".into(), v)], &sliders);
            assert_eq!(w.keys().collect::<Vec<_>>(), [name]);
            assert!(notes.is_empty());
        }
        let (w, notes) = weights(&[("CME_EyesType".into(), 0.0)], &sliders);
        assert!(w.is_empty() && notes.is_empty());
        // Two sliders on one morph add up.
        let def = |name: &str| SliderDef {
            name: name.into(),
            category: 0,
            kind: SliderKind::Slider {
                low: None,
                high: Some("Same".into()),
            },
        };
        let (a, b) = (def("A"), def("B"));
        let (w, _) = weights(&[("A".into(), 0.25), ("B".into(), 0.5)], &[&a, &b]);
        assert_eq!(w, HashMap::from([("same".to_string(), 0.75)]));
    }

    /// Folders of `n` races, sliders and extensions each, as files.
    fn big(folders: usize, n: usize) -> (Vec<String>, Files) {
        let races: String = (0..n)
            .map(|i| format!("Race{i} = sliders\\s.ini\n"))
            .collect();
        let sliders: String = (0..n)
            .map(|i| format!("Slider{i} = 1, Slider, Low{i}, High{i}\n"))
            .collect();
        let sliders = format!("[Female]\n{sliders}");
        let morphs: String = (0..n)
            .map(|i| format!("extension = s{i}.tri, a{i}.tri, b{i}.tri\n"))
            .collect();
        let plugins: Vec<String> = (0..folders).map(|i| format!("Mod{i}.esp")).collect();
        let mut files = Files::new();
        for p in &plugins {
            let d = format!("{FOLDER}/{p}");
            files.insert(&format!("{d}/races.ini"), races.clone().into_bytes());
            files.insert(&format!("{d}/sliders/s.ini"), sliders.clone().into_bytes());
            files.insert(&format!("{d}/morphs.ini"), morphs.clone().into_bytes());
        }
        (plugins, files)
    }

    /// Asking, reading and looking up, as a host and a plan do it.
    fn use_all(plugins: &[String], files: &Files) -> RaceMenuMorphs {
        let names: Vec<&str> = plugins.iter().map(String::as_str).collect();
        assert!(RaceMenuMorphs::needed(&names, files).0.is_empty());
        let rm = RaceMenuMorphs::read(&names, files);
        for i in 0..50 {
            rm.sliders_for(&format!("race{i}"), true);
        }
        rm.extras_for("s9.tri");
        rm
    }

    #[test]
    fn past_the_most_sliders_from_all_folders_the_rest_are_noted() {
        let (plugins, files) = big(2, MAX_ENTRIES);
        let rm = use_all(&plugins, &files);
        assert_eq!(rm.sliders_for("Race7", true).len(), MAX_SLIDERS);
        assert!(
            rm.notes
                .iter()
                .any(|n| n.contains(&format!("past the most sliders kept from all folders ({MAX_SLIDERS}), {MAX_ENTRIES} are left out"))),
            "{:?}",
            rm.notes
        );
        // Worked out once, then the same.
        assert_eq!(rm.sliders_for("RACE7", true), rm.sliders_for("race7", true));
        // Both folders name the same two extras: each once.
        assert_eq!(rm.extras_for("s9.tri").len(), 2);
    }

    /// Eight folders: four times the entries take about four times as
    /// long, not sixteen (the first version took half a minute for 16,384
    /// each). The fastest of three runs of each, against noise.
    #[test]
    fn eight_full_folders_take_time_in_step_with_their_size() {
        let time = |n: usize| {
            let (plugins, files) = big(8, n);
            (0..3)
                .map(|_| {
                    let start = std::time::Instant::now();
                    use_all(&plugins, &files);
                    start.elapsed().as_secs_f64()
                })
                .fold(f64::MAX, f64::min)
        };
        let (small, large) = (time(512), time(2048));
        assert!(large / small < 9.0, "{small} s, then {large} s");
    }

    #[test]
    fn paths_and_plugin_names_that_could_leave_their_folder_are_refused() {
        for bad in [
            "..",
            ". .",
            "...",
            "a/ ../b.ini",
            "a\\.. \\b.ini",
            "/etc/x.ini",
            "\\\\server\\x.ini",
            "C:x.ini",
            "a\u{0}.ini",
            "a\u{1b}b.ini",
            "",
        ] {
            assert_eq!(safe_path(bad), None, "{bad:?}");
        }
        assert_eq!(
            safe_path("Sliders\\A.ini").as_deref(),
            Some("sliders/a.ini")
        );
        assert_eq!(safe_path("a..b/c.ini").as_deref(), Some("a..b/c.ini"));
        for bad in [
            "",
            ".",
            "..",
            "...",
            " . ",
            "a/b.esp",
            "a\\b.esp",
            "C:a.esp",
            "a?.esp",
            "a\u{0}.esp",
        ] {
            assert_eq!(folder_of(bad), None, "{bad:?}");
        }
        assert!(folder_of("Expressive Facegen Morphs.esl").is_some());
        let (needed, notes) = RaceMenuMorphs::needed(&["..", "Ok.esp"], &Files::new());
        assert_eq!(needed.len(), 2);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn hostile_input_is_held_and_never_panics() {
        let long = "x".repeat(MAX_TEXT + 1);
        let mut file = String::from("[Female]\n");
        file.push_str(&format!("{long} = 1, Slider, None, A\n"));
        file.push_str(&format!("A = 1, Slider, None, {long}\n"));
        file.push_str("B = 99999999999, Slider, None, A\n");
        file.push_str("C = 1, Preset, P, -4\n");
        file.push_str("D = 1, HeadPart\n");
        file.push_str("= 1, Slider, None, A\n");
        file.push_str("[\n]\n[[Female]]\n\u{0}\u{FFFD}=,,,,\n");
        let (s, notes) = parse_sliders(file.as_bytes(), "s.ini");
        assert!(s.female.is_empty(), "{s:?}");
        assert!(!notes.is_empty());

        let many: String = (0..MAX_ENTRIES + 10)
            .map(|i| format!("S{i} = 1, Slider, None, M\n"))
            .collect();
        let (s, _) = parse_sliders(format!("[Male]\n{many}").as_bytes(), "s.ini");
        assert_eq!(s.male.len(), MAX_ENTRIES);

        let lines = "\n".repeat(MAX_LINES + 5) + "[Male]\nA = 1, Slider, None, M\n";
        let (s, notes) = parse_sliders(lines.as_bytes(), "s.ini");
        assert!(s.male.is_empty());
        assert!(notes.iter().any(|n| n.contains("lines")));

        let extras: String = (0..MAX_EXTRAS + 5).map(|i| format!(", e{i}.tri")).collect();
        let (m, notes) = parse_morphs(format!("extension = a.tri{extras}").as_bytes(), "m");
        assert_eq!(m[0].1.len(), MAX_EXTRAS);
        assert_eq!(notes.len(), 1);

        // Bytes of every kind, cut anywhere.
        let bytes: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        for cut in (0..bytes.len()).step_by(97) {
            let _ = parse_sliders(&bytes[..cut], "s");
            let _ = parse_morphs(&bytes[..cut], "m");
            let _ = parse_races(&bytes[..cut], "r");
        }
        let (_, notes) = parse_sliders(&vec![b'\n'; MAX_FILE + 1], "big");
        assert!(notes[0].contains("longer"));

        // Notes are held to their most.
        let bad: String = (0..MAX_NOTES + 50).map(|_| "junk\n").collect();
        let (_, notes) = parse_races(bad.as_bytes(), "r");
        assert_eq!(notes.len(), MAX_NOTES + 1);
        assert_eq!(notes[MAX_NOTES], "And 50 more.");
    }
}
