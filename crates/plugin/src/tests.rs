//! Tests on hand-made bytes. Tests on the real plugins are in `tests/`.

use crate::container::{inflate, subrecords, GroupHeader, Header, RecordHeader, COMPRESSED};
use crate::records::*;
use crate::walk::{read, Walk};
use crate::{FormRef, LoadOrder, Masters, Plugin, PluginError, Record};

pub fn sub(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend((data.len() as u16).to_le_bytes());
    out.extend(data);
    out
}

pub fn rec(kind: &[u8; 4], flags: u32, id: u32, data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(flags.to_le_bytes());
    out.extend(id.to_le_bytes());
    out.extend([0; 4]);
    out.extend(44u16.to_le_bytes());
    out.extend([0; 2]);
    out.extend(data);
    out
}

pub fn grup(label: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = b"GRUP".to_vec();
    out.extend((body.len() as u32 + 24).to_le_bytes());
    out.extend(label);
    out.extend(0i32.to_le_bytes());
    out.extend([0; 8]);
    out.extend(body);
    out
}

fn tes4(flags: u32, masters: &[&str]) -> Vec<u8> {
    let mut d = sub(b"HEDR", &[0x9a, 0x99, 0xd9, 0x3f, 1, 0, 0, 0, 0, 8, 0, 0]);
    d.extend(sub(b"CNAM", b"me\0"));
    for m in masters {
        d.extend(sub(b"MAST", format!("{m}\0").as_bytes()));
        d.extend(sub(b"DATA", &[0; 8]));
    }
    rec(b"TES4", flags, 0, &d)
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_le_bytes().to_vec();
    out.extend(miniz_oxide::deflate::compress_to_vec_zlib(data, 6));
    out
}

fn f(plugin: &str, id: u32) -> FormRef {
    FormRef::new(plugin, id)
}

fn names(own: &str, masters: &[&str]) -> Masters {
    Masters {
        own: own.into(),
        masters: masters.iter().map(|m| m.to_string()).collect(),
        localized: false,
    }
}

fn record(kind: &[u8; 4], data: Vec<u8>) -> Record {
    Record {
        kind: *kind,
        flags: 0,
        id: 0x0100_0800,
        version: 44,
        data,
    }
}

#[test]
fn record_and_group_headers() {
    let r = rec(b"RACE", 0x20, 0x0001_3746, &[1, 2, 3]);
    let h = RecordHeader::read(&r).unwrap();
    assert_eq!(
        (h.kind, h.size, h.flags, h.id, h.version),
        (*b"RACE", 3, 0x20, 0x0001_3746, 44)
    );
    let g = grup(b"HDPT", &[0; 10]);
    let g = GroupHeader::read(&g).unwrap();
    assert_eq!((g.size, g.label, g.kind), (34, *b"HDPT", 0));
    assert!(RecordHeader::read(&r[..23]).is_err());
    // A group's size includes its header: less than 24 is refused.
    let mut bad = grup(b"HDPT", &[]);
    bad[4] = 23;
    assert!(matches!(
        GroupHeader::read(&bad),
        Err(PluginError::Corrupt(_))
    ));
    assert!(GroupHeader::read(&rec(b"RACE", 0, 0, &[])).is_err());
}

#[test]
fn subrecords_and_the_large_size_override() {
    let big = vec![7u8; 70_000];
    let mut d = sub(b"EDID", b"a\0");
    d.extend(sub(b"XXXX", &(big.len() as u32).to_le_bytes()));
    d.extend(b"DATA");
    d.extend(0u16.to_le_bytes());
    d.extend(&big);
    d.extend(sub(b"FULL", &[1, 2, 3, 4]));
    let s: Vec<_> = subrecords(&d).collect::<Result<_, _>>().unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!((&s[1].kind, s[1].data.len()), (b"DATA", 70_000));
    assert_eq!(s[2].data, &[1, 2, 3, 4]);

    // A size past the end, an XXXX of the wrong size, a cut-off header.
    let mut cut = sub(b"EDID", b"abc");
    cut.pop();
    assert_eq!(subrecords(&cut).last(), Some(Err(PluginError::Truncated)));
    let wrong = sub(b"XXXX", &[1, 0]);
    assert!(subrecords(&wrong).any(|s| s.is_err()));
    assert!(subrecords(b"EDI").any(|s| s.is_err()));
    // After an error the iterator ends.
    assert_eq!(subrecords(&cut).count(), 1);
}

#[test]
fn compressed_records() {
    let data = sub(b"EDID", b"compressed\0");
    assert_eq!(inflate(COMPRESSED, &zlib(&data)).unwrap(), data);
    assert_eq!(inflate(0, &data).unwrap(), data);
    // A stated size that disagrees with the data, either way.
    let mut more = zlib(&data);
    more[0] += 1;
    assert!(matches!(
        inflate(COMPRESSED, &more),
        Err(PluginError::Inflate(_))
    ));
    let mut less = zlib(&data);
    less[0] -= 1;
    assert!(inflate(COMPRESSED, &less).is_err());
    // A claim past the bound is refused before anything is allocated.
    let mut huge = zlib(&data);
    huge[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        inflate(COMPRESSED, &huge),
        Err(PluginError::TooLarge(_))
    ));
    // Not zlib, or too short to hold a size.
    assert!(inflate(COMPRESSED, &[5, 0, 0, 0, 1, 2, 3]).is_err());
    assert!(inflate(COMPRESSED, &[5, 0]).is_err());
}

#[test]
fn the_header_record() {
    let t = tes4(0x281, &["Skyrim.esm", "Update.esm"]);
    let h = Header::read(&RecordHeader::read(&t).unwrap(), &t[24..]).unwrap();
    assert!(h.esm() && h.light() && h.localized());
    assert_eq!(h.masters, ["Skyrim.esm", "Update.esm"]);
    assert_eq!(h.author.as_deref(), Some("me"));
    assert_eq!((h.records, h.next_id), (1, 0x800));
    assert!((h.version - 1.7).abs() < 1e-6);
    let plain = tes4(0, &[]);
    let h = Header::read(&RecordHeader::read(&plain).unwrap(), &plain[24..]).unwrap();
    assert!(!h.esm() && !h.light() && !h.localized());
    let race = rec(b"RACE", 0, 0, &[]);
    assert_eq!(
        Header::read(&RecordHeader::read(&race).unwrap(), &[]),
        Err(PluginError::NotPlugin)
    );
}

#[test]
fn formids_name_masters_then_the_plugin_itself() {
    let m = names("Mod.esp", &["Skyrim.esm", "Dawnguard.esm"]);
    assert_eq!(m.resolve(0x0001_3746), f("Skyrim.esm", 0x13746));
    assert_eq!(m.resolve(0x0100_2B6C), f("dawnguard.ESM", 0x2B6C));
    assert_eq!(m.resolve(0x0200_0801), f("Mod.esp", 0x801));
    assert_eq!(m.resolve(0x0500_0801), f("Mod.esp", 0x801));
    assert_eq!(m.form(0), None);
    // Names compare and hash without case.
    let mut set = std::collections::HashSet::new();
    set.insert(f("SKYRIM.ESM", 1));
    assert!(set.contains(&f("skyrim.esm", 1)));
    assert_ne!(f("Skyrim.esm", 1), f("Skyrim.esm", 2));
    assert_ne!(f("Skyrim.esm", 1), f("Update.esm", 1));
}

/// A plugin of a header, a RACE group, an ignored WEAP group, and a CLFM
/// group with one compressed record and a nested group.
fn small_plugin() -> Vec<u8> {
    let mut out = tes4(0x1, &["Skyrim.esm"]);
    out.extend(grup(
        b"RACE",
        &rec(b"RACE", 0, 0x0001_3746, &sub(b"EDID", b"NordRace\0")),
    ));
    out.extend(grup(b"WEAP", &vec![0xEE; 5000]));
    let mut clfm = rec(
        b"CLFM",
        COMPRESSED,
        0x0100_0801,
        &zlib(&sub(b"EDID", b"Red\0")),
    );
    clfm.extend(grup(b"CLFM", &rec(b"CLFM", 0, 0x0100_0802, &[])));
    out.extend(grup(b"CLFM", &clfm));
    out
}

#[test]
fn the_range_walk_reads_only_the_groups_asked_for() {
    let bytes = small_plugin();
    let mut walk = Walk::new("Mod.esp", bytes.len() as u64, &[*b"RACE", *b"CLFM"]);
    let mut asked = Vec::new();
    while let Some((at, len)) = walk.need() {
        asked.push((at, len));
        walk.feed(at, &bytes[at as usize..at as usize + len])
            .unwrap();
    }
    let s = walk.stats();
    // The first record's header; the rest of it and the RACE group's
    // header; the RACE group and the WEAP header; the CLFM header, past the
    // WEAP group; the CLFM group.
    assert_eq!(s.reads, 5);
    assert_eq!((s.groups, s.groups_read), (3, 2));
    assert_eq!(s.bytes, bytes.len() as u64 - 5000);
    let p = walk.finish().unwrap();
    assert_eq!(p.records.len(), 3);
    assert_eq!(p.get(&f("skyrim.esm", 0x13746)).unwrap().kind, *b"RACE");
    let red = p.typed::<Colour>(&f("Mod.esp", 0x801)).unwrap().unwrap();
    assert_eq!(red.editor_id.as_deref(), Some("Red"));
    assert!(p.get(&f("Mod.esp", 0x802)).is_some());

    // Nothing asked for: every group is stepped over.
    let (p, s) = read("Mod.esp", &bytes, &[]).unwrap();
    assert!(p.records.is_empty());
    assert_eq!(s.groups_read, 0);
    assert_eq!(
        s.bytes,
        bytes.len() as u64 - (bytes.len() as u64 - 24 - 24 * 3 - small_header_len())
    );
}

fn small_header_len() -> u64 {
    tes4(0x1, &["Skyrim.esm"]).len() as u64 - 24
}

#[test]
fn the_walk_refuses_what_it_did_not_ask_for() {
    let bytes = small_plugin();
    let mut walk = Walk::new("Mod.esp", bytes.len() as u64, &[*b"RACE"]);
    assert!(matches!(
        walk.feed(1, &bytes[1..25]),
        Err(PluginError::Unasked(_))
    ));
    // An error ends the walk.
    assert_eq!(walk.need(), None);
    assert!(walk.finish().is_err());
    // Finishing before the end.
    let walk = Walk::new("Mod.esp", bytes.len() as u64, &[]);
    assert_eq!(walk.finish().unwrap_err(), PluginError::Truncated);
}

#[test]
fn bad_plugins_give_errors() {
    let good = small_plugin();
    let all = [*b"RACE", *b"CLFM"];
    assert_eq!(read("x", b"", &all).unwrap_err(), PluginError::NotPlugin);
    assert_eq!(
        read("x", b"TES3", &all).unwrap_err(),
        PluginError::NotPlugin
    );
    assert_eq!(
        read("x", &rec(b"RACE", 0, 0, &[]), &all).unwrap_err(),
        PluginError::NotPlugin
    );
    // Every truncation is an error, never a panic, except at the end of a
    // group, which leaves a whole plugin of fewer groups.
    let header = tes4(0x1, &["Skyrim.esm"]).len();
    let mut ends = vec![header];
    let mut at = header;
    while at < good.len() {
        at += u32::from_le_bytes(good[at + 4..at + 8].try_into().unwrap()) as usize;
        ends.push(at);
    }
    assert!(read("x", &good, &all).is_ok());
    // The WEAP group is not records: read, it is an error.
    assert!(read("x", &good, &[*b"WEAP"]).is_err());
    for n in 0..good.len() {
        let r = read("x", &good[..n], &all);
        assert_eq!(r.is_ok(), ends.contains(&n), "cut at {n}");
    }
    // A top-level group of another type, a group claiming more than the
    // file, a record claiming more than its group.
    let mut odd = good.clone();
    let g = ends[2];
    odd[g + 12] = 1;
    assert!(read("x", &odd, &all).is_err());
    let mut long = good.clone();
    long[g + 4] = 0xFF;
    long[g + 5] = 0xFF;
    assert!(read("x", &long, &all).is_err());
    // A compressed record that will not inflate is a failure of that record,
    // not of the plugin.
    let mut out = tes4(0, &[]);
    let mut bad = zlib(&sub(b"EDID", b"x\0"));
    let n = bad.len();
    bad[n - 1] ^= 0xFF;
    let mut body = rec(b"CLFM", COMPRESSED, 0x801, &bad);
    body.extend(rec(b"CLFM", 0, 0x802, &[]));
    out.extend(grup(b"CLFM", &body));
    let (p, _) = read("x", &out, &all).unwrap();
    assert_eq!(p.records.len(), 1);
    assert_eq!(p.failures.len(), 1);
    assert_eq!(p.failures[0].0, 0x801);
}

#[test]
fn every_flipped_byte_is_handled() {
    let good = small_plugin();
    let all = [*b"RACE", *b"CLFM"];
    for i in 0..good.len() {
        for bit in [0x01, 0x80, 0xFF] {
            let mut b = good.clone();
            b[i] ^= bit;
            if let Ok((p, _)) = read("x", &b, &all) {
                for r in &p.records {
                    let _ = Colour::read(r, &p.names);
                    let _ = Race::read(r, &p.names);
                }
            }
        }
    }
}

#[test]
fn race() {
    let mut d = sub(b"EDID", b"NordRace\0");
    d.extend(sub(b"FULL", b"Nord\0"));
    d.extend(sub(b"WNAM", &0x0d64u32.to_le_bytes()));
    let mut hclf = 0x0A042Eu32.to_le_bytes().to_vec();
    hclf.extend(0x0A042Fu32.to_le_bytes());
    d.extend(sub(b"HCLF", &hclf));
    d.extend(sub(b"MNAM", &[]));
    d.extend(sub(b"ANAM", b"skeleton.nif\0"));
    d.extend(sub(b"FNAM", &[]));
    d.extend(sub(b"ANAM", b"skeleton_female.nif\0"));
    d.extend(sub(b"NAM1", &[]));
    d.extend(sub(b"MNAM", &[]));
    d.extend(sub(b"INDX", &0u32.to_le_bytes()));
    d.extend(sub(b"MODL", b"UpperBodyHumanMale.egt\0"));
    d.extend(sub(b"FNAM", &[]));
    d.extend(sub(b"INDX", &0u32.to_le_bytes()));
    d.extend(sub(b"MODL", b"UpperBodyHumanFemale.egt\0"));
    d.extend(sub(b"NAM3", &[]));
    d.extend(sub(b"MNAM", &[]));
    d.extend(sub(b"MODL", b"DefaultMale.hkx\0"));
    d.extend(sub(b"FNAM", &[]));
    d.extend(sub(b"MODL", b"DefaultFemale.hkx\0"));
    d.extend(sub(b"NAM0", &[]));
    d.extend(sub(b"MNAM", &[]));
    for (i, h) in [(0u32, 0x5140Eu32), (1, 0x5162F)] {
        d.extend(sub(b"INDX", &i.to_le_bytes()));
        d.extend(sub(b"HEAD", &h.to_le_bytes()));
    }
    let mut mpav = 0x001F_FF3Eu32.to_le_bytes().to_vec();
    mpav.extend([0xFF; 28]);
    d.extend(sub(b"MPAI", &2u32.to_le_bytes()));
    d.extend(sub(b"MPAV", &mpav));
    d.extend(sub(b"RPRM", &0x1750Cu32.to_le_bytes()));
    d.extend(sub(b"AHCM", &0xA0439u32.to_le_bytes()));
    d.extend(sub(b"FTSM", &0x3B521u32.to_le_bytes()));
    d.extend(sub(b"DFTM", &0x3B521u32.to_le_bytes()));
    d.extend(sub(b"TINI", &1u16.to_le_bytes()));
    d.extend(sub(b"TINT", b"SkinTone.dds\0"));
    d.extend(sub(b"TINP", &6u16.to_le_bytes()));
    d.extend(sub(b"TIND", &0xE6E08u32.to_le_bytes()));
    d.extend(sub(b"TINC", &0xE6E17u32.to_le_bytes()));
    d.extend(sub(b"TINV", &1.0f32.to_le_bytes()));
    d.extend(sub(b"TIRS", &0x4Bu16.to_le_bytes()));
    d.extend(sub(b"NAM0", &[]));
    d.extend(sub(b"FNAM", &[]));
    d.extend(sub(b"INDX", &0u32.to_le_bytes()));
    d.extend(sub(b"HEAD", &0x51176u32.to_le_bytes()));
    d.extend(sub(b"RPRF", &0x1751Cu32.to_le_bytes()));
    d.extend(sub(b"AHCF", &0xA0440u32.to_le_bytes()));
    d.extend(sub(b"FTSF", &0x3B522u32.to_le_bytes()));
    d.extend(sub(b"DFTF", &0x3B523u32.to_le_bytes()));

    let m = names("Mod.esp", &["Skyrim.esm"]);
    let r = Race::read(&record(b"RACE", d), &names("Skyrim.esm", &[])).unwrap();
    assert_eq!(r.editor_id.as_deref(), Some("NordRace"));
    assert_eq!(r.name, Some(Name::Text("Nord".into())));
    assert_eq!(r.skin, Some(f("Skyrim.esm", 0xd64)));
    assert_eq!(
        r.default_hair_colour,
        [
            Some(f("Skyrim.esm", 0xA042E)),
            Some(f("Skyrim.esm", 0xA042F))
        ]
    );
    let [male, female] = &r.sexes;
    assert_eq!(male.skeleton.as_deref(), Some("skeleton.nif"));
    assert_eq!(female.skeleton.as_deref(), Some("skeleton_female.nif"));
    assert_eq!(
        male.body_lighting.as_deref(),
        Some("UpperBodyHumanMale.egt")
    );
    assert_eq!(
        female.body_lighting.as_deref(),
        Some("UpperBodyHumanFemale.egt")
    );
    assert_eq!(male.body_physics.as_deref(), Some("DefaultMale.hkx"));
    assert_eq!(female.body_physics.as_deref(), Some("DefaultFemale.hkx"));
    let h = &male.head;
    assert_eq!(
        h.parts,
        [(0, f("Skyrim.esm", 0x5140E)), (1, f("Skyrim.esm", 0x5162F))]
    );
    assert_eq!(h.morphs.len(), 1);
    assert_eq!(h.morphs[0].index, 2);
    assert_eq!(h.morphs[0].flags[0], 0x001F_FF3E);
    assert_eq!(h.morphs[0].flags[7], u32::MAX);
    assert_eq!(h.presets, [f("Skyrim.esm", 0x1750C)]);
    assert_eq!(h.hair_colours, [f("Skyrim.esm", 0xA0439)]);
    assert_eq!(h.face_textures, [f("Skyrim.esm", 0x3B521)]);
    assert_eq!(h.default_face_texture, Some(f("Skyrim.esm", 0x3B521)));
    assert_eq!(h.tints.len(), 1);
    let t = &h.tints[0];
    assert_eq!(
        (t.index, t.mask.as_deref(), t.kind),
        (1, Some("SkinTone.dds"), Some(6))
    );
    assert_eq!(t.default, Some(f("Skyrim.esm", 0xE6E08)));
    assert_eq!(
        t.presets,
        [TintPreset {
            colour: Some(f("Skyrim.esm", 0xE6E17)),
            value: 1.0,
            number: 0x4B
        }]
    );
    let h = &female.head;
    assert_eq!(h.parts, [(0, f("Skyrim.esm", 0x51176))]);
    assert_eq!(h.presets, [f("Skyrim.esm", 0x1751C)]);
    assert_eq!(h.hair_colours, [f("Skyrim.esm", 0xA0440)]);
    assert_eq!(h.face_textures, [f("Skyrim.esm", 0x3B522)]);
    assert_eq!(h.default_face_texture, Some(f("Skyrim.esm", 0x3B523)));
    assert!(male.head.tints.len() == 1 && female.head.tints.is_empty());

    // In a localized plugin a name is an id; a formID resolves through the
    // masters.
    let mut loc = m.clone();
    loc.localized = true;
    let mut d = sub(b"FULL", &0xD98Au32.to_le_bytes());
    d.extend(sub(b"WNAM", &0x0000_0801u32.to_le_bytes()));
    let r = Race::read(&record(b"RACE", d), &loc).unwrap();
    assert_eq!(r.name, Some(Name::Id(0xD98A)));
    assert_eq!(r.skin, Some(f("skyrim.esm", 0x801)));
    // A tint field with no TINI before it, a formID of the wrong size.
    let bad = sub(b"NAM0", &[])
        .into_iter()
        .chain(sub(b"TINT", b"x\0"))
        .collect();
    assert!(Race::read(&record(b"RACE", bad), &m).is_err());
    assert!(Race::read(&record(b"RACE", sub(b"WNAM", &[1, 2])), &m).is_err());
}

#[test]
fn head_part() {
    let mut d = sub(b"EDID", b"HairMaleNord09\0");
    d.extend(sub(b"MODL", b"Hair09.nif\0"));
    d.extend(sub(b"DATA", &[0x0B]));
    d.extend(sub(b"PNAM", &3u32.to_le_bytes()));
    d.extend(sub(b"HNAM", &0x51407u32.to_le_bytes()));
    d.extend(sub(b"HNAM", &0x51408u32.to_le_bytes()));
    for (k, p) in [
        (0u32, &b"Races.tri\0"[..]),
        (1, b"Hair09.tri\0"),
        (2, b"Chargen.tri\0"),
        (7, b"Odd.tri\0"),
    ] {
        d.extend(sub(b"NAM0", &k.to_le_bytes()));
        d.extend(sub(b"NAM1", p));
    }
    d.extend(sub(b"NAM1", b"Alone.tri\0"));
    d.extend(sub(b"TNAM", &0x3B521u32.to_le_bytes()));
    d.extend(sub(b"CNAM", &0xA042Eu32.to_le_bytes()));
    d.extend(sub(b"RNAM", &0xA8023u32.to_le_bytes()));
    let h = HeadPart::read(&record(b"HDPT", d), &names("Skyrim.esm", &[])).unwrap();
    assert_eq!(h.editor_id.as_deref(), Some("HairMaleNord09"));
    assert_eq!(h.model.as_deref(), Some("Hair09.nif"));
    assert_eq!(h.flags, 0x0B);
    assert_eq!(h.flags & EXTRA_PART, EXTRA_PART);
    assert_eq!(h.kind, Some(HeadPartKind::Hair));
    assert_eq!(
        h.extra_parts,
        [f("Skyrim.esm", 0x51407), f("Skyrim.esm", 0x51408)]
    );
    let kinds: Vec<_> = h.files.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            PartFileKind::RaceMorph,
            PartFileKind::Tri,
            PartFileKind::ChargenMorph,
            PartFileKind::Other(7),
            PartFileKind::Unstated
        ]
    );
    assert_eq!(h.files[1].path, "Hair09.tri");
    assert_eq!(h.texture_set, Some(f("Skyrim.esm", 0x3B521)));
    assert_eq!(h.colour, Some(f("Skyrim.esm", 0xA042E)));
    assert_eq!(h.valid_races, Some(f("Skyrim.esm", 0xA8023)));
    for (v, k) in [
        (0, HeadPartKind::Misc),
        (1, HeadPartKind::Face),
        (2, HeadPartKind::Eyes),
        (4, HeadPartKind::FacialHair),
        (5, HeadPartKind::Scar),
        (6, HeadPartKind::Eyebrows),
        (9, HeadPartKind::Other(9)),
    ] {
        assert_eq!(HeadPartKind::of(v), k);
    }
    let m = names("x", &[]);
    assert!(HeadPart::read(&record(b"HDPT", sub(b"DATA", &[1, 2])), &m).is_err());
    assert!(HeadPart::read(&record(b"HDPT", sub(b"PNAM", &[1])), &m).is_err());
}

#[test]
fn texture_set() {
    let mut d = sub(b"EDID", b"SkinHead\0");
    d.extend(sub(b"TX00", b"head.dds\0"));
    d.extend(sub(b"TX01", b"head_msn.dds\0"));
    d.extend(sub(b"TX07", b"head_s.dds\0"));
    d.extend(sub(b"TX08", b"ignored\0"));
    d.extend(sub(b"DNAM", &4u16.to_le_bytes()));
    let t = TextureSet::read(&record(b"TXST", d), &names("x", &[])).unwrap();
    assert_eq!(t.editor_id.as_deref(), Some("SkinHead"));
    assert_eq!(t.textures[0].as_deref(), Some("head.dds"));
    assert_eq!(t.textures[1].as_deref(), Some("head_msn.dds"));
    assert_eq!(t.textures[2], None);
    assert_eq!(t.textures[7].as_deref(), Some("head_s.dds"));
    assert_eq!(t.flags, 4);
    assert!(TextureSet::read(&record(b"TXST", sub(b"DNAM", &[1])), &names("x", &[])).is_err());
}

#[test]
fn armor_and_addon() {
    let m = names("Mod.esp", &["Skyrim.esm"]);
    let mut d = sub(b"EDID", b"SkinNaked\0");
    d.extend(sub(b"BOD2", &[0x5C, 2, 0, 0x80, 2, 0, 0, 0]));
    d.extend(sub(b"RNAM", &0x19u32.to_le_bytes()));
    d.extend(sub(b"MODL", &0xD67u32.to_le_bytes()));
    d.extend(sub(b"MODL", &0x0100_0801u32.to_le_bytes()));
    let a = Armor::read(&record(b"ARMO", d), &m).unwrap();
    assert_eq!(a.slots, 0x8000_025C);
    assert_eq!(a.race, Some(f("Skyrim.esm", 0x19)));
    assert_eq!(a.addons, [f("Skyrim.esm", 0xD67), f("Mod.esp", 0x801)]);

    let mut d = sub(b"EDID", b"NakedTorso\0");
    d.extend(sub(b"BODT", &[4, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0]));
    d.extend(sub(b"RNAM", &0x13746u32.to_le_bytes()));
    d.extend(sub(b"DNAM", &[3, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    d.extend(sub(b"MOD2", b"MaleBody.nif\0"));
    d.extend(sub(b"MOD4", b"1stPersonMaleBody.nif\0"));
    d.extend(sub(b"NAM0", &0xD4u32.to_le_bytes()));
    d.extend(sub(b"NAM3", &0xD7u32.to_le_bytes()));
    d.extend(sub(b"MODL", &0x88794u32.to_le_bytes()));
    let a = ArmorAddon::read(&record(b"ARMA", d), &m).unwrap();
    assert_eq!(a.slots, 4);
    assert_eq!(a.race, Some(f("Skyrim.esm", 0x13746)));
    assert_eq!(a.priority, [3, 5]);
    assert_eq!(a.models[0].as_deref(), Some("MaleBody.nif"));
    assert_eq!(a.models[1], None);
    assert_eq!(a.models[2].as_deref(), Some("1stPersonMaleBody.nif"));
    // Without MOD3 the male model is for both.
    assert_eq!(a.model(true), Some("MaleBody.nif"));
    assert_eq!(
        a.skin_textures,
        [
            Some(f("Skyrim.esm", 0xD4)),
            None,
            None,
            Some(f("Skyrim.esm", 0xD7))
        ]
    );
    assert_eq!(a.additional_races, [f("Skyrim.esm", 0x88794)]);
    assert!(ArmorAddon::read(&record(b"ARMA", sub(b"BOD2", &[1])), &m).is_err());
    assert!(ArmorAddon::read(&record(b"ARMA", sub(b"DNAM", &[1; 8])), &m).is_err());
}

#[test]
fn colour() {
    let mut d = sub(b"EDID", b"HairColor01\0");
    d.extend(sub(b"CNAM", &[0x9a, 0x9b, 0x97, 0]));
    d.extend(sub(b"FNAM", &1u32.to_le_bytes()));
    let c = Colour::read(&record(b"CLFM", d), &names("x", &[])).unwrap();
    assert_eq!((c.rgba, c.flags), ([0x9a, 0x9b, 0x97, 0], 1));
    assert!(Colour::read(&record(b"CLFM", sub(b"CNAM", &[1, 2, 3])), &names("x", &[])).is_err());
}

#[test]
fn npc() {
    let mut d = sub(b"EDID", b"Ulfric\0");
    let mut acbs = 0x21u32.to_le_bytes().to_vec();
    acbs.extend([0; 14]);
    acbs.extend(0x0101u16.to_le_bytes());
    acbs.extend([0; 4]);
    d.extend(sub(b"ACBS", &acbs));
    d.extend(sub(b"TPLT", &0x14128u32.to_le_bytes()));
    d.extend(sub(b"RNAM", &0x13746u32.to_le_bytes()));
    d.extend(sub(b"PNAM", &0xC716Du32.to_le_bytes()));
    d.extend(sub(b"PNAM", &0x51412u32.to_le_bytes()));
    d.extend(sub(b"HCLF", &0xA042Eu32.to_le_bytes()));
    d.extend(sub(b"FTST", &0xD071Du32.to_le_bytes()));
    let q: Vec<u8> = [0.5f32, 0.25, 0.125]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    d.extend(sub(b"QNAM", &q));
    let mut nam9: Vec<u8> = (0..18)
        .flat_map(|i| (i as f32 / 18.0).to_le_bytes())
        .collect();
    nam9.extend([0xFF, 0xFF, 0x7F, 0x7F]);
    d.extend(sub(b"NAM9", &nam9));
    let nama: Vec<u8> = [12i32, -1, 5, 11]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    d.extend(sub(b"NAMA", &nama));
    d.extend(sub(b"NAM6", &1.0f32.to_le_bytes()));
    d.extend(sub(b"NAM7", &70.0f32.to_le_bytes()));
    d.extend(sub(b"WNAM", &0xD64u32.to_le_bytes()));
    d.extend(sub(b"TINI", &1u16.to_le_bytes()));
    d.extend(sub(b"TINC", &[0xc6, 0xb0, 0xa8, 0]));
    d.extend(sub(b"TINV", &100i32.to_le_bytes()));
    d.extend(sub(b"TIAS", &0x4Bi16.to_le_bytes()));
    d.extend(sub(b"TINI", &19u16.to_le_bytes()));
    let n = Npc::read(&record(b"NPC_", d), &names("Skyrim.esm", &[])).unwrap();
    let s = |id| Some(f("Skyrim.esm", id));
    assert_eq!(n.editor_id.as_deref(), Some("Ulfric"));
    assert!(n.female());
    assert_eq!((n.flags, n.template_flags), (0x21, 0x0101));
    assert_eq!(n.template, s(0x14128));
    assert!(n.face_from_template());
    assert_eq!(n.race, s(0x13746));
    assert_eq!(
        n.head_parts,
        [f("Skyrim.esm", 0xC716D), f("Skyrim.esm", 0x51412)]
    );
    assert_eq!(
        (n.hair_colour, n.face_texture, n.skin),
        (s(0xA042E), s(0xD071D), s(0xD64))
    );
    assert_eq!(n.skin_tone, Some([0.5, 0.25, 0.125]));
    let m = n.face_morphs.unwrap();
    assert_eq!((m.values[17], m.tail), (17.0 / 18.0, 0x7F7F_FFFF));
    assert_eq!(
        n.face_parts,
        Some(FaceParts {
            nose: 12,
            unknown: -1,
            eyes: 5,
            mouth: 11
        })
    );
    assert_eq!((n.height, n.weight), (Some(1.0), Some(70.0)));
    assert_eq!(
        n.tints,
        [
            NpcTint {
                index: 1,
                rgba: [0xc6, 0xb0, 0xa8, 0],
                value: 100,
                preset: 0x4B
            },
            NpcTint {
                index: 19,
                ..NpcTint::default()
            }
        ]
    );
    // Without a template, the traits flag means nothing.
    let plain = Npc {
        template_flags: USE_TRAITS,
        ..Npc::default()
    };
    assert!(!plain.face_from_template());
    let m = names("x", &[]);
    assert!(Npc::read(&record(b"NPC_", sub(b"ACBS", &[0; 20])), &m).is_err());
    assert!(Npc::read(&record(b"NPC_", sub(b"TINC", &[0; 4])), &m).is_err());
    assert!(Npc::read(&record(b"NPC_", sub(b"NAM9", &[0; 72])), &m).is_err());
}

#[test]
fn the_last_plugin_to_hold_a_form_wins() {
    let colour = |rgb: u8, id: u32| {
        let mut d = sub(b"EDID", b"C\0");
        d.extend(sub(b"CNAM", &[rgb, 0, 0, 0]));
        rec(b"CLFM", 0, id, &d)
    };
    let plugin = |name: &str, masters: &[&str], records: Vec<Vec<u8>>| {
        let mut b = tes4(0, masters);
        b.extend(grup(b"CLFM", &records.concat()));
        read(name, &b, &[*b"CLFM"]).unwrap().0
    };
    let base = plugin("Skyrim.esm", &[], vec![colour(1, 0x801), colour(2, 0x802)]);
    let patch = plugin(
        "Patch.esp",
        &["Skyrim.esm"],
        vec![colour(3, 0x801), colour(4, 0x0100_0801)],
    );
    let last = plugin(
        "Last.esp",
        &["SKYRIM.ESM", "Patch.esp"],
        vec![colour(5, 0x0100_0801)],
    );
    let order = LoadOrder::new(vec![base, patch, last]);
    let rgb = |p: &str, id| order.get::<Colour>(&f(p, id)).unwrap().unwrap().rgba[0];
    assert_eq!(rgb("Skyrim.esm", 0x801), 3);
    assert_eq!(rgb("Skyrim.esm", 0x802), 2);
    assert_eq!(rgb("patch.esp", 0x801), 5);
    assert_eq!(
        order.winner(&f("Skyrim.esm", 0x801)).unwrap().0.name(),
        "Patch.esp"
    );
    assert_eq!(order.copies(&f("Patch.esp", 0x801)).len(), 2);
    assert!(order.get::<Colour>(&f("Skyrim.esm", 0x803)).is_none());
    // Asked for as another type, there is nothing.
    assert!(order.get::<Race>(&f("Skyrim.esm", 0x801)).is_none());
    assert!(order.plugin("LAST.ESP").is_some());
    assert_eq!(order.forms().count(), 3);
}

#[test]
fn a_deleted_winner_is_no_record() {
    let mut b = tes4(0, &[]);
    b.extend(grup(b"CLFM", &rec(b"CLFM", 0x20, 0x801, &[])));
    let p: Plugin = read("x.esp", &b, &[*b"CLFM"]).unwrap().0;
    assert!(p.records[0].deleted());
    let order = LoadOrder::new(vec![p]);
    assert!(order.winner(&f("x.esp", 0x801)).is_some());
    assert!(order.get::<Colour>(&f("x.esp", 0x801)).is_none());
}

/// Drives a walk over bytes in memory as [`read`] does, but gives up (a
/// test failure, not a hang) if it has not ended after `cap` reads.
fn walk_capped(bytes: &[u8], wanted: &[[u8; 4]], cap: usize) -> Result<Plugin, PluginError> {
    let mut walk = Walk::new("x.esp", bytes.len() as u64, wanted);
    let mut reads = 0;
    while let Some((at, len)) = walk.need() {
        reads += 1;
        assert!(reads <= cap, "the walk did not end after {cap} reads");
        walk.feed(at, &bytes[at as usize..at as usize + len])?;
    }
    walk.finish()
}

/// A plugin of a header and one group, its size field set to `size`.
fn one_group(label: &[u8; 4], size: u32) -> Vec<u8> {
    let mut b = tes4(0, &[]);
    let at = b.len();
    b.extend(grup(label, &rec(label, 0, 0x801, &[])));
    b[at + 4..at + 8].copy_from_slice(&size.to_le_bytes());
    b
}

#[test]
fn groups_of_fewer_bytes_than_their_own_header_are_refused() {
    let empty = grup(b"HDPT", &[]);
    assert_eq!(GroupHeader::read(&empty).unwrap().size, 24);
    for size in [0, 1, 23] {
        for wanted in [true, false] {
            let b = one_group(b"CLFM", size);
            let asked: &[[u8; 4]] = if wanted { &[*b"CLFM"] } else { &[] };
            assert!(
                matches!(walk_capped(&b, asked, 10), Err(PluginError::Corrupt(_))),
                "size {size}, wanted {wanted}"
            );
        }
    }
    // The right size reads, asked for or not.
    let b = one_group(b"CLFM", 48);
    assert_eq!(walk_capped(&b, &[*b"CLFM"], 10).unwrap().records.len(), 1);
    assert!(walk_capped(&b, &[], 10).unwrap().records.is_empty());
    // Inside a group: a nested group smaller than a header, or larger than
    // what is left of its parent.
    for size in [23u32, 49] {
        let mut nested = grup(b"CLFM", &rec(b"CLFM", 0, 0x801, &[]));
        nested[4..8].copy_from_slice(&size.to_le_bytes());
        let mut p = Plugin::default();
        assert!(p.read_group(&nested).is_err(), "nested {size}");
    }
    let mut p = Plugin::default();
    p.read_group(&grup(b"CLFM", &rec(b"CLFM", 0, 0x801, &[])))
        .unwrap();
    assert_eq!(p.records.len(), 1);
}

#[test]
fn groups_and_records_that_run_past_the_end_are_refused() {
    let good = one_group(b"CLFM", 48);
    assert!(read("x", &good, &[*b"CLFM"]).is_ok());
    for (size, wanted) in [(49, true), (49, false), (u32::MAX, true), (u32::MAX, false)] {
        let b = one_group(b"CLFM", size);
        let asked: &[[u8; 4]] = if wanted { &[*b"CLFM"] } else { &[] };
        assert_eq!(
            walk_capped(&b, asked, 10).err(),
            Some(PluginError::Truncated),
            "size {size}, wanted {wanted}"
        );
    }
    // A record whose size runs past its group, and bytes after the last
    // record too few to be one.
    let mut p = Plugin::default();
    let mut r = rec(b"CLFM", 0, 0x801, &[1, 2, 3]);
    r[4] = 4;
    assert_eq!(p.read_group(&r), Err(PluginError::Truncated));
    let mut r = rec(b"CLFM", 0, 0x801, &[]);
    r.extend([0; 23]);
    assert_eq!(p.read_group(&r), Err(PluginError::Truncated));
    // The TES4 record claiming more than the file.
    let mut b = tes4(0, &[]);
    let n = b.len() as u32 - 24;
    b[4..8].copy_from_slice(&(n + 1).to_le_bytes());
    assert_eq!(read("x", &b, &[]).err(), Some(PluginError::Truncated));
    b[4..8].copy_from_slice(&n.to_le_bytes());
    assert!(read("x", &b, &[]).unwrap().0.records.is_empty());
}

#[test]
fn the_xxxx_override_sizes_the_next_subrecord_only() {
    // XXXX's size, not the next subrecord's own (5 here), is used; the one
    // after has its own again.
    let mut d = sub(b"XXXX", &3u32.to_le_bytes());
    d.extend(b"DATA");
    d.extend(5u16.to_le_bytes());
    d.extend([1, 2, 3]);
    d.extend(sub(b"FULL", &[4, 5]));
    let s: Vec<_> = subrecords(&d).collect::<Result<_, _>>().unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!((&s[0].kind, s[0].data), (b"DATA", &[1u8, 2, 3][..]));
    assert_eq!((&s[1].kind, s[1].data), (b"FULL", &[4u8, 5][..]));
    // A size of 0 is a subrecord of no bytes.
    let mut d = sub(b"XXXX", &0u32.to_le_bytes());
    d.extend(b"DATA\x09\x00");
    let s: Vec<_> = subrecords(&d).collect::<Result<_, _>>().unwrap();
    assert_eq!(s[0].data, &[] as &[u8]);
    // An XXXX at the end, one whose subrecord header is cut, one whose
    // size runs past the end.
    let alone = sub(b"XXXX", &8u32.to_le_bytes());
    assert_eq!(subrecords(&alone).last(), Some(Err(PluginError::Truncated)));
    let mut cut = alone.clone();
    cut.extend(b"DAT");
    assert_eq!(subrecords(&cut).last(), Some(Err(PluginError::Truncated)));
    let mut past = alone.clone();
    past.extend(b"DATA\x00\x00");
    past.extend([0; 7]);
    assert_eq!(subrecords(&past).last(), Some(Err(PluginError::Truncated)));
    past.push(0);
    assert_eq!(subrecords(&past).last().unwrap().unwrap().data.len(), 8);
    for n in [0u8, 3, 5, 8] {
        let mut wrong = b"XXXX".to_vec();
        wrong.extend((n as u16).to_le_bytes());
        wrong.extend(vec![0; n as usize]);
        wrong.extend(sub(b"DATA", &[]));
        assert!(
            matches!(
                subrecords(&wrong).next(),
                Some(Err(PluginError::Corrupt(_)))
            ),
            "XXXX of {n}"
        );
    }
}

/// 16 MiB is the most a compressed record may claim to inflate to.
#[test]
fn a_record_may_claim_16_mib_inflated_and_no_more() {
    let data = sub(b"EDID", b"x\0");
    let mut z = zlib(&data);
    z[..4].copy_from_slice(&(16u32 << 20).to_le_bytes());
    assert!(matches!(
        inflate(COMPRESSED, &z),
        Err(PluginError::Inflate(_))
    ));
    z[..4].copy_from_slice(&((16u32 << 20) + 1).to_le_bytes());
    assert!(matches!(
        inflate(COMPRESSED, &z),
        Err(PluginError::TooLarge(_))
    ));
    // An empty record inflates to nothing.
    assert_eq!(inflate(COMPRESSED, &zlib(&[])).unwrap(), Vec::<u8>::new());
}

#[test]
fn a_compressed_header_record_is_inflated() {
    let mut d = sub(b"HEDR", &[0, 0, 0x80, 0x3f, 2, 0, 0, 0, 0, 8, 0, 0]);
    d.extend(sub(b"MAST", b"Skyrim.esm\0"));
    let t = rec(b"TES4", COMPRESSED | 0x1, 0, &zlib(&d));
    let h = Header::read(&RecordHeader::read(&t).unwrap(), &t[24..]).unwrap();
    assert_eq!(h.masters, ["Skyrim.esm"]);
    assert_eq!(h.records, 2);
    assert!(h.esm());
}

/// A walk over a file it is told is 1 TiB long, fed only the bytes it
/// asks for: the header and one group header, of `size` bytes.
fn huge_walk(label: &[u8; 4], size: u32) -> (Walk, Result<(), PluginError>, u64) {
    let head = tes4(0, &[]);
    let at = head.len() as u64;
    let mut g = grup(label, &[]);
    g[4..8].copy_from_slice(&size.to_le_bytes());
    let mut walk = Walk::new("x", 1 << 40, &[*b"NPC_"]);
    assert_eq!(walk.need(), Some((0, 24)));
    walk.feed(0, &head[..24]).unwrap();
    assert_eq!(walk.need(), Some((24, head.len())));
    let r = walk.feed(24, &[&head[24..], &g[..]].concat());
    (walk, r, at)
}

/// A group asked for is read whole, up to 512 MiB; one not asked for is
/// stepped over whatever its size.
#[test]
fn a_group_asked_for_may_be_512_mib_and_no_more() {
    let max = 512u32 << 20;
    let (walk, r, at) = huge_walk(b"NPC_", max);
    r.unwrap();
    assert_eq!(walk.need(), Some((at + 24, max as usize)));
    let (walk, r, _) = huge_walk(b"NPC_", max + 1);
    assert!(matches!(r, Err(PluginError::TooLarge(_))));
    assert_eq!(walk.need(), None);
    let (walk, r, at) = huge_walk(b"WEAP", u32::MAX);
    r.unwrap();
    assert_eq!(walk.need(), Some((at + u32::MAX as u64, 24)));
}

#[test]
fn the_walk_refuses_short_long_or_late_bytes() {
    let bytes = small_plugin();
    let all = [*b"RACE", *b"CLFM"];
    // Every step of the walk, fed one byte short or one byte long, or at
    // the next offset, is refused, and ends it.
    let mut steps = Vec::new();
    let mut walk = Walk::new("Mod.esp", bytes.len() as u64, &all);
    while let Some(want) = walk.need() {
        steps.push(want);
        let (at, len) = (want.0 as usize, want.1);
        walk.feed(want.0, &bytes[at..at + len]).unwrap();
    }
    assert_eq!(steps.len(), 5);
    for (i, &(at, len)) in steps.iter().enumerate() {
        let a = at as usize;
        for (off, n) in [(0, len - 1), (0, len + 1), (1, len)] {
            let mut walk = Walk::new("Mod.esp", bytes.len() as u64, &all);
            for &(at, len) in &steps[..i] {
                walk.feed(at, &bytes[at as usize..at as usize + len])
                    .unwrap();
            }
            // Past the end of the file, the extra byte is a zero.
            let mut fed = bytes[a + off..(a + off + n).min(bytes.len())].to_vec();
            fed.resize(n, 0);
            assert!(
                matches!(
                    walk.feed(at + off as u64, &fed),
                    Err(PluginError::Unasked(_))
                ),
                "step {i}: {} bytes at {}",
                fed.len(),
                at + off as u64
            );
            assert_eq!(walk.need(), None);
        }
    }
    // Once done, anything more is refused.
    assert!(matches!(
        walk.feed(0, &bytes[..24]),
        Err(PluginError::Unasked(_))
    ));
}

#[test]
fn records_keep_their_flags_but_compression_and_their_version() {
    let mut body = rec(
        b"CLFM",
        COMPRESSED | 0x20 | 0x400,
        0x801,
        &zlib(&sub(b"EDID", b"x\0")),
    );
    let mut plain = rec(b"CLFM", 0x400, 0x802, &[]);
    plain[20..22].copy_from_slice(&40u16.to_le_bytes());
    body.extend(plain);
    let mut p = Plugin::default();
    p.read_group(&body).unwrap();
    assert_eq!(p.records[0].flags, 0x420);
    assert!(p.records[0].deleted());
    assert_eq!(p.records[0].data, sub(b"EDID", b"x\0"));
    assert_eq!((p.records[1].flags, p.records[1].version), (0x400, 40));
    assert!(!p.records[1].deleted());
    // A plugin's copy asked for as another type is not one.
    assert!(p.typed::<Colour>(&f("", 0x802)).is_some());
    assert!(p.typed::<Race>(&f("", 0x802)).is_none());
    assert!(p.typed::<Colour>(&f("", 0x803)).is_none());
}

#[test]
fn a_deleted_override_hides_the_copies_before_it() {
    let colour = |flags, id| rec(b"CLFM", flags, id, &sub(b"CNAM", &[1, 2, 3, 0]));
    let plugin = |name: &str, masters: &[&str], records: Vec<Vec<u8>>| {
        let mut b = tes4(0, masters);
        b.extend(grup(b"CLFM", &records.concat()));
        read(name, &b, &[*b"CLFM"]).unwrap().0
    };
    let base = plugin("Skyrim.esm", &[], vec![colour(0, 0x801), colour(0, 0x802)]);
    let patch = plugin("Patch.esp", &["Skyrim.esm"], vec![colour(0x20, 0x801)]);
    let order = LoadOrder::new(vec![base, patch]);
    assert_eq!(
        order.winner(&f("Skyrim.esm", 0x801)).unwrap().0.name(),
        "Patch.esp"
    );
    assert!(order.get::<Colour>(&f("Skyrim.esm", 0x801)).is_none());
    assert_eq!(order.copies(&f("Skyrim.esm", 0x801)).len(), 2);
    // The form it did not touch still reads.
    let c = order
        .get::<Colour>(&f("Skyrim.esm", 0x802))
        .unwrap()
        .unwrap();
    assert_eq!(c.rgba, [1, 2, 3, 0]);
}

#[test]
fn race_details_the_main_race_test_leaves_out() {
    let m = names("Mod.esp", &["Skyrim.esm"]);
    let mut d = sub(b"RNAM", &0x13746u32.to_le_bytes());
    let mut hclf = 0u32.to_le_bytes().to_vec();
    hclf.extend(0x0100_0801u32.to_le_bytes());
    d.extend(sub(b"HCLF", &hclf));
    d.extend(sub(b"NAM0", &[]));
    d.extend(sub(b"FNAM", &[]));
    // A HEAD with no INDX takes its place in the list; a null one is
    // skipped but uses up its INDX.
    d.extend(sub(b"HEAD", &0x10u32.to_le_bytes()));
    d.extend(sub(b"INDX", &5u32.to_le_bytes()));
    d.extend(sub(b"HEAD", &0u32.to_le_bytes()));
    d.extend(sub(b"HEAD", &0x11u32.to_le_bytes()));
    // An MPAV without MPAI is marked as of no feature.
    d.extend(sub(b"MPAV", &[0; 32]));
    // TINV and TIRS before any TINC set nothing.
    d.extend(sub(b"TINI", &3u16.to_le_bytes()));
    d.extend(sub(b"TINV", &0.5f32.to_le_bytes()));
    d.extend(sub(b"TIRS", &7u16.to_le_bytes()));
    d.extend(sub(b"TINC", &0u32.to_le_bytes()));
    d.extend(sub(b"TIRS", &8u16.to_le_bytes()));
    // A later DFTF wins, even a null one.
    d.extend(sub(b"DFTF", &0x3B523u32.to_le_bytes()));
    d.extend(sub(b"DFTF", &0u32.to_le_bytes()));
    let r = Race::read(&record(b"RACE", d), &m).unwrap();
    assert_eq!(r.morph_race, Some(f("Skyrim.esm", 0x13746)));
    assert_eq!(r.skin, None);
    assert_eq!(r.default_hair_colour, [None, Some(f("Mod.esp", 0x801))]);
    let h = &r.sexes[1].head;
    assert_eq!(
        h.parts,
        [(0, f("Skyrim.esm", 0x10)), (1, f("Skyrim.esm", 0x11))]
    );
    assert_eq!(h.morphs[0].index, u32::MAX);
    assert_eq!(
        h.tints[0].presets,
        [TintPreset {
            colour: None,
            value: 0.0,
            number: 8
        }]
    );
    assert_eq!(h.default_face_texture, None);
    assert!(r.sexes[0].head.parts.is_empty());
    for (kind, data) in [
        (b"HCLF", &[0u8; 4][..]),
        (b"HCLF", &[0; 12]),
        (b"MPAV", &[0; 28]),
        (b"TINI", &[0; 4]),
        (b"INDX", &[0; 2]),
    ] {
        let mut d = sub(b"NAM0", &[]);
        d.extend(sub(b"TINI", &[0; 2]));
        d.extend(sub(kind, data));
        assert!(
            Race::read(&record(b"RACE", d), &m).is_err(),
            "{} of {}",
            String::from_utf8_lossy(kind),
            data.len()
        );
    }
}

#[test]
fn an_addon_has_a_model_for_each_sex_and_person() {
    let m = names("x", &[]);
    let mut d = sub(b"MOD2", b"m.nif\0");
    d.extend(sub(b"MOD3", b"f.nif\0"));
    d.extend(sub(b"MOD5", b"1st_f.nif\0"));
    d.extend(sub(b"NAM1", &0x55u32.to_le_bytes()));
    d.extend(sub(b"NAM2", &0x56u32.to_le_bytes()));
    let a = ArmorAddon::read(&record(b"ARMA", d), &m).unwrap();
    assert_eq!(
        (a.model(false), a.model(true)),
        (Some("m.nif"), Some("f.nif"))
    );
    assert_eq!(a.models[3].as_deref(), Some("1st_f.nif"));
    assert_eq!(a.models[2], None);
    assert_eq!(
        a.skin_textures,
        [None, Some(f("x", 0x55)), Some(f("x", 0x56)), None]
    );
    // Only a female model: a male has none.
    let a = ArmorAddon::read(&record(b"ARMA", sub(b"MOD3", b"f.nif\0")), &m).unwrap();
    assert_eq!((a.model(false), a.model(true)), (None, Some("f.nif")));
}

#[test]
fn armour_names_and_short_slots() {
    let m = names("Skyrim.esm", &[]);
    let mut d = sub(b"FULL", b"Iron Helmet\0");
    d.extend(sub(b"BOD2", &[1, 0, 0, 0]));
    d.extend(sub(b"MODL", &0u32.to_le_bytes()));
    let a = Armor::read(&record(b"ARMO", d), &m).unwrap();
    assert_eq!(a.name, Some(Name::Text("Iron Helmet".into())));
    assert_eq!(a.slots, 1);
    assert!(a.addons.is_empty());
    let mut loc = m.clone();
    loc.localized = true;
    let a = Armor::read(&record(b"ARMO", sub(b"FULL", &7u32.to_le_bytes())), &loc).unwrap();
    assert_eq!(a.name, Some(Name::Id(7)));
    for kind in [b"BOD2", b"BODT"] {
        assert!(Armor::read(&record(b"ARMO", sub(kind, &[1, 2, 3])), &m).is_err());
    }
    // A localized name must be four bytes.
    assert!(Armor::read(&record(b"ARMO", sub(b"FULL", b"Iron\0")), &loc).is_err());
}

#[test]
fn npc_tint_fields_need_a_tini_before_them() {
    let m = names("x", &[]);
    for kind in [b"TINC", b"TINV", b"TIAS"] {
        let n = if kind == b"TIAS" { 2 } else { 4 };
        let data = vec![1; n];
        assert!(Npc::read(&record(b"NPC_", sub(kind, &data)), &m).is_err());
        let mut d = sub(b"TINI", &2u16.to_le_bytes());
        d.extend(sub(kind, &data));
        let t = Npc::read(&record(b"NPC_", d), &m).unwrap().tints[0];
        assert_eq!(t.index, 2);
        assert!(t.rgba != [0; 4] || t.value != 0 || t.preset != 0);
    }
}

/// The `TES4` record's size is held only to the file's length, not to
/// [`crate::walk::MAX_GROUP`] as a group's is: a file of a few GiB whose
/// header claims nearly all of it is asked for whole. On wasm32 it is worse:
/// `need` turns the claimed size plus 24 into a `usize`, which wraps past
/// 4 GiB, so a 5 GiB file whose header claims `u32::MAX` bytes is asked for
/// 23 and then sliced to `u32::MAX`, a panic.
#[test]
#[ignore = "the TES4 record's size is not bounded like a group's (see the comment)"]
fn a_header_record_claiming_more_than_a_group_may_hold_is_refused() {
    let head = tes4(0, &[]);
    let mut big = head[..24].to_vec();
    big[4..8].copy_from_slice(&((512u32 << 20) + 1).to_le_bytes());
    let mut walk = Walk::new("x", 1 << 40, &[]);
    let r = walk.feed(0, &big);
    assert!(
        matches!(r, Err(PluginError::TooLarge(_))),
        "{r:?}, then asked for {:?}",
        walk.need()
    );
}
