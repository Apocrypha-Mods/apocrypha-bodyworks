//! Print the subrecords of records of one type, for checking layouts.
//!
//!     cargo run --release -p apoc-plugin --example dump -- <plugin> <TYPE> [editor id]

use apoc_plugin::native::read_file;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let kind: [u8; 4] = a[2].as_bytes().try_into().unwrap();
    let (p, s) = read_file(a[1].as_ref(), &[kind]).unwrap();
    eprintln!(
        "{s:?} {} records, masters {:?}",
        p.records.len(),
        p.header.masters
    );
    for r in &p.records {
        let edid = r
            .subrecords()
            .flatten()
            .find(|s| &s.kind == b"EDID")
            .map(|s| {
                String::from_utf8_lossy(s.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if a.get(3).is_some_and(|e| !edid.eq_ignore_ascii_case(e)) {
            continue;
        }
        println!("{:08X} {edid}", r.id);
        for s in r.subrecords() {
            let s = s.unwrap();
            let k = String::from_utf8_lossy(&s.kind);
            let printable = s.data.len() > 1
                && s.data[..s.data.len() - 1]
                    .iter()
                    .all(|c| (0x20..0x7f).contains(c));
            if printable {
                println!("  {k} {:?}", String::from_utf8_lossy(s.data));
            } else {
                let hex: String = s
                    .data
                    .iter()
                    .take(48)
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  {k} [{}] {hex}", s.data.len());
            }
        }
    }
}
