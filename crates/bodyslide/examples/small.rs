//! The smallest offsets in the lists of a slider data file whose name has a
//! given part: where "nothing" ends for a list that marks vertices.
//!
//!     cargo run --release -p apoc-bodyslide --example small -- <file.osd> <list name part>

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = apoc_bodyslide::osd::SliderData::read(&std::fs::read(&args[0]).unwrap()).unwrap();
    for (name, diffs) in &data.lists {
        if !name.contains(args[1].as_str()) {
            continue;
        }
        let mut lens: Vec<(f32, [f32; 3])> = diffs
            .iter()
            .map(|d| {
                (
                    (d.offset[0].powi(2) + d.offset[1].powi(2) + d.offset[2].powi(2)).sqrt(),
                    d.offset,
                )
            })
            .filter(|l| l.0 > 0.0)
            .collect();
        lens.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!("{name}: {} marked; smallest:", lens.len());
        for l in lens.iter().take(14) {
            println!("    {:e}  {:?}", l.0, l.1);
        }
    }
}
