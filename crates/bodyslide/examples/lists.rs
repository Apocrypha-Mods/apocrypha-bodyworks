//! Say what is in a slider data file: each list, how many vertices it names,
//! and how large its offsets are.
//!
//!     cargo run --release -p apoc-bodyslide --example lists -- <file.osd> [list name part]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = apoc_bodyslide::osd::SliderData::read(&std::fs::read(&args[0]).unwrap()).unwrap();
    for (name, diffs) in &data.lists {
        if args
            .get(1)
            .is_some_and(|part| !name.contains(part.as_str()))
        {
            continue;
        }
        let len = |d: &apoc_bodyslide::osd::Diff| {
            (d.offset[0].powi(2) + d.offset[1].powi(2) + d.offset[2].powi(2)).sqrt()
        };
        let zero = diffs.iter().filter(|d| len(d) == 0.0).count();
        let max = diffs.iter().map(len).fold(0.0f32, f32::max);
        let min = diffs.iter().map(len).fold(f32::MAX, f32::min);
        let top = diffs.iter().map(|d| d.vertex).max().unwrap_or(0);
        println!(
            "{name}: {} vertices (highest {top}), {zero} with no offset, lengths {min}..{max}",
            diffs.len()
        );
        if args.len() > 2 {
            for d in diffs.iter().take(8) {
                println!("    {} {:?}", d.vertex, d.offset);
            }
        }
    }
}
