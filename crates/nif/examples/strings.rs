//! Print a mesh file's string table, and each NiStringExtraData's name and value.
//!
//!     cargo run --release -p apoc-nif --example strings -- <file.nif>

fn main() {
    for path in std::env::args().skip(1) {
        let nif = apoc_nif::Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        let text = |i: i32| {
            nif.string(i)
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .unwrap_or_default()
        };
        println!("{path}");
        for (i, s) in nif.header.strings.iter().enumerate() {
            println!("  {i}: {}", String::from_utf8_lossy(s));
        }
        for i in 0..nif.blocks.len() {
            if nif.kind_of(i) == "NiStringExtraData" {
                let d = &nif.blocks[i].data;
                let at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
                println!("  block {i}: {:?} = {:?}", text(at(0)), text(at(4)));
            }
        }
    }
}
