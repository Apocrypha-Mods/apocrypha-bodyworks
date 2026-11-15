//! A typed, read-only view of the parts of a preset a character editor
//! needs: who wrote it, the actor's weight, hair colour and head texture,
//! the head parts, the plugins it names, the face sliders and presets,
//! RaceMenu's named sliders, and the tint layers.
//!
//! Every field is a [`Field`]: what was read, or a [`Missing`] saying where
//! and why it could not be. One field that is absent or of the wrong kind
//! never stops the rest from being read, and keys this view does not know
//! are left alone. Sculpting, overrides, transforms, face textures and body
//! morphs are not typed here; they stay in the [`Document`] as read.
//!
//! The view does not change anything. Edits go through the document.

use crate::value::{Document, Object, Value};
use std::fmt;

/// What was read for one field, or why it could not be.
pub type Field<T> = Result<T, Missing>;

/// A field that could not be read: where it is (`actor.weight`,
/// `headParts[2].formIdentifier`) and what was wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub path: String,
    pub problem: Problem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The key is not there.
    Absent,
    /// Something is there, but not what was expected, which this says
    /// ("a whole number from 0 to 4294967295").
    Expected(&'static str),
}

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "the preset"
        } else {
            &self.path
        };
        match self.problem {
            Problem::Absent => write!(f, "{path} is missing"),
            Problem::Expected(what) => write!(f, "{path} is not {what}"),
        }
    }
}

/// A form named by its plugin and its id within it, written `Plugin|ID` in
/// a preset (`UBE_AllRace.esp|011D49`: the id is in hex, six digits and
/// upper case in every one seen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormRef {
    pub plugin: String,
    /// The form's id within its plugin, 24 bits: the load-order byte is not
    /// part of it.
    pub id: u32,
}

impl FormRef {
    /// Reads `Plugin|ID`. The id may be one to six hex digits in either
    /// case; the plugin is what comes before the last `|` and may not be
    /// empty.
    pub fn parse(text: &str) -> Option<FormRef> {
        let (plugin, id) = text.rsplit_once('|')?;
        if plugin.is_empty() || id.is_empty() || id.len() > 6 {
            return None;
        }
        if !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(FormRef {
            plugin: plugin.to_string(),
            id: u32::from_str_radix(id, 16).ok()?,
        })
    }
}

/// `Plugin|ID` with the id as six upper-case hex digits, as every one in the
/// sample is written (`|011D49`). An id beyond 24 bits, which
/// [`FormRef::parse`] never gives, would take more digits.
impl fmt::Display for FormRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{:06X}", self.plugin, self.id)
    }
}

/// `version`: which format the preset is in and what wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    /// `formatVersion`, 3 in the sample.
    pub format: Field<u32>,
    /// `runtimeVersion`: the game's version as SKSE packs it.
    pub runtime: Field<u32>,
    /// `skseVersion`: SKSE's version, packed the same way.
    pub skse: Field<u32>,
    /// `signature`: 1163086675 in the sample, `SKSE` read as a
    /// little-endian number.
    pub signature: Field<u32>,
}

/// `actor`.
#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    /// `weight`, 0 to 100.
    pub weight: Field<f64>,
    /// `hairColor`, a packed colour.
    pub hair_color: Field<u32>,
    /// `headTexture`, the texture set on the head.
    pub head_texture: Field<FormRef>,
}

/// One of `headParts`.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadPart {
    /// `type`: the head part's kind, as the game numbers them.
    pub kind: Field<u32>,
    /// `formId`: the part's id in the load order the preset was saved with.
    pub form_id: Field<u32>,
    /// `formIdentifier`: the part by its plugin, whatever the load order.
    pub form_identifier: Field<FormRef>,
}

/// One of `mods`: a plugin and its place in the load order.
#[derive(Debug, Clone, PartialEq)]
pub struct Mod {
    pub index: Field<u32>,
    pub name: Field<String>,
}

/// `morphs.default`: the game's own face sliders.
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultMorphs {
    /// `morphs`: the game's face sliders, 19 in the sample, as read whatever
    /// their number.
    pub morphs: Field<Vec<Field<f64>>>,
    /// `presets`: the face presets (nose, unused, eyes, lips in the sample's
    /// order of four), 4294967295 for none.
    pub presets: Field<Vec<Field<u32>>>,
}

/// One of `morphs.custom`: a RaceMenu slider by name.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomMorph {
    pub name: Field<String>,
    pub value: Field<f64>,
}

/// One of `tintInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct Tint {
    pub index: Field<u32>,
    /// `color`: alpha in the top byte, then red, green and blue.
    pub color: Field<u32>,
    /// `texture`: the tint mask's path.
    pub texture: Field<String>,
}

/// The typed view of a preset.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub version: Field<Version>,
    pub actor: Field<Actor>,
    pub head_parts: Field<Vec<Field<HeadPart>>>,
    /// `modNames`.
    pub mod_names: Field<Vec<Field<String>>>,
    pub mods: Field<Vec<Field<Mod>>>,
    /// `morphs.default`.
    pub default_morphs: Field<DefaultMorphs>,
    /// `morphs.custom`, in the preset's order.
    pub custom_morphs: Field<Vec<Field<CustomMorph>>>,
    /// `tintInfo`, in the preset's order.
    pub tints: Field<Vec<Field<Tint>>>,
}

impl Preset {
    /// Reads the view from a document. It never fails as a whole: whatever
    /// cannot be read is a [`Missing`] in its own field.
    pub fn of(doc: &Document) -> Preset {
        let top = object(&doc.root, "");
        let section = |key: &str| top.clone().and_then(|o| member(o, key, ""));
        let morphs = section("morphs").and_then(|v| object(v, "morphs"));
        let in_morphs = |key: &str| morphs.clone().and_then(|o| member(o, key, "morphs"));
        Preset {
            version: section("version").and_then(|v| {
                let o = object(v, "version")?;
                Ok(Version {
                    format: get(o, "formatVersion", "version", whole),
                    runtime: get(o, "runtimeVersion", "version", whole),
                    skse: get(o, "skseVersion", "version", whole),
                    signature: get(o, "signature", "version", whole),
                })
            }),
            actor: section("actor").and_then(|v| {
                let o = object(v, "actor")?;
                Ok(Actor {
                    weight: get(o, "weight", "actor", number),
                    hair_color: get(o, "hairColor", "actor", whole),
                    head_texture: get(o, "headTexture", "actor", form_ref),
                })
            }),
            head_parts: section("headParts").and_then(|v| {
                list(v, "headParts", |o, p| HeadPart {
                    kind: get(o, "type", p, whole),
                    form_id: get(o, "formId", p, whole),
                    form_identifier: get(o, "formIdentifier", p, form_ref),
                })
            }),
            mod_names: section("modNames").and_then(|v| scalars(v, "modNames", text)),
            mods: section("mods").and_then(|v| {
                list(v, "mods", |o, p| Mod {
                    index: get(o, "index", p, whole),
                    name: get(o, "name", p, text),
                })
            }),
            default_morphs: in_morphs("default").and_then(|v| {
                let o = object(v, "morphs.default")?;
                let p = "morphs.default";
                Ok(DefaultMorphs {
                    morphs: member(o, "morphs", p)
                        .and_then(|v| scalars(v, "morphs.default.morphs", number)),
                    presets: member(o, "presets", p)
                        .and_then(|v| scalars(v, "morphs.default.presets", whole)),
                })
            }),
            custom_morphs: in_morphs("custom").and_then(|v| {
                list(v, "morphs.custom", |o, p| CustomMorph {
                    name: get(o, "name", p, text),
                    value: get(o, "value", p, number),
                })
            }),
            tints: section("tintInfo").and_then(|v| {
                list(v, "tintInfo", |o, p| Tint {
                    index: get(o, "index", p, whole),
                    color: get(o, "color", p, whole),
                    texture: get(o, "texture", p, text),
                })
            }),
        }
    }

    /// Everything that could not be read, in the order of the fields. A
    /// section that is missing or wrong is one entry, not one per field in
    /// it, and sections that fail for one shared reason (a top that is not
    /// an object, a missing `morphs`) give that reason once.
    pub fn problems(&self) -> Vec<&Missing> {
        self.problems_up_to(usize::MAX).0
    }

    /// The first `limit` of [`Preset::problems`], and how many there are in
    /// all, without listing the rest: a hostile preset can have millions.
    pub fn problems_up_to(&self, limit: usize) -> (Vec<&Missing>, usize) {
        let mut out = Problems {
            list: Vec::new(),
            last: None,
            total: 0,
            limit,
        };
        if let Some(v) = check(&self.version, &mut out) {
            check(&v.format, &mut out);
            check(&v.runtime, &mut out);
            check(&v.skse, &mut out);
            check(&v.signature, &mut out);
        }
        if let Some(a) = check(&self.actor, &mut out) {
            check(&a.weight, &mut out);
            check(&a.hair_color, &mut out);
            check(&a.head_texture, &mut out);
        }
        for h in each(&self.head_parts, &mut out) {
            check(&h.kind, &mut out);
            check(&h.form_id, &mut out);
            check(&h.form_identifier, &mut out);
        }
        for _ in each(&self.mod_names, &mut out) {}
        for m in each(&self.mods, &mut out) {
            check(&m.index, &mut out);
            check(&m.name, &mut out);
        }
        if let Some(d) = check(&self.default_morphs, &mut out) {
            for _ in each(&d.morphs, &mut out) {}
            for _ in each(&d.presets, &mut out) {}
        }
        for c in each(&self.custom_morphs, &mut out) {
            check(&c.name, &mut out);
            check(&c.value, &mut out);
        }
        for t in each(&self.tints, &mut out) {
            check(&t.index, &mut out);
            check(&t.color, &mut out);
            check(&t.texture, &mut out);
        }
        (out.list, out.total)
    }
}

/// Problems as they are found: the first `limit` kept, all counted.
struct Problems<'a> {
    list: Vec<&'a Missing>,
    last: Option<&'a Missing>,
    total: usize,
    limit: usize,
}

impl<'a> Problems<'a> {
    fn push(&mut self, m: &'a Missing) {
        // Problems are pushed in field order and every field has its own
        // path, so a repeat can only be the shared reason of sections read
        // through the same parent.
        if self.last == Some(m) {
            return;
        }
        self.last = Some(m);
        self.total += 1;
        if self.list.len() < self.limit {
            self.list.push(m);
        }
    }
}

/// The value, or nothing with its problem noted.
fn check<'a, T>(field: &'a Field<T>, out: &mut Problems<'a>) -> Option<&'a T> {
    match field {
        Ok(v) => Some(v),
        Err(m) => {
            out.push(m);
            None
        }
    }
}

/// The entries of a list that could be read, noting the problem with the
/// list or with any entry.
fn each<'a, T>(field: &'a Field<Vec<Field<T>>>, out: &mut Problems<'a>) -> Vec<&'a T> {
    let Some(items) = check(field, out) else {
        return Vec::new();
    };
    items.iter().filter_map(|i| check(i, out)).collect()
}

fn join(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

fn member<'a>(o: &'a Object, key: &str, parent: &str) -> Field<&'a Value> {
    o.get(key).ok_or_else(|| Missing {
        path: join(parent, key),
        problem: Problem::Absent,
    })
}

fn expected(path: &str, what: &'static str) -> Missing {
    Missing {
        path: path.to_string(),
        problem: Problem::Expected(what),
    }
}

fn object<'a>(v: &'a Value, path: &str) -> Field<&'a Object> {
    v.as_object().ok_or_else(|| expected(path, "an object"))
}

/// A member of `o`, read by `read`.
fn get<T>(o: &Object, key: &str, parent: &str, read: fn(&Value, &str) -> Field<T>) -> Field<T> {
    let path = join(parent, key);
    o.get(key)
        .ok_or_else(|| Missing {
            path: path.clone(),
            problem: Problem::Absent,
        })
        .and_then(|v| read(v, &path))
}

/// A list of objects, each read by `read` with its own path.
fn list<T>(v: &Value, path: &str, read: impl Fn(&Object, &str) -> T) -> Field<Vec<Field<T>>> {
    let items = v.as_array().ok_or_else(|| expected(path, "a list"))?;
    Ok(items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let p = format!("{path}[{i}]");
            object(item, &p).map(|o| read(o, &p))
        })
        .collect())
}

/// A list of scalars, each read by `read`.
fn scalars<T>(v: &Value, path: &str, read: fn(&Value, &str) -> Field<T>) -> Field<Vec<Field<T>>> {
    let items = v.as_array().ok_or_else(|| expected(path, "a list"))?;
    Ok(items
        .iter()
        .enumerate()
        .map(|(i, item)| read(item, &format!("{path}[{i}]")))
        .collect())
}

fn whole(v: &Value, path: &str) -> Field<u32> {
    v.as_number()
        .and_then(|n| n.as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| expected(path, "a whole number from 0 to 4294967295"))
}

/// A number RaceMenu keeps as a float: it must be finite and within a
/// float's range once rounded to one. The sample's `3.402823466385289e+38`
/// is a little above the largest float as a double, and rounds to it.
fn number(v: &Value, path: &str) -> Field<f64> {
    let n = v
        .as_number()
        .ok_or_else(|| expected(path, "a number"))?
        .as_f64();
    if (n as f32).is_finite() {
        Ok(n)
    } else {
        Err(expected(path, "a number within a float's range"))
    }
}

fn text(v: &Value, path: &str) -> Field<String> {
    v.as_str()
        .ok_or_else(|| expected(path, "text"))?
        .text()
        .ok_or_else(|| expected(path, "UTF-8 text"))
}

fn form_ref(v: &Value, path: &str) -> Field<FormRef> {
    let t = text(v, path)?;
    FormRef::parse(&t).ok_or_else(|| expected(path, "a form as Plugin|ID"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const WHOLE: Problem = Problem::Expected("a whole number from 0 to 4294967295");

    fn read(text: &str) -> Preset {
        Preset::of(&parse(text.as_bytes()).unwrap())
    }

    fn missing(path: &str, problem: Problem) -> Missing {
        Missing {
            path: path.to_string(),
            problem,
        }
    }

    const FULL: &str = r#"{
        "actor" : { "hairColor" : 1193046, "headTexture" : "Some Race.esp|0A0b0C", "weight" : 50.5 },
        "headParts" : [
            { "formId" : 4278190081, "formIdentifier" : "Hair.esp|000801", "type" : 3 },
            { "formId" : 1, "formIdentifier" : "Skyrim.esm|1", "type" : 1, "extra" : [] }
        ],
        "modNames" : [ "Hair.esp", "Some Race.esp" ],
        "mods" : [ { "index" : 2, "name" : "Hair.esp" } ],
        "morphs" : {
            "custom" : [ { "name" : "Slider A", "value" : -0.25 } ],
            "default" : { "morphs" : [ 0, -1, 0.5 ], "presets" : [ 1, 4294967295, 0, 2 ] },
            "sculpt" : []
        },
        "tintInfo" : [ { "color" : 4278255360, "index" : 7, "texture" : "Masks\\Cheek.dds" } ],
        "version" : { "formatVersion" : 3, "runtimeVersion" : 10, "signature" : 20, "skseVersion" : 30 },
        "unknown" : { "kept" : true }
    }"#;

    #[test]
    fn a_whole_preset_is_read_into_its_fields() {
        let p = read(FULL);
        assert!(p.problems().is_empty(), "{:?}", p.problems());
        let v = p.version.unwrap();
        assert_eq!(
            (v.format, v.runtime, v.signature, v.skse),
            (Ok(3), Ok(10), Ok(20), Ok(30))
        );
        let a = p.actor.unwrap();
        assert_eq!(a.weight, Ok(50.5));
        assert_eq!(a.hair_color, Ok(0x123456));
        assert_eq!(
            a.head_texture,
            Ok(FormRef {
                plugin: "Some Race.esp".into(),
                id: 0x0A0B0C
            })
        );
        let h = p.head_parts.unwrap();
        assert_eq!(h.len(), 2);
        let h0 = h[0].as_ref().unwrap();
        assert_eq!(h0.kind, Ok(3));
        assert_eq!(h0.form_id, Ok(0xFF000001));
        assert_eq!(h0.form_identifier.as_ref().unwrap().id, 0x801);
        assert_eq!(
            h[1].as_ref().unwrap().form_identifier.as_ref().unwrap().id,
            1
        );
        assert_eq!(
            p.mod_names.unwrap(),
            vec![Ok("Hair.esp".into()), Ok("Some Race.esp".into())]
        );
        let m = p.mods.unwrap();
        assert_eq!(m[0].as_ref().unwrap().index, Ok(2));
        let d = p.default_morphs.unwrap();
        assert_eq!(d.morphs.unwrap(), vec![Ok(0.0), Ok(-1.0), Ok(0.5)]);
        assert_eq!(d.presets.unwrap(), vec![Ok(1), Ok(u32::MAX), Ok(0), Ok(2)]);
        let c = p.custom_morphs.unwrap();
        assert_eq!(c[0].as_ref().unwrap().name, Ok("Slider A".into()));
        assert_eq!(c[0].as_ref().unwrap().value, Ok(-0.25));
        let t = p.tints.unwrap();
        let t0 = t[0].as_ref().unwrap();
        assert_eq!(
            (t0.index.clone(), t0.color.clone()),
            (Ok(7), Ok(0xFF00FF00))
        );
        assert_eq!(t0.texture, Ok("Masks\\Cheek.dds".into()));
    }

    #[test]
    fn missing_sections_and_fields_are_reported_one_by_one() {
        let p = read(
            r#"{ "actor" : { "weight" : "heavy" }, "morphs" : {}, "headParts" : [ 5, { "type" : -1 } ] }"#,
        );
        assert_eq!(
            p.problems(),
            vec![
                &missing("version", Problem::Absent),
                &missing("actor.weight", Problem::Expected("a number")),
                &missing("actor.hairColor", Problem::Absent),
                &missing("actor.headTexture", Problem::Absent),
                &missing("headParts[0]", Problem::Expected("an object")),
                &missing("headParts[1].type", WHOLE),
                &missing("headParts[1].formId", Problem::Absent),
                &missing("headParts[1].formIdentifier", Problem::Absent),
                &missing("modNames", Problem::Absent),
                &missing("mods", Problem::Absent),
                &missing("morphs.default", Problem::Absent),
                &missing("morphs.custom", Problem::Absent),
                &missing("tintInfo", Problem::Absent),
            ]
        );
    }

    #[test]
    fn bad_entries_in_lists_do_not_lose_the_others() {
        let p = read(
            r#"{ "modNames" : [ "A.esp", 1, "B.esp" ], "morphs" : { "default" : { "morphs" : [ 0, "x" ], "presets" : 7 } } }"#,
        );
        let names = p.mod_names.unwrap();
        assert_eq!(names[0], Ok("A.esp".into()));
        assert_eq!(
            names[1],
            Err(missing("modNames[1]", Problem::Expected("text")))
        );
        assert_eq!(names[2], Ok("B.esp".into()));
        let d = p.default_morphs.unwrap();
        assert_eq!(
            d.morphs.unwrap()[1],
            Err(missing(
                "morphs.default.morphs[1]",
                Problem::Expected("a number")
            ))
        );
        assert_eq!(
            d.presets,
            Err(missing(
                "morphs.default.presets",
                Problem::Expected("a list")
            ))
        );
    }

    #[test]
    fn numbers_out_of_range_or_with_fractions_are_not_whole() {
        let p = read(
            r#"{ "version" : { "formatVersion" : 4294967296, "runtimeVersion" : 3.0, "signature" : -1, "skseVersion" : 1e3 } }"#,
        );
        let v = p.version.unwrap();
        assert_eq!(v.format.unwrap_err().problem, WHOLE);
        assert_eq!(v.runtime.unwrap_err().problem, WHOLE);
        assert_eq!(v.signature.unwrap_err().problem, WHOLE);
        assert_eq!(v.skse.unwrap_err().problem, WHOLE);
    }

    #[test]
    fn a_top_that_is_not_an_object_is_one_problem() {
        let p = read("[]");
        assert_eq!(p.version, Err(missing("", Problem::Expected("an object"))));
        assert_eq!(p.tints, Err(missing("", Problem::Expected("an object"))));
        let all = p.problems();
        assert_eq!(all, vec![&missing("", Problem::Expected("an object"))]);
        assert_eq!(all[0].to_string(), "the preset is not an object");
    }

    #[test]
    fn text_that_is_not_utf8_is_reported() {
        let p = Preset::of(&parse(b"{ \"modNames\" : [ \"caf\xe9.esp\" ] }").unwrap());
        assert_eq!(
            p.mod_names.unwrap()[0],
            Err(missing("modNames[0]", Problem::Expected("UTF-8 text")))
        );
    }

    #[test]
    fn form_refs_need_a_plugin_and_a_24_bit_hex_id() {
        let ok = |t: &str, plugin: &str, id: u32| {
            assert_eq!(
                FormRef::parse(t),
                Some(FormRef {
                    plugin: plugin.into(),
                    id
                }),
                "{t}"
            )
        };
        ok("Skyrim.esm|000007", "Skyrim.esm", 7);
        ok("A.esp|FFFFFF", "A.esp", 0xFFFFFF);
        ok("A.esp|abc", "A.esp", 0xABC);
        ok("A|B.esp|01", "A|B.esp", 1);
        for bad in [
            "A.esp",
            "|000001",
            "A.esp|",
            "A.esp|1000000",
            "A.esp|00000G",
            "A.esp|+1",
            "A.esp| 1",
        ] {
            assert_eq!(FormRef::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_missing_or_wrong_morphs_section_is_one_problem() {
        let p = read(r#"{ "morphs" : 1 }"#);
        let morphs: Vec<&Missing> = p
            .problems()
            .into_iter()
            .filter(|m| m.path.starts_with("morphs"))
            .collect();
        assert_eq!(
            morphs,
            vec![&missing("morphs", Problem::Expected("an object"))]
        );
        let p = read("{}");
        let morphs = p
            .problems()
            .into_iter()
            .filter(|m| m.path.starts_with("morphs"))
            .count();
        assert_eq!(morphs, 1);
    }

    #[test]
    fn float_fields_must_fit_a_float() {
        const RANGE: Problem = Problem::Expected("a number within a float's range");
        let weight = |text: &str| {
            read(&format!(r#"{{ "actor" : {{ "weight" : {text} }} }}"#))
                .actor
                .unwrap()
                .weight
        };
        // The largest float as RaceMenu writes it, a hair above it as a
        // double, and its negative.
        assert_eq!(weight("3.402823466385289e+38"), Ok(3.402823466385289e38));
        assert_eq!(weight("-3.402823466385289e+38"), Ok(-3.402823466385289e38));
        assert_eq!(weight("1e-50"), Ok(1e-50));
        for bad in ["3.5e38", "-3.5e38", "1e39", "1e400", "-1e400"] {
            assert_eq!(weight(bad), Err(missing("actor.weight", RANGE)), "{bad}");
        }
        let p = read(
            r#"{ "morphs" : { "custom" : [ { "name" : "A", "value" : 1e300 } ], "default" : { "morphs" : [ 0, 1e39 ], "presets" : [] } } }"#,
        );
        let c = p.custom_morphs.unwrap();
        assert_eq!(
            c[0].as_ref().unwrap().value,
            Err(missing("morphs.custom[0].value", RANGE))
        );
        let d = p.default_morphs.unwrap().morphs.unwrap();
        assert_eq!(
            d,
            vec![Ok(0.0), Err(missing("morphs.default.morphs[1]", RANGE))]
        );
    }

    #[test]
    fn form_refs_write_back_as_six_upper_case_hex_digits() {
        for (text, written) in [
            ("UBE_AllRace.esp|0144EA", "UBE_AllRace.esp|0144EA"),
            ("Skyrim.esm|7", "Skyrim.esm|000007"),
            ("A.esp|abcdef", "A.esp|ABCDEF"),
            ("A|B.esp|000801", "A|B.esp|000801"),
        ] {
            let f = FormRef::parse(text).unwrap();
            assert_eq!(f.to_string(), written);
            assert_eq!(FormRef::parse(&f.to_string()), Some(f));
        }
    }

    #[test]
    fn problems_are_counted_past_a_limit_without_being_listed() {
        let p = read(r#"{ "modNames" : [ 1, 2, 3, 4, 5 ] }"#);
        let all = p.problems();
        let (first, total) = p.problems_up_to(2);
        assert_eq!(total, all.len());
        assert_eq!(first, all[..2].to_vec());
        assert_eq!(p.problems_up_to(0), (vec![], all.len()));
        assert_eq!(p.problems_up_to(usize::MAX), (all.clone(), all.len()));
    }

    #[test]
    fn missing_reads_as_a_sentence() {
        assert_eq!(
            missing("actor.weight", Problem::Absent).to_string(),
            "actor.weight is missing"
        );
        assert_eq!(
            missing("mods[0].index", WHOLE).to_string(),
            "mods[0].index is not a whole number from 0 to 4294967295"
        );
    }
}
