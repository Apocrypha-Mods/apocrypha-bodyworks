//! The module driven as the interface drives it, over CBBE's real files
//! when they are in `scratch/` (skipped when not).

use apoc_bodyslide_app::answer;
use serde_json::{json, Value};
use std::path::Path;

fn ask(request: Value, data: &[u8]) -> (Value, Vec<u8>) {
    let a = answer(request.to_string().as_bytes(), data);
    let v: Value = serde_json::from_str(&a.json).unwrap();
    assert!(v.get("error").is_none(), "{v}");
    (v["ok"].clone(), a.bytes)
}

#[test]
fn a_body_is_listed_previewed_and_built() {
    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    if !tools.is_dir() {
        return;
    }
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
    let (catalog, _) = ask(json!({ "op": "load" }), &[]);
    let sets = catalog["sets"].as_array().unwrap();
    let body = sets
        .iter()
        .find(|s| s["name"] == "CBBE Body")
        .expect("CBBE Body");
    let sliders = body["sliders"].as_array().unwrap();
    assert!(sliders.len() > 100);
    // The categories give sliders the names people know them by.
    assert!(sliders
        .iter()
        .any(|s| s["shown"] == "Ankle Size" && s["category"] == "Seams"));
    assert!(catalog["presets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "CBBE Curvy"));

    // What it needs is asked for, handed over, and then needs nothing.
    let (needs, _) = ask(json!({ "op": "needs", "set": "CBBE Body" }), &[]);
    let needs: Vec<String> = serde_json::from_value(needs).unwrap();
    assert!(needs.iter().any(|p| p.ends_with("CBBE Body.nif")));
    for path in &needs {
        ask(
            json!({ "op": "put", "path": path }),
            &std::fs::read(tools.join(path)).unwrap(),
        );
    }
    assert_eq!(
        ask(json!({ "op": "needs", "set": "CBBE Body" }), &[]).0,
        json!([])
    );

    let (preview, numbers) = ask(json!({ "op": "preview", "set": "CBBE Body" }), &[]);
    let shape = &preview["shapes"][0];
    let (at, count) = (
        shape["positions"][0].as_u64().unwrap() as usize,
        shape["positions"][1].as_u64().unwrap() as usize,
    );
    assert_eq!(count, 13554 * 3);
    assert!((at + count) * 4 <= numbers.len());
    assert!(shape["sliders"].as_array().unwrap().len() > 100);
    // Every offset the JSON gives is inside the numbers that follow it.
    for s in shape["sliders"].as_array().unwrap() {
        for part in ["vertices", "offsets"] {
            let end = s[part][0].as_u64().unwrap() + s[part][1].as_u64().unwrap();
            assert!(end as usize * 4 <= numbers.len());
        }
    }

    let n = sliders.len();
    let small: Vec<f32> = sliders
        .iter()
        .map(|s| s["small"].as_f64().unwrap() as f32)
        .collect();
    let big: Vec<f32> = sliders
        .iter()
        .map(|s| s["big"].as_f64().unwrap() as f32)
        .collect();
    assert_eq!(small.len(), n);
    let (built, bytes) = ask(
        json!({ "op": "build", "set": "CBBE Body", "small": small, "big": big }),
        &[],
    );
    let outputs = built["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 2);
    assert_eq!(
        outputs[0]["path"],
        "meshes/actors/character/character assets/femalebody_0.nif"
    );
    let first = &bytes[..outputs[0]["length"].as_u64().unwrap() as usize];
    assert!(apoc_nif::Nif::read(first).is_ok());

    // One mesh as it stands, where it is asked to go.
    let (made, mesh) = ask(
        json!({ "op": "export", "set": "CBBE Body", "values": big, "path": "meshes\\mine/../Body.nif" }),
        &[],
    );
    assert_eq!(made["path"], "meshes/mine/Body.nif");
    assert!(apoc_nif::Nif::read(&mesh).is_ok());
    assert_eq!(
        mesh,
        &bytes[outputs[1]["at"].as_u64().unwrap() as usize..][..mesh.len()]
    );

    // With morphs asked for, the morph file comes too, after the meshes.
    let (built, bytes) = ask(
        json!({ "op": "build", "set": "CBBE Body", "small": small, "big": big, "morphs": true }),
        &[],
    );
    let outputs = built["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 3);
    assert_eq!(
        outputs[2]["path"],
        "meshes/actors/character/character assets/femalebody.tri"
    );
    let at = outputs[2]["at"].as_u64().unwrap() as usize;
    assert_eq!(&bytes[at..at + 4], b"PIRT");
    let whole = &bytes[..outputs[0]["length"].as_u64().unwrap() as usize];

    // The same set as a project: changed, the changes taken back, saved.
    let (work, _) = ask(json!({ "op": "open", "set": "CBBE Body" }), &[]);
    assert_eq!(work["changed"], false);
    assert_eq!(work["sliders"].as_array().unwrap().len(), n);
    let first = work["shapes"][0].as_str().unwrap().to_string();
    let (work, _) = ask(
        json!({ "op": "change", "change": { "do": "renameShape", "from": first, "to": "Skin" } }),
        &[],
    );
    assert_eq!(work["shapes"][0], "Skin");
    assert_eq!(work["changed"], true);
    let (shown, _) = ask(json!({ "op": "preview", "set": "", "work": true }), &[]);
    assert_eq!(shown["shapes"][0]["name"], "Skin");
    assert!(shown["shapes"][0]["sliders"].as_array().unwrap().len() > 100);
    let gone = sliders[0]["name"].as_str().unwrap();
    let (work, _) = ask(
        json!({ "op": "change", "change": { "do": "deleteSlider", "name": gone } }),
        &[],
    );
    assert_eq!(work["sliders"].as_array().unwrap().len(), n - 1);
    // A change that cannot be made says why and changes nothing.
    let a = answer(
        json!({ "op": "change", "change": { "do": "addSlider", "name": sliders[1]["name"] } })
            .to_string()
            .as_bytes(),
        &[],
    );
    assert!(a.json.contains("already a slider"), "{}", a.json);
    let (work, _) = ask(json!({ "op": "undo" }), &[]);
    assert_eq!(work["sliders"].as_array().unwrap().len(), n);
    assert_eq!(work["redo"], true);
    let (work, _) = ask(json!({ "op": "undo" }), &[]);
    assert_eq!(work["shapes"][0], first.as_str());
    assert_eq!(work["undo"], false);

    // Under its own name it would be taken for the set it came from.
    let a = answer(
        json!({ "op": "save", "name": "CBBE Body" })
            .to_string()
            .as_bytes(),
        &[],
    );
    assert!(a.json.contains("already a project"), "{}", a.json);
    let (saved, files) = ask(json!({ "op": "save", "name": "Mine: a body" }), &[]);
    let paths: Vec<&str> = saved["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "SliderSets/Mine_ a body.osp",
            "ShapeData/Mine_ a body/Mine_ a body.nif",
            "ShapeData/Mine_ a body/Mine_ a body.osd"
        ]
    );
    assert!(files.starts_with(b"<?xml"));
    // It is a set like any other from then on, and builds what the set it
    // was opened from builds, somewhere else.
    let (catalog, _) = ask(json!({ "op": "load" }), &[]);
    assert!(catalog["sets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "Mine: a body"));
    assert_eq!(
        ask(json!({ "op": "needs", "set": "Mine: a body" }), &[]).0,
        json!([])
    );
    let (built, bytes) = ask(
        json!({ "op": "build", "set": "Mine: a body", "small": small, "big": big, "morphs": true }),
        &[],
    );
    let length = built["outputs"][0]["length"].as_u64().unwrap() as usize;
    assert_eq!(&bytes[..length], whole);
    // Saving again under the name it now has is not refused.
    ask(json!({ "op": "save", "name": "Mine: a body" }), &[]);
}

#[test]
fn a_request_that_cannot_be_answered_says_why() {
    let a = answer(b"not json", &[]);
    assert!(a.json.contains("error"));
    let a = answer(
        json!({ "op": "preview", "set": "No such set" })
            .to_string()
            .as_bytes(),
        &[],
    );
    assert!(a.json.contains("no set named"));
}
