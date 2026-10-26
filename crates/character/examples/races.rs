//! Each race's editor id, morph race (`NAM8`), armour race (`RNAM`), flags
//! and keywords: what tells a vampire race from its living one.
mod common;
use apoc_character::Records;
fn main() {
    let order = common::load_order();
    let mut races: Vec<_> = order
        .forms()
        .filter(|f| order.kind(f) == Some(*b"RACE"))
        .cloned()
        .collect();
    races.sort_by_key(|f| (f.plugin.clone(), f.id));
    for f in races {
        let (p, r) = order.winner(&f).unwrap();
        let mut edid = String::new();
        let mut nam8 = None;
        let mut rnam = None;
        let mut flags = 0u32;
        let mut kw = vec![];
        for s in r.subrecords().flatten() {
            match &s.kind {
                b"EDID" => {
                    edid = String::from_utf8_lossy(s.data)
                        .trim_end_matches('\0')
                        .to_string()
                }
                b"NAM8" => {
                    nam8 = p
                        .names
                        .form(u32::from_le_bytes(s.data[..4].try_into().unwrap()))
                }
                b"RNAM" => {
                    rnam = p
                        .names
                        .form(u32::from_le_bytes(s.data[..4].try_into().unwrap()))
                }
                b"DATA" if s.data.len() >= 36 => {
                    flags = u32::from_le_bytes(s.data[32..36].try_into().unwrap())
                }
                b"KWDA" => {
                    for c in s.data.chunks(4) {
                        if let Some(k) = p.names.form(u32::from_le_bytes(c.try_into().unwrap())) {
                            kw.push(k);
                        }
                    }
                }
                _ => {}
            }
        }
        let name = |o: Option<apoc_plugin::FormRef>| {
            o.and_then(|x| order.race(&x))
                .and_then(|r| r.editor_id)
                .unwrap_or("-".into())
        };
        let kws: Vec<String> = kw
            .iter()
            .map(|k| {
                let ed = order
                    .winner(k)
                    .and_then(|(_, r)| {
                        r.subrecords()
                            .flatten()
                            .find(|s| &s.kind == b"EDID")
                            .map(|s| {
                                String::from_utf8_lossy(s.data)
                                    .trim_end_matches('\0')
                                    .to_string()
                            })
                    })
                    .unwrap_or_default();
                format!("{ed} ({}:{:06X})", k.plugin, k.id)
            })
            .collect();
        if edid.contains("Race")
            && (edid.contains("Elf")
                || edid.contains("Nord")
                || edid.contains("Elder")
                || edid.contains("Khajiit"))
        {
            println!(
                "{edid:28} nam8 {:24} rnam {:24} flags {flags:08x} kw {kws:?}",
                name(nam8),
                name(rnam)
            );
        }
    }
}
