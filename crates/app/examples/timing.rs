//! How long the module takes over what the interface asks of it, on a real
//! set: the numbers a slow screen is traced back to.
//!
//!     cargo run --release -p apoc-bodyslide-app --example timing -- <BodySlide folder> [set name]

use apoc_bodyslide_app::answer;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

fn ask(request: Value, data: &[u8]) -> (Value, Vec<u8>) {
    let a = answer(request.to_string().as_bytes(), data);
    let v: Value = serde_json::from_str(&a.json).unwrap();
    assert!(v.get("error").is_none(), "{v}");
    (v["ok"].clone(), a.bytes)
}

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{:>9.2} ms  {what}", start.elapsed().as_secs_f64() * 1000.0);
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let tools = args.next().expect("the BodySlide folder");
    let set = args.next().unwrap_or_else(|| "CBBE Body".to_string());
    let tools = Path::new(&tools);
    timed(
        "every project, preset, group and category put and read",
        || {
            for folder in [
                "SliderSets",
                "SliderPresets",
                "SliderGroups",
                "SliderCategories",
            ] {
                for e in std::fs::read_dir(tools.join(folder))
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let name = e.file_name().to_string_lossy().into_owned();
                    ask(
                        json!({ "op": "put", "path": format!("{folder}/{name}") }),
                        &std::fs::read(e.path()).unwrap(),
                    );
                }
            }
            ask(json!({ "op": "load" }), &[]);
        },
    );
    let (needs, _) = ask(json!({ "op": "needs", "set": set }), &[]);
    let mut bytes = 0;
    for path in needs.as_array().unwrap() {
        let path = path.as_str().unwrap();
        let data = std::fs::read(tools.join(path)).unwrap();
        bytes += data.len();
        ask(json!({ "op": "put", "path": path }), &data);
    }
    println!(
        "{set}: {} files, {:.1} MB",
        needs.as_array().unwrap().len(),
        bytes as f64 / 1e6
    );
    let (preview, numbers) = timed("preview of the set", || {
        ask(json!({ "op": "preview", "set": set }), &[])
    });
    let shape = preview["shapes"][0]["name"].as_str().unwrap().to_string();
    println!(
        "          {} shapes, {:.1} MB of numbers",
        preview["shapes"].as_array().unwrap().len(),
        numbers.len() as f64 / 1e6
    );
    let (work, _) = timed("open as a project", || {
        ask(json!({ "op": "open", "set": set }), &[])
    });
    let n = work["sliders"].as_array().unwrap().len();
    timed("preview of the project", || {
        ask(json!({ "op": "preview", "set": "", "work": true }), &[])
    });
    // A stroke over 400 vertices, on the shape and on a slider.
    let mut stroke = Vec::new();
    for v in 0..400u32 {
        stroke.extend(v.to_le_bytes());
        for c in [0.0f32, 0.1, 0.2] {
            stroke.extend(c.to_le_bytes());
        }
    }
    timed("a stroke on the shape", || {
        ask(
            json!({ "op": "change", "change": { "do": "stroke", "shape": shape } }),
            &stroke,
        )
    });
    let slider = work["sliders"][0]["name"].as_str().unwrap().to_string();
    timed("a stroke on a slider", || {
        ask(
            json!({ "op": "change", "change": { "do": "stroke", "shape": shape, "slider": slider } }),
            &stroke,
        )
    });
    timed("a slider renamed", || {
        ask(
            json!({ "op": "change", "change": { "do": "renameSlider", "from": slider, "to": "Renamed for timing" } }),
            &[],
        )
    });
    timed("undo", || ask(json!({ "op": "undo" }), &[]));
    let values = vec![50.0f32; n];
    timed("build, both weights, with morphs", || {
        ask(
            json!({ "op": "build", "set": set, "small": values, "big": values, "morphs": true }),
            &[],
        )
    });
    timed("save the project", || {
        ask(json!({ "op": "save", "name": "Timing" }), &[])
    });
}
