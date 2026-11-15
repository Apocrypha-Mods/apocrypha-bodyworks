//! Behaviour of the crate from outside, through its public API only: round
//! trips over many generated documents, robustness against broken input,
//! the layout rules stated in `src/lib.rs`, new numbers and strings, and the
//! typed view.
//!
//! Every input here is built by hand or generated; nothing is copied from a
//! real preset. Expected number texts were worked out from C's `%.16g`
//! rules, not taken from the code under test.

use apoc_jslot::preset::{Missing, Problem};
use apoc_jslot::{
    parse, write, Document, FormRef, JslotError, Number, Object, Preset, Str, Value, MAX_DEPTH,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A small seeded generator (xorshift64*), so every run sees the same cases.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// A number with chosen text; only the parser makes one.
fn num(text: &str) -> Value {
    let v = parse(text.as_bytes()).unwrap().root;
    assert!(v.as_number().is_some());
    v
}

fn s(text: &str) -> Value {
    Value::String(Str::new(text))
}

fn doc(root: Value) -> Document {
    Document { root, bom: false }
}

fn written(root: Value) -> String {
    String::from_utf8(write(&doc(root)).unwrap()).unwrap()
}

fn err(input: &[u8]) -> (usize, usize, &'static str) {
    match parse(input) {
        Err(JslotError::Malformed {
            line,
            column,
            reason,
        }) => (line, column, reason),
        Ok(d) => panic!("read {:?} as {d:?}", String::from_utf8_lossy(input)),
        Err(e) => panic!("{e}"),
    }
}

/// The byte offset a line and column point at, checking they point inside
/// the input or just past its end. Columns count from after a byte order
/// mark.
fn offset_of(input: &[u8], line: usize, column: usize) -> usize {
    assert!(line >= 1 && column >= 1, "line {line}, column {column}");
    let mut start = if input.starts_with(b"\xEF\xBB\xBF") {
        3
    } else {
        0
    };
    for _ in 1..line {
        let nl = input[start..]
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or_else(|| panic!("line {line} is past the end"));
        start += nl + 1;
    }
    let line_len = input[start..]
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(input.len() - start);
    assert!(
        column - 1 <= line_len,
        "column {column} is past the end of line {line} ({line_len} bytes)"
    );
    start + column - 1
}

/// Checks the error position of a failed parse is inside the input.
fn error_is_in_bounds(input: &[u8]) {
    if let Err(JslotError::Malformed { line, column, .. }) = parse(input) {
        offset_of(input, line, column);
    }
}

/// Exercises everything a caller does with a document read from untrusted
/// input; none of it may panic, and what was read must write stably.
fn use_document(d: &Document) {
    let w = write(d).unwrap();
    let again = parse(&w).expect("written output reads back");
    assert_eq!(&again, d);
    assert_eq!(write(&again).unwrap(), w);
    let p = Preset::of(d);
    for m in p.problems() {
        let _ = m.to_string();
    }
}

// ---------------------------------------------------------------------------
// A reference layout, from the rules in lib.rs, written independently of
// the crate's writer.
// ---------------------------------------------------------------------------

fn is_inline_item(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => true,
    }
}

fn reference(d: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    if d.bom {
        out.extend_from_slice("\u{feff}".as_bytes());
    }
    ref_value(&mut out, &d.root, 0);
    out.push(b'\n');
    out
}

fn ref_newline(out: &mut Vec<u8>, level: usize) {
    out.push(b'\n');
    out.extend(std::iter::repeat_n(b' ', level * 3));
}

fn ref_value(out: &mut Vec<u8>, v: &Value, level: usize) {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Number(n) => out.extend_from_slice(n.text().as_bytes()),
        Value::String(st) => {
            out.push(b'"');
            out.extend_from_slice(st.raw());
            out.push(b'"');
        }
        Value::Array(a) if a.is_empty() => out.extend_from_slice(b"[]"),
        Value::Object(o) if o.is_empty() => out.extend_from_slice(b"{}"),
        Value::Array(a) => {
            if a.iter().all(is_inline_item) {
                let mut line = b"[ ".to_vec();
                for (i, item) in a.iter().enumerate() {
                    if i > 0 {
                        line.extend_from_slice(b", ");
                    }
                    ref_value(&mut line, item, 0);
                }
                line.extend_from_slice(b" ]");
                if line.len() < 74 {
                    out.extend_from_slice(&line);
                    return;
                }
            }
            out.push(b'[');
            for (i, item) in a.iter().enumerate() {
                ref_newline(out, level + 1);
                ref_value(out, item, level + 1);
                if i + 1 < a.len() {
                    out.push(b',');
                }
            }
            ref_newline(out, level);
            out.push(b']');
        }
        Value::Object(o) => {
            out.push(b'{');
            let n = o.len();
            for (i, (k, item)) in o.iter().enumerate() {
                ref_newline(out, level + 1);
                out.push(b'"');
                out.extend_from_slice(k.raw());
                out.extend_from_slice(b"\" : ");
                ref_value(out, item, level + 1);
                if i + 1 < n {
                    out.push(b',');
                }
            }
            ref_newline(out, level);
            out.push(b'}');
        }
    }
}

/// The plain layout rules: LF only, one LF at the end, no tabs, indents in
/// threes.
fn check_layout(bytes: &[u8]) {
    let body = bytes.strip_prefix("\u{feff}".as_bytes()).unwrap_or(bytes);
    assert!(body.ends_with(b"\n"));
    assert!(!body.ends_with(b"\n\n"));
    assert!(!body.contains(&b'\t'));
    assert!(!body.contains(&b'\r'));
    for line in body[..body.len() - 1].split(|&b| b == b'\n') {
        let indent = line.iter().take_while(|&&b| b == b' ').count();
        assert_eq!(indent % 3, 0, "{:?}", String::from_utf8_lossy(line));
    }
}

// ---------------------------------------------------------------------------
// 1. Property-style round trips
// ---------------------------------------------------------------------------

const TEXT_POOL: &[char] = &[
    'a', 'b', 'Z', '0', ' ', '|', '.', '"', '\\', '/', '\n', '\r', '\t', '\u{0}', '\u{1}', '\u{8}',
    '\u{c}', '\u{1f}', '\u{7f}', 'é', '€', '😀', '\u{feff}',
];

fn gen_text(rng: &mut Rng, max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(TEXT_POOL)).collect()
}

fn gen_number(rng: &mut Rng) -> Number {
    loop {
        let n = match rng.below(7) {
            0 => Some(Number::from_i64(rng.below(2000) as i64 - 1000)),
            1 => Some(Number::from_i64(rng.next() as i64)),
            2 => Some(Number::from_u64(rng.next())),
            3 => Number::from_f64(f64::from_bits(rng.next())),
            4 => Number::from_f32(f32::from_bits(rng.next() as u32)),
            5 => Number::from_f64((rng.below(20001) as f64 - 10000.0) / 64.0),
            _ => Number::from_f32(rng.below(1000) as f32 / 100.0),
        };
        if let Some(n) = n {
            return n;
        }
    }
}

fn gen_scalar(rng: &mut Rng) -> Value {
    match rng.below(6) {
        0 => Value::Null,
        1 => Value::Bool(rng.chance(50)),
        2 | 3 => Value::Number(gen_number(rng)),
        _ => Value::String(Str::new(&gen_text(rng, 10))),
    }
}

fn gen_value(rng: &mut Rng, depth: usize) -> Value {
    if depth == 0 || rng.chance(35) {
        return gen_scalar(rng);
    }
    match rng.below(5) {
        // Arrays of small numbers, whose lengths land either side of the
        // one-line limit.
        0 => Value::Array(
            (0..rng.below(30))
                .map(|_| Value::Number(Number::from_i64(rng.below(1000) as i64)))
                .collect(),
        ),
        1 | 2 => Value::Array(
            (0..rng.below(6))
                .map(|_| match rng.below(4) {
                    0 => Value::Array(vec![]),
                    1 => Value::Object(Object::new()),
                    _ => gen_value(rng, depth - 1),
                })
                .collect(),
        ),
        _ => {
            let mut o = Object::new();
            for _ in 0..rng.below(7) {
                o.insert(&gen_text(rng, 6), gen_value(rng, depth - 1));
            }
            Value::Object(o)
        }
    }
}

fn keys_in_byte_order(v: &Value) {
    match v {
        Value::Object(o) => {
            let keys: Vec<Vec<u8>> = o.iter().map(|(k, _)| k.bytes().into_owned()).collect();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted);
            o.iter().for_each(|(_, v)| keys_in_byte_order(v));
        }
        Value::Array(a) => a.iter().for_each(keys_in_byte_order),
        _ => {}
    }
}

#[test]
fn generated_documents_write_read_and_write_again_the_same() {
    for seed in 0..3000u64 {
        let mut rng = Rng::new(seed);
        let d = Document {
            root: gen_value(&mut rng, 6),
            bom: rng.chance(20),
        };
        let first = write(&d).unwrap();
        let read = parse(&first)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}\n{}", String::from_utf8_lossy(&first)));
        assert_eq!(
            read, d,
            "seed {seed}: reading the output changed the document"
        );
        assert_eq!(
            write(&read).unwrap(),
            first,
            "seed {seed}: second write differs"
        );
    }
}

#[test]
fn generated_documents_are_laid_out_by_the_stated_rules() {
    for seed in 0..3000u64 {
        let mut rng = Rng::new(seed ^ 0xABCD);
        let d = Document {
            root: gen_value(&mut rng, 6),
            bom: rng.chance(20),
        };
        let out = write(&d).unwrap();
        check_layout(&out);
        assert!(
            out == reference(&d),
            "seed {seed}:\nwritten:\n{}\nexpected:\n{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&reference(&d))
        );
        keys_in_byte_order(&d.root);
    }
}

/// JSON text in another layout: odd whitespace, escapes RaceMenu would not
/// write, numbers in forms it would not write, keys out of order.
fn gen_json(rng: &mut Rng, depth: usize, out: &mut String) {
    const WS: &[&str] = &["", " ", "  ", "\t", "\n", "\r\n", " \n\t "];
    const NUMBERS: &[&str] = &[
        "0",
        "-0",
        "1E5",
        "2.50",
        "-0.0",
        "1e-05",
        "1e+400",
        "4294967295",
        "18446744073709551616",
        "3.402823466385289e+38",
        "-12.5E-3",
    ];
    const STRINGS: &[&str] = &[
        "",
        "plain",
        r"\/",
        r"\u00e9",
        r"\u00E9",
        r"\uD83D\uDE00",
        r#"\"q\""#,
        r"a\\b",
        r"\b\f\n\r\t",
        "\u{e9}",
        r"\u0000",
    ];
    let ws = |rng: &mut Rng, out: &mut String| out.push_str(rng.pick(WS));
    if depth == 0 || rng.chance(30) {
        match rng.below(5) {
            0 => out.push_str(rng.pick(&["null", "true", "false"])),
            1 | 2 => out.push_str(rng.pick(NUMBERS)),
            _ => {
                out.push('"');
                out.push_str(rng.pick(STRINGS));
                out.push('"');
            }
        }
        return;
    }
    if rng.chance(50) {
        out.push('[');
        let n = rng.below(5);
        ws(rng, out);
        for i in 0..n {
            if i > 0 {
                out.push(',');
                ws(rng, out);
            }
            gen_json(rng, depth - 1, out);
            ws(rng, out);
        }
        out.push(']');
    } else {
        out.push('{');
        let n = rng.below(5);
        ws(rng, out);
        // Keys are unique by index; some are written with an escape.
        let start = rng.below(10);
        for i in 0..n {
            if i > 0 {
                out.push(',');
                ws(rng, out);
            }
            let k = 9 - ((start + i) % 10);
            if rng.chance(30) {
                out.push_str(&format!("\"\\u006B{k}\""));
            } else {
                out.push_str(&format!("\"k{k}\""));
            }
            ws(rng, out);
            out.push(':');
            ws(rng, out);
            gen_json(rng, depth - 1, out);
            ws(rng, out);
        }
        out.push('}');
    }
}

#[test]
fn text_in_other_layouts_reads_back_equal_once_written() {
    for seed in 0..3000u64 {
        let mut rng = Rng::new(seed ^ 0x5151);
        let mut text = String::new();
        if rng.chance(20) {
            text.push('\u{feff}');
        }
        text.push_str(rng.pick(&["", " ", "\r\n"]));
        gen_json(&mut rng, 5, &mut text);
        text.push_str(rng.pick(&["", "\n", " \r\n\t"]));
        let d = parse(text.as_bytes()).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{text}"));
        assert_eq!(d.bom, text.starts_with('\u{feff}'));
        let out = write(&d).unwrap();
        check_layout(&out);
        assert_eq!(out, reference(&d), "seed {seed}");
        let again = parse(&out).unwrap();
        assert_eq!(again, d, "seed {seed}");
        assert_eq!(write(&again).unwrap(), out, "seed {seed}");
    }
}

// ---------------------------------------------------------------------------
// 2. Robustness
// ---------------------------------------------------------------------------

/// A hand-built preset in RaceMenu's layout (no bytes from a real one).
const PRESET: &str = r#"{
   "actor" : {
      "hairColor" : 1193046,
      "headTexture" : "Test Race.esp|000D62",
      "weight" : 37.5
   },
   "headParts" : [
      {
         "formId" : 4278192226,
         "formIdentifier" : "Test Hair.esp|000D62",
         "type" : 3
      },
      {
         "formId" : 81,
         "formIdentifier" : "Skyrim.esm|51",
         "type" : 1
      }
   ],
   "modNames" : [ "Skyrim.esm", "Test Hair.esp", "Test Race.esp" ],
   "mods" : [
      {
         "index" : 0,
         "name" : "Skyrim.esm"
      },
      {
         "index" : 1,
         "name" : "Test Hair.esp"
      }
   ],
   "morphs" : {
      "custom" : [
         {
            "name" : "Brow \u00e9 \uD83D\uDE00",
            "value" : 0.3100000023841858
         }
      ],
      "default" : {
         "morphs" : [ 0, -1, 0.5, 3.402823466385289e+38 ],
         "presets" : [ 2, 4294967295, 0, 7 ]
      },
      "sculpt" : []
   },
   "tintInfo" : [
      {
         "color" : 4294901760,
         "index" : 4,
         "texture" : "Actors\\Character\\Masks\\Cheek.dds"
      }
   ],
   "version" : {
      "formatVersion" : 3,
      "runtimeVersion" : 1,
      "signature" : 1163086675,
      "skseVersion" : 2
   }
}
"#;

#[test]
fn the_hand_built_preset_round_trips_byte_for_byte() {
    let d = parse(PRESET.as_bytes()).unwrap();
    assert_eq!(write(&d).unwrap(), PRESET.as_bytes());
    let p = Preset::of(&d);
    assert!(p.problems().is_empty(), "{:?}", p.problems());
    let name = p.custom_morphs.unwrap().remove(0).unwrap().name.unwrap();
    assert_eq!(name, "Brow é 😀");
}

#[test]
fn every_truncation_of_a_preset_is_refused_with_a_position_inside_it() {
    for bom in [false, true] {
        let mut bytes = Vec::new();
        if bom {
            bytes.extend_from_slice("\u{feff}".as_bytes());
        }
        bytes.extend_from_slice(PRESET.as_bytes());
        for end in 0..bytes.len() {
            let cut = &bytes[..end];
            match parse(cut) {
                // Only dropping the final line feed leaves a whole preset.
                Ok(d) => {
                    assert_eq!(end, bytes.len() - 1, "a prefix of {end} bytes was read");
                    assert_eq!(d.bom, bom);
                }
                Err(JslotError::Malformed { line, column, .. }) => {
                    let at = offset_of(cut, line, column);
                    assert!(at <= cut.len());
                }
                Err(e) => panic!("{e}"),
            }
        }
        // And every suffix: a preset with its start cut off.
        for start in 1..bytes.len() {
            let cut = &bytes[start..];
            if let Ok(d) = parse(cut) {
                use_document(&d);
            }
            error_is_in_bounds(cut);
        }
    }
}

#[test]
fn a_cut_preset_reports_where_the_file_ends() {
    // Cut straight after `"weight" : `: the value is missing at the end.
    let at = PRESET.find("37.5").unwrap();
    let cut = &PRESET.as_bytes()[..at];
    assert_eq!(err(cut), (5, 18, "the file ends where a value should be"));
    // Cut inside a string: the open quote is what is reported.
    let at = PRESET.find("Race.esp|").unwrap();
    let (line, column, reason) = err(&PRESET.as_bytes()[..at]);
    assert_eq!((line, column, reason), (4, 23, "a string is not closed"));
}

#[test]
fn randomly_damaged_presets_never_panic() {
    let base = PRESET.as_bytes();
    let pieces: &[&[u8]] = &[
        b"{",
        b"}",
        b"[",
        b"]",
        b"\"",
        b"\\",
        b",",
        b":",
        b"\\u",
        b"\\uD800",
        b"\\uDC00",
        b"e",
        b"-",
        b".",
        b"0",
        b"\xEF\xBB\xBF",
        b"\xFF",
        b"\x00",
        b"\n",
        b"null",
        b"1e99999",
    ];
    for seed in 0..20_000u64 {
        let mut rng = Rng::new(seed ^ 0xDEAD);
        let mut bytes = base.to_vec();
        for _ in 0..1 + rng.below(4) {
            let at = rng.below(bytes.len() + 1);
            match rng.below(5) {
                0 if at < bytes.len() => bytes[at] = rng.next() as u8,
                1 if at < bytes.len() => {
                    bytes.remove(at);
                }
                2 => {
                    let p = rng.pick(pieces);
                    bytes.splice(at..at, p.iter().copied());
                }
                3 => {
                    let len = rng.below(40).min(bytes.len() - at.min(bytes.len()));
                    let slice = bytes[at..at + len].to_vec();
                    let to = rng.below(bytes.len() + 1);
                    bytes.splice(to..to, slice);
                }
                _ => bytes.truncate(at),
            }
        }
        match parse(&bytes) {
            Ok(d) => use_document(&d),
            Err(JslotError::Malformed { line, column, .. }) => {
                offset_of(&bytes, line, column);
            }
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    for seed in 0..5000u64 {
        let mut rng = Rng::new(seed ^ 0xF00D);
        let alphabet = b"{}[]\":,\\ \n-0123456789.eEtrufalsn\xEF\xBB\xBF\xC3\xA9u";
        let n = rng.below(64);
        let bytes: Vec<u8> = (0..n)
            .map(|_| {
                if rng.chance(10) {
                    rng.next() as u8
                } else {
                    *rng.pick(alphabet)
                }
            })
            .collect();
        match parse(&bytes) {
            Ok(d) => use_document(&d),
            Err(JslotError::Malformed { line, column, .. }) => {
                offset_of(&bytes, line, column);
            }
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn nesting_up_to_the_limit_is_read_and_one_past_it_is_refused() {
    for (open, middle, close) in [("[", "", "]"), ("{\"k\":", "null", "}")] {
        let at_limit = format!(
            "{}{middle}{}",
            open.repeat(MAX_DEPTH),
            close.repeat(MAX_DEPTH)
        );
        let d = parse(at_limit.as_bytes()).unwrap();
        // The deepest document allowed also writes and reads back.
        use_document(&d);
        let past = format!(
            "{}{middle}{}",
            open.repeat(MAX_DEPTH + 1),
            close.repeat(MAX_DEPTH + 1)
        );
        let (line, column, reason) = err(past.as_bytes());
        assert_eq!(reason, "arrays and objects nest too deeply");
        // It points at the bracket that goes one too deep.
        assert_eq!(line, 1);
        assert_eq!(column, open.len() * MAX_DEPTH + 1);
    }
    // Far past the limit, mixed and unclosed, without exhausting the stack.
    let huge = "[{\"a\":".repeat(500_000);
    assert_eq!(err(huge.as_bytes()).2, "arrays and objects nest too deeply");
    let huge = "[".repeat(2_000_000);
    assert_eq!(err(huge.as_bytes()).2, "arrays and objects nest too deeply");
}

#[test]
fn huge_numbers_are_kept_as_written() {
    let long = format!("[{}]", "9".repeat(100_000));
    let d = parse(long.as_bytes()).unwrap();
    let n = d.root.as_array().unwrap()[0].as_number().unwrap();
    assert_eq!(n.text().len(), 100_000);
    assert_eq!(n.as_u64(), None);
    assert_eq!(n.as_i64(), None);
    assert_eq!(n.as_f64(), f64::INFINITY);
    use_document(&d);

    for (text, value) in [
        ("1e999999999999999999999", f64::INFINITY),
        ("-1e400", f64::NEG_INFINITY),
        ("1e-99999", 0.0),
        ("18446744073709551615", 18446744073709551615.0),
    ] {
        let d = parse(format!("[ {text} ]\n").as_bytes()).unwrap();
        let n = d.root.as_array().unwrap()[0].as_number().unwrap();
        assert_eq!(n.text(), text);
        assert_eq!(n.as_f64(), value, "{text}");
        assert_eq!(write(&d).unwrap(), format!("[ {text} ]\n").as_bytes());
    }
    assert_eq!(
        parse(b"[18446744073709551615]")
            .unwrap()
            .root
            .as_array()
            .unwrap()[0]
            .as_number()
            .unwrap()
            .as_u64(),
        Some(u64::MAX)
    );
}

#[test]
fn bad_escapes_are_refused_at_their_backslash() {
    // Column 5 is the backslash in each: `[`, ` `, `"`, `x`, `\`.
    let cases: [(&[u8], &str); 7] = [
        (br#"[ "x\q" ]"#, "a string has an unknown escape"),
        (br#"[ "x\U0041" ]"#, "a string has an unknown escape"),
        (br#"[ "x\'" ]"#, "a string has an unknown escape"),
        (b"[ \"x\\\x00\" ]", "a string has an unknown escape"),
        (br#"[ "x\u12G4" ]"#, "a \\u escape needs four hex digits"),
        (br#"[ "x\u" ]"#, "a \\u escape needs four hex digits"),
        (br#"[ "x\u+123" ]"#, "a \\u escape needs four hex digits"),
    ];
    for (input, reason) in cases {
        assert_eq!(
            err(input),
            (1, 5, reason),
            "{}",
            String::from_utf8_lossy(input)
        );
    }
    // A backslash as the last byte.
    assert_eq!(err(b"[ \"x\\").2, "a string has an unknown escape");
}

#[test]
fn lone_surrogates_are_refused_at_their_escape() {
    let inputs: [&[u8]; 7] = [
        br#"[ "x\uDC00" ]"#,
        br#"[ "x\uDFFF" ]"#,
        br#"[ "x\uD800" ]"#,
        br#"[ "x\uDBFF\n" ]"#,
        br#"[ "x\uD800\u0041" ]"#,
        br#"[ "x\uD800\uD800" ]"#,
        br#"[ "x\uDC00\uD800" ]"#,
    ];
    for input in inputs {
        assert_eq!(
            err(input),
            (1, 5, "a \\u escape is half a pair"),
            "{}",
            String::from_utf8_lossy(input)
        );
    }
    // A high surrogate whose partner is cut short.
    assert!(parse(br#"[ "x\uD800\uDC0" ]"#).is_err());
    assert!(parse(br#"[ "x\uD800\"#).is_err());
    // Paired at the ends of the range.
    for ok in [r#"[ "\uD800\uDC00" ]"#, r#"[ "\uDBFF\uDFFF" ]"#] {
        let d = parse(ok.as_bytes()).unwrap();
        let t = d.root.as_array().unwrap()[0]
            .as_str()
            .unwrap()
            .text()
            .unwrap();
        assert!(t == "\u{10000}" || t == "\u{10FFFF}");
        assert_eq!(write(&d).unwrap(), format!("{ok}\n").as_bytes());
    }
}

#[test]
fn empty_and_bom_only_input_are_refused() {
    assert_eq!(err(b""), (1, 1, "the file ends where a value should be"));
    assert_eq!(
        err(b" \n "),
        (2, 2, "the file ends where a value should be")
    );
    // Columns count from after a byte order mark.
    assert_eq!(
        err(b"\xEF\xBB\xBF"),
        (1, 1, "the file ends where a value should be")
    );
    // Half a byte order mark is not one.
    assert_eq!(err(b"\xEF\xBB{}"), (1, 1, "expected a value"));
    // Nor is a second one after the first.
    assert_eq!(
        err(b"\xEF\xBB\xBF\xEF\xBB\xBF{}"),
        (1, 1, "expected a value")
    );
    // A byte order mark in UTF-16 is not one either.
    assert_eq!(err(b"\xFF\xFE{}"), (1, 1, "expected a value"));
}

#[test]
fn trailing_garbage_is_refused_where_it_starts() {
    let follows = "something follows the end of the preset";
    assert_eq!(err(b"{}\n\n x"), (3, 2, follows));
    assert_eq!(err(b"{}\x00"), (1, 3, follows));
    assert_eq!(err(b"[1]]"), (1, 4, follows));
    assert_eq!(err(b"{} {}"), (1, 4, follows));
    assert_eq!(err(b"{}\n\xEF\xBB\xBF"), (2, 1, follows));
    assert_eq!(err(b"1 2"), (1, 3, follows));
    let mut preset = PRESET.as_bytes().to_vec();
    preset.extend_from_slice(b",");
    let lines = PRESET.lines().count();
    assert_eq!(err(&preset), (lines + 1, 1, follows));
}

#[test]
fn errors_name_the_line_and_column_of_the_fault() {
    // A missing comma between members: at the second key.
    assert_eq!(
        err(b"{\n   \"a\" : 1\n   \"b\" : 2\n}\n"),
        (3, 4, "expected ',' or '}' after a member")
    );
    // A repeated key: at the second one.
    assert_eq!(
        err(b"{\n   \"a\" : 1,\n   \"a\" : 2\n}\n"),
        (3, 4, "an object names the same key twice")
    );
    // An unclosed string: at its opening quote.
    assert_eq!(err(b"{\n   \"abc"), (2, 4, "a string is not closed"));
    // A bad escape on line 2.
    assert_eq!(
        err(b"[\n  \"x\\q\"]"),
        (2, 5, "a string has an unknown escape")
    );
    // A malformed number: at its start.
    assert_eq!(err(b"[ 01 ]"), (1, 3, "a number is malformed"));
    assert_eq!(err(b"[\n   -1.e5 ]"), (2, 4, "a number is malformed"));
    // CR LF line ends: the CR is not a column of the next line.
    assert_eq!(err(b"{\r\n  ]"), (2, 3, "expected a key in quotes"));
    // A raw control character inside a string: at that character.
    assert_eq!(
        err(b"[ \"ab\tc\" ]"),
        (1, 6, "a string holds a control character")
    );
    // Multi-byte text before the fault counts in bytes.
    assert_eq!(
        err("[ \"é\" x ]".as_bytes()),
        (1, 8, "expected ',' or ']' after an item")
    );
    // Error messages carry the position too.
    let e = parse(b"[ 01 ]").unwrap_err();
    assert_eq!(
        e.to_string(),
        "can't read this preset: a number is malformed (line 1, column 3)"
    );
}

// ---------------------------------------------------------------------------
// 3. Layout rules
// ---------------------------------------------------------------------------

/// `n` numbers of one digit each.
fn digits(n: usize) -> Value {
    Value::Array((0..n).map(|i| num(&((i % 9) + 1).to_string())).collect())
}

#[test]
fn arrays_shorter_than_74_go_on_one_line_and_74_or_more_do_not() {
    // `[ ` + ` ]` is 4, each `, ` 2. Three items of 21, 22, 22 make 73.
    let n = |w: usize| num(&"7".repeat(w));
    let fits = Value::Array(vec![n(21), n(22), n(22)]);
    let t = written(fits);
    assert_eq!(t.len(), 73 + 1);
    assert_eq!(
        t,
        format!(
            "[ {}, {}, {} ]\n",
            "7".repeat(21),
            "7".repeat(22),
            "7".repeat(22)
        )
    );

    let at_74 = Value::Array(vec![n(22), n(22), n(22)]);
    assert_eq!(
        written(at_74),
        format!("[\n   {0},\n   {0},\n   {0}\n]\n", "7".repeat(22))
    );
    let at_75 = Value::Array(vec![n(23), n(22), n(22)]);
    assert_eq!(written(at_75).lines().count(), 5);

    // One item: `[ ` + 69 + ` ]` = 73 fits; 70 is 74 and does not.
    assert_eq!(written(Value::Array(vec![n(69)])).lines().count(), 1);
    assert_eq!(
        written(Value::Array(vec![n(70)])),
        format!("[\n   {}\n]\n", "7".repeat(70))
    );

    // 23 one-digit items: 4 + 22*2 + 23 = 71; 24 items: 74.
    assert_eq!(written(digits(23)).lines().count(), 1);
    assert_eq!(written(digits(24)).lines().count(), 26);
}

#[test]
fn the_limit_ignores_the_indent_and_the_key() {
    // A 73-character array, four levels deep under a long key, still fits.
    let n = |w: usize| num(&"7".repeat(w));
    let arr = Value::Array(vec![n(21), n(22), n(22)]);
    let mut inner = Object::new();
    inner.insert(&"k".repeat(40), arr);
    let mut v = Value::Object(inner);
    for _ in 0..3 {
        let mut o = Object::new();
        o.insert("level", v);
        v = Value::Object(o);
    }
    let t = written(v);
    let line = t.lines().find(|l| l.contains("[ ")).unwrap();
    assert!(line.starts_with(&format!("            \"{}\" : [ ", "k".repeat(40))));
    assert!(line.ends_with(" ]"));
    assert!(line.len() > 74);
}

#[test]
fn strings_count_as_written_with_quotes_and_escapes() {
    // `[ "` ... `" ]` is 6, plus 67 written bytes = 73.
    let fits = Value::Array(vec![s(&format!("{}\\", "a".repeat(65)))]);
    assert_eq!(written(fits).lines().count(), 1);
    // 68 written bytes (66 + `\\`), though 67 once undone.
    let over = Value::Array(vec![s(&format!("{}\\", "a".repeat(66)))]);
    assert_eq!(written(over).lines().count(), 3);
    // `\u001F` is six bytes as written.
    let fits = Value::Array(vec![s(&format!("{}\u{1f}", "a".repeat(61)))]);
    assert_eq!(written(fits).lines().count(), 1);
    let over = Value::Array(vec![s(&format!("{}\u{1f}", "a".repeat(62)))]);
    assert_eq!(written(over).lines().count(), 3);
}

#[test]
fn arrays_of_empty_containers_stay_on_one_line() {
    let empties = Value::Array(vec![
        Value::Array(vec![]),
        Value::Object(Object::new()),
        Value::Array(vec![]),
    ]);
    assert_eq!(written(empties), "[ [], {}, [] ]\n");
    // Empty containers count two characters each towards the limit:
    // 4 + 2*(n-1) + 2n < 74 holds up to n = 17.
    let n_empty = |n: usize| Value::Array((0..n).map(|_| Value::Array(vec![])).collect());
    assert_eq!(written(n_empty(17)).len(), 4 + 32 + 34 + 1);
    assert_eq!(written(n_empty(17)).lines().count(), 1);
    assert_eq!(written(n_empty(18)).lines().count(), 20);
    assert!(written(n_empty(18)).starts_with("[\n   [],\n   [],"));
    // A single non-empty container makes the whole array one item a line.
    let mixed = Value::Array(vec![Value::Array(vec![]), Value::Array(vec![num("1")])]);
    assert_eq!(written(mixed), "[\n   [],\n   [ 1 ]\n]\n");
    let mut o = Object::new();
    o.insert("a", Value::Null);
    let mixed = Value::Array(vec![num("1"), Value::Object(o)]);
    assert_eq!(
        written(mixed),
        "[\n   1,\n   {\n      \"a\" : null\n   }\n]\n"
    );
}

#[test]
fn nested_arrays_are_one_item_a_line_with_short_ones_inline() {
    let v = Value::Array(vec![
        Value::Array(vec![num("1"), num("2")]),
        Value::Array(vec![Value::Array(vec![num("3")]), Value::Array(vec![])]),
        digits(24),
    ]);
    let mut expected = String::from("[\n   [ 1, 2 ],\n   [\n      [ 3 ],\n      []\n   ],\n   [\n");
    for i in 0..24 {
        expected.push_str(&format!("      {}", (i % 9) + 1));
        expected.push_str(if i < 23 { ",\n" } else { "\n" });
    }
    expected.push_str("   ]\n]\n");
    assert_eq!(written(v.clone()), expected);
    assert_eq!(parse(expected.as_bytes()).unwrap().root, v);

    // Deep nesting indents by three per level and closes at the opener's indent.
    let mut deep = Value::Array(vec![num("1")]);
    for _ in 0..3 {
        deep = Value::Array(vec![deep]);
    }
    assert_eq!(
        written(deep),
        "[\n   [\n      [\n         [ 1 ]\n      ]\n   ]\n]\n"
    );
}

#[test]
fn insert_places_new_keys_in_byte_order() {
    let mut o = Object::new();
    for key in [
        "sculptDivisor",
        "formIdentifier",
        "sculpt",
        "formId",
        "é",
        "z",
        "Z",
        "",
        "a b",
        "a",
        "_",
    ] {
        assert_eq!(o.insert(key, Value::Null), None, "{key}");
    }
    let keys: Vec<String> = o.iter().map(|(k, _)| k.text().unwrap()).collect();
    assert_eq!(
        keys,
        [
            "",
            "Z",
            "_",
            "a",
            "a b",
            "formId",
            "formIdentifier",
            "sculpt",
            "sculptDivisor",
            "z",
            "é"
        ]
    );
    // Replacing keeps the key where it is and gives the old value back.
    assert_eq!(o.insert("formId", Value::Bool(true)), Some(Value::Null));
    assert_eq!(o.iter().nth(5).unwrap().0.text().unwrap(), "formId");
    assert_eq!(o.get("formId"), Some(&Value::Bool(true)));
    assert_eq!(o.len(), 11);
    // A key that needs escaping is stored escaped and sorted by its text.
    o.insert("\"q", Value::Null);
    assert_eq!(o.iter().nth(1).unwrap().0.raw(), b"\\\"q");
}

#[test]
fn insert_into_a_read_object_compares_keys_by_their_text() {
    // Read out of order, and one key written with an escape (`\u0062` is `b`).
    let mut d = parse(br#"{ "c" : 1, "\u0062" : 2, "a" : 3 }"#).unwrap();
    let o = d.root.as_object_mut().unwrap();
    // `b` exists, though written escaped: it is replaced in place.
    assert_eq!(o.insert("b", num("9")), Some(num("2")));
    // `bb` goes before the first key that sorts after it: `c`, at the start.
    o.insert("bb", Value::Null);
    // `d` sorts after everything: at the end.
    o.insert("d", Value::Null);
    let raw: Vec<&[u8]> = o.iter().map(|(k, _)| k.raw()).collect();
    assert_eq!(raw, [&b"bb"[..], b"c", b"\\u0062", b"a", b"d"]);
    assert_eq!(
        write(&d).unwrap(),
        b"{\n   \"bb\" : null,\n   \"c\" : 1,\n   \"\\u0062\" : 9,\n   \"a\" : 3,\n   \"d\" : null\n}\n"
    );
}

#[test]
fn a_byte_order_mark_is_kept_and_only_when_there_was_one() {
    let with = parse(b"\xEF\xBB\xBF{\r\n\t\"a\":[1,2]}").unwrap();
    assert!(with.bom);
    assert_eq!(
        write(&with).unwrap(),
        b"\xEF\xBB\xBF{\n   \"a\" : [ 1, 2 ]\n}\n"
    );
    let without = parse(b"{\"a\":[1,2]}").unwrap();
    assert!(!without.bom);
    assert_eq!(write(&without).unwrap(), b"{\n   \"a\" : [ 1, 2 ]\n}\n");
    // Whitespace may follow the mark.
    assert!(parse(b"\xEF\xBB\xBF \n{}").unwrap().bom);
    // Set by hand on a new document.
    let d = Document {
        root: Value::Null,
        bom: true,
    };
    assert_eq!(write(&d).unwrap(), b"\xEF\xBB\xBFnull\n");
    // Inside a string it is text, not a mark.
    let d = parse("[\"\u{feff}\"]".as_bytes()).unwrap();
    assert!(!d.bom);
}

// ---------------------------------------------------------------------------
// 4. New values
// ---------------------------------------------------------------------------

#[test]
fn doubles_are_written_as_percent_16g() {
    // Each expected text worked out from the exact double: 16 significant
    // digits rounded half-even on the exact binary value, trailing zeros
    // dropped, exponent form when the exponent is below -4 or 16 or more.
    let cases: &[(f64, &str)] = &[
        (0.0, "0"),
        (-0.0, "-0"),
        (1.0, "1"),
        (-1.0, "-1"),
        (42.0, "42"),
        (-1234567.0, "-1234567"),
        (4294967295.0, "4294967295"),
        (0.5, "0.5"),
        (-0.75, "-0.75"),
        // 0.1 is 0.1000000000000000055...: 16 digits end in zeros.
        (0.1, "0.1"),
        // 0.3 is 0.2999999999999999888...: rounds up to 0.3.
        (0.3, "0.3"),
        (2.0 / 3.0, "0.6666666666666666"),
        // The largest exponent written plainly is 15.
        (1e15, "1000000000000000"),
        (9999999999999998.0, "9999999999999998"),
        (9007199254740992.0, "9007199254740992"),
        (1e16, "1e+16"),
        (-1e16, "-1e+16"),
        (1.234e16, "1.234e+16"),
        // 123456789012345678 is held as 123456789012345680.
        (123456789012345678.0, "1.234567890123457e+17"),
        // 1e23 is held as 99999999999999991611392: not rounded up to 1e+23.
        (1e23, "9.999999999999999e+22"),
        // The smallest exponent written plainly is -4.
        (0.0001, "0.0001"),
        (0.00012345, "0.00012345"),
        (1e-5, "1e-05"),
        (1.5e-5, "1.5e-05"),
        (-2.5e-7, "-2.5e-07"),
        // Subnormals and the ends of the range: three-digit exponents.
        (f64::from_bits(1), "4.940656458412465e-324"),
        (f64::MIN_POSITIVE, "2.225073858507201e-308"),
        (f64::MAX, "1.797693134862316e+308"),
        (f64::MIN, "-1.797693134862316e+308"),
    ];
    for &(value, text) in cases {
        assert_eq!(
            Number::from_f64(value).map(|n| n.text().to_string()),
            Some(text.to_string()),
            "{value:e}"
        );
    }
}

#[test]
fn floats_are_widened_then_written_as_percent_16g() {
    let cases: &[(f32, &str)] = &[
        (0.0, "0"),
        (-0.0, "-0"),
        (0.5, "0.5"),
        (100.0, "100"),
        (-1.0, "-1"),
        (16777216.0, "16777216"),
        // 0.1f32 is 0.100000001490116119384765625.
        (0.1, "0.1000000014901161"),
        (0.31, "0.3100000023841858"),
        // 1e16f32 is 10000000272564224.
        (1e16, "1.000000027256422e+16"),
        // 1e-5f32 is 9.99999974737875163555...e-06.
        (1e-5, "9.999999747378752e-06"),
        (f32::MAX, "3.402823466385289e+38"),
        (f32::MIN, "-3.402823466385289e+38"),
        (f32::MIN_POSITIVE, "1.175494350822288e-38"),
        // The smallest subnormal float, 2^-149.
        (f32::from_bits(1), "1.401298464324817e-45"),
    ];
    for &(value, text) in cases {
        assert_eq!(
            Number::from_f32(value).map(|n| n.text().to_string()),
            Some(text.to_string()),
            "{value:e}"
        );
    }
}

#[test]
fn nan_and_infinity_give_no_number() {
    for v in [f64::NAN, -f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Number::from_f64(v), None, "{v}");
    }
    for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(Number::from_f32(v), None, "{v}");
    }
    // A NaN with a payload is still NaN.
    assert_eq!(
        Number::from_f64(f64::from_bits(0x7FF0_0000_0000_0001)),
        None
    );
}

#[test]
fn every_finite_double_gives_json_that_reads_back_close_and_stable() {
    let mut rng = Rng::new(0x1234);
    for _ in 0..50_000 {
        let x = f64::from_bits(rng.next());
        let Some(n) = Number::from_f64(x) else {
            assert!(!x.is_finite());
            continue;
        };
        let text = n.text();
        // It is a JSON number the parser takes, and keeps.
        let d = parse(text.as_bytes()).unwrap_or_else(|e| panic!("{x:e}: {text}: {e}"));
        assert_eq!(d.root.as_number().unwrap().text(), text);
        let back: f64 = text.parse().unwrap();
        // Sixteen significant digits: within half a unit in the 16th.
        if x.abs() >= f64::MIN_POSITIVE {
            assert!(((back - x) / x).abs() <= 6e-16, "{x:e}: {text}");
        }
        // At most 16 significant digits, no trailing zeros in a fraction.
        let mantissa = text.split(['e']).next().unwrap();
        let sig: String = mantissa
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .trim_start_matches('0')
            .to_string();
        assert!(sig.len() <= 16, "{text}");
        if mantissa.contains('.') {
            assert!(!mantissa.ends_with('0'), "{text}");
        }
        // Exponent form exactly when the written value is below 1e-4 or at
        // least 1e16, with a sign and at least two digits.
        let magnitude = back.abs();
        let exponent_form = magnitude != 0.0 && !(1e-4..1e16).contains(&magnitude);
        assert_eq!(text.contains('e'), exponent_form, "{text}");
        if let Some((_, e)) = text.split_once('e') {
            assert!(e.starts_with('+') || e.starts_with('-'), "{text}");
            assert!(e.len() >= 3, "{text}");
        }
        // Writing what was read gives the same text again.
        assert_eq!(Number::from_f64(back).unwrap().text(), text, "{x:e}");
    }
}

#[test]
fn new_strings_read_back_as_the_same_text() {
    // Every ASCII character alone, then mixed with non-ASCII.
    let mut texts: Vec<String> = (0u8..0x80).map(|b| (b as char).to_string()).collect();
    texts.extend(
        [
            "",
            "Actors\\Character\\Masks\\Cheek.dds",
            "a/b",
            "say \"hi\"",
            "line\nbreak\r\n",
            "\u{0}\u{1}\u{1f}\u{7f}",
            "é€😀\u{feff}\u{10FFFF}",
            "\\u0041",
            "\\\\\\",
        ]
        .map(String::from),
    );
    let mut rng = Rng::new(77);
    for _ in 0..2000 {
        texts.push(gen_text(&mut rng, 20));
    }
    for t in &texts {
        let st = Str::new(t);
        assert_eq!(st.text().as_deref(), Some(t.as_str()), "{t:?}");
        assert_eq!(st.bytes().as_ref(), t.as_bytes());
        // No raw control characters, and only JSON escapes in what is written.
        assert!(st.raw().iter().all(|&b| b >= 0x20), "{t:?}");
        let d = doc(Value::Array(vec![Value::String(st.clone())]));
        let back = parse(&write(&d).unwrap()).unwrap();
        assert_eq!(back, d, "{t:?}");
        let read = back.root.as_array().unwrap()[0].as_str().unwrap().text();
        assert_eq!(read.as_deref(), Some(t.as_str()));
    }
}

#[test]
fn new_strings_use_the_documented_escapes() {
    for b in 0u8..0x20 {
        let expected = match b {
            0x08 => "\\b".to_string(),
            0x0C => "\\f".to_string(),
            b'\n' => "\\n".to_string(),
            b'\r' => "\\r".to_string(),
            b'\t' => "\\t".to_string(),
            _ => format!("\\u00{}{}", hex_upper(b >> 4), hex_upper(b & 15)),
        };
        assert_eq!(
            Str::new(&(b as char).to_string()).raw(),
            expected.as_bytes(),
            "{b:#x}"
        );
    }
    assert_eq!(Str::new("\"").raw(), b"\\\"");
    assert_eq!(Str::new("\\").raw(), b"\\\\");
    assert_eq!(Str::new("/").raw(), b"/");
    assert_eq!(Str::new("\u{7f}").raw(), b"\x7f");
    assert_eq!(Str::new("é😀").raw(), "é😀".as_bytes());
}

fn hex_upper(n: u8) -> char {
    b"0123456789ABCDEF"[n as usize] as char
}

// ---------------------------------------------------------------------------
// 5. Typed view
// ---------------------------------------------------------------------------

const WHOLE: Problem = Problem::Expected("a whole number from 0 to 4294967295");

fn view(text: &str) -> Preset {
    Preset::of(&parse(text.as_bytes()).unwrap())
}

fn missing(path: &str, problem: Problem) -> Missing {
    Missing {
        path: path.to_string(),
        problem,
    }
}

#[test]
fn form_refs_read_plugin_bar_id() {
    let ok = |t: &str, plugin: &str, id: u32| {
        assert_eq!(
            FormRef::parse(t),
            Some(FormRef {
                plugin: plugin.into(),
                id
            }),
            "{t}"
        );
    };
    ok("Skyrim.esm|7", "Skyrim.esm", 7);
    ok("Skyrim.esm|07", "Skyrim.esm", 7);
    ok("A.esp|000000", "A.esp", 0);
    ok("A.esp|FFFFFF", "A.esp", 0xFF_FFFF);
    ok("A.esp|ffffff", "A.esp", 0xFF_FFFF);
    ok("A.esp|0a0B0c", "A.esp", 0x0A_0B0C);
    ok("My Mod - Part 2.esp|123", "My Mod - Part 2.esp", 0x123);
    ok("Café.esp|1", "Café.esp", 1);
    ok(" |1", " ", 1);
    // The plugin is everything before the last bar.
    ok("A|B|C.esp|1F", "A|B|C.esp", 0x1F);
    ok("||1", "|", 1);
}

#[test]
fn form_refs_that_break_the_rules_are_refused() {
    for bad in [
        "",
        "|",
        "Skyrim.esm",
        "Skyrim.esm000007",
        "|000001",
        "Skyrim.esm|",
        "A.esp|1000000",
        "A.esp|0000000",
        "A.esp|FFFFFFF",
        "A.esp|00000G",
        "A.esp|0x1F",
        "A.esp|-1",
        "A.esp|+1",
        "A.esp| 1",
        "A.esp|1 ",
        "A.esp|1|",
        "A.esp|é",
        // Full-width digits: not ASCII hex.
        "A.esp|\u{FF11}",
    ] {
        assert_eq!(FormRef::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn form_refs_in_a_preset_report_where_they_fail() {
    let p = view(
        r#"{ "actor" : { "headTexture" : "NoBar.esp" },
             "headParts" : [ { "formIdentifier" : 7 }, { "formIdentifier" : "A.esp|1234567" } ] }"#,
    );
    let form = Problem::Expected("a form as Plugin|ID");
    assert_eq!(
        p.actor.unwrap().head_texture,
        Err(missing("actor.headTexture", form))
    );
    let h = p.head_parts.unwrap();
    assert_eq!(
        h[0].as_ref().unwrap().form_identifier,
        Err(missing(
            "headParts[0].formIdentifier",
            Problem::Expected("text")
        ))
    );
    assert_eq!(
        h[1].as_ref().unwrap().form_identifier,
        Err(missing("headParts[1].formIdentifier", form))
    );
}

#[test]
fn whole_number_fields_take_0_to_u32_max_written_plainly() {
    let read = |text: &str| {
        let p = view(&format!(r#"{{ "actor" : {{ "hairColor" : {text} }} }}"#));
        p.actor.unwrap().hair_color
    };
    assert_eq!(read("0"), Ok(0));
    assert_eq!(read("4294967295"), Ok(u32::MAX));
    assert_eq!(read("4294967294"), Ok(u32::MAX - 1));
    for bad in [
        "4294967296",
        "18446744073709551615",
        "18446744073709551616",
        "99999999999999999999999",
        "-1",
        "-4294967295",
        "1.0",
        "0.5",
        "1e3",
        "1E0",
        "\"5\"",
        "null",
        "true",
        "[ 1 ]",
        "{}",
    ] {
        assert_eq!(read(bad), Err(missing("actor.hairColor", WHOLE)), "{bad}");
    }
}

#[test]
fn one_bad_entry_does_not_hide_the_others() {
    let p = view(
        r#"{
        "headParts" : [
            { "formId" : 1, "formIdentifier" : "A.esp|1", "type" : 1 },
            "not an object",
            { "formId" : -5, "formIdentifier" : "A.esp|3", "type" : 3 },
            { "formId" : 4, "formIdentifier" : "A.esp|4", "type" : 4 }
        ],
        "mods" : [ { "index" : 0, "name" : 0 }, { "index" : 1, "name" : "B.esp" } ],
        "morphs" : {
            "custom" : [ { "name" : "S1", "value" : "x" }, null, { "name" : "S3", "value" : 1 } ],
            "default" : { "morphs" : [ 0.5, null, -1 ], "presets" : [ 1, 4294967296, 3 ] }
        },
        "tintInfo" : [ [], { "color" : 1, "index" : 2, "texture" : "t.dds" } ]
    }"#,
    );
    let h = p.head_parts.clone().unwrap();
    assert_eq!(h.len(), 4);
    assert_eq!(h[0].as_ref().unwrap().form_id, Ok(1));
    assert_eq!(
        h[1],
        Err(missing("headParts[1]", Problem::Expected("an object")))
    );
    let h2 = h[2].as_ref().unwrap();
    assert_eq!(h2.form_id, Err(missing("headParts[2].formId", WHOLE)));
    assert_eq!(h2.kind, Ok(3));
    assert_eq!(h2.form_identifier.as_ref().unwrap().id, 3);
    assert_eq!(h[3].as_ref().unwrap().form_id, Ok(4));

    let m = p.mods.clone().unwrap();
    assert_eq!(m[0].as_ref().unwrap().index, Ok(0));
    assert_eq!(
        m[0].as_ref().unwrap().name,
        Err(missing("mods[0].name", Problem::Expected("text")))
    );
    assert_eq!(m[1].as_ref().unwrap().name, Ok("B.esp".to_string()));

    let c = p.custom_morphs.clone().unwrap();
    assert_eq!(c[0].as_ref().unwrap().name, Ok("S1".into()));
    assert_eq!(
        c[1],
        Err(missing("morphs.custom[1]", Problem::Expected("an object")))
    );
    assert_eq!(c[2].as_ref().unwrap().value, Ok(1.0));

    let d = p.default_morphs.clone().unwrap();
    assert_eq!(
        d.morphs.unwrap(),
        vec![
            Ok(0.5),
            Err(missing(
                "morphs.default.morphs[1]",
                Problem::Expected("a number")
            )),
            Ok(-1.0)
        ]
    );
    assert_eq!(
        d.presets.unwrap(),
        vec![
            Ok(1),
            Err(missing("morphs.default.presets[1]", WHOLE)),
            Ok(3)
        ]
    );

    let t = p.tints.clone().unwrap();
    assert_eq!(
        t[0],
        Err(missing("tintInfo[0]", Problem::Expected("an object")))
    );
    assert_eq!(t[1].as_ref().unwrap().texture, Ok("t.dds".into()));
}

#[test]
fn problems_list_every_path_in_field_order() {
    let p = view(
        r#"{
        "version" : { "formatVersion" : 3, "runtimeVersion" : "1", "skseVersion" : 2 },
        "actor" : { "weight" : null, "hairColor" : 1, "headTexture" : "A.esp|1" },
        "headParts" : [ { "formId" : 1, "formIdentifier" : "A.esp|1", "type" : 1 }, 2 ],
        "modNames" : [ "A.esp", false ],
        "mods" : { },
        "morphs" : {
            "custom" : [ { "value" : 1 } ],
            "default" : { "morphs" : [], "presets" : [ 1.5 ] }
        },
        "tintInfo" : [ { "color" : -1, "index" : 0, "texture" : "\uD83D\uDE00" } ]
    }"#,
    );
    let got: Vec<String> = p.problems().iter().map(|m| m.to_string()).collect();
    assert_eq!(
        got,
        [
            "version.runtimeVersion is not a whole number from 0 to 4294967295",
            "version.signature is missing",
            "actor.weight is not a number",
            "headParts[1] is not an object",
            "modNames[1] is not text",
            "mods is not a list",
            "morphs.default.presets[0] is not a whole number from 0 to 4294967295",
            "morphs.custom[0].name is missing",
            "tintInfo[0].color is not a whole number from 0 to 4294967295",
        ]
    );
    // The emoji texture was read through its escapes.
    let t = p.tints.unwrap();
    assert_eq!(t[0].as_ref().unwrap().texture, Ok("😀".into()));
}

#[test]
fn a_whole_section_that_is_wrong_is_one_problem_not_one_per_field() {
    let p = view(
        r#"{ "version" : 3, "actor" : [], "morphs" : { "default" : "x", "custom" : [] }, "headParts" : [], "modNames" : [], "mods" : [], "tintInfo" : [] }"#,
    );
    let got: Vec<String> = p.problems().iter().map(|m| m.to_string()).collect();
    assert_eq!(
        got,
        [
            "version is not an object",
            "actor is not an object",
            "morphs.default is not an object",
        ]
    );
}

#[test]
fn a_morphs_section_that_is_not_an_object_is_reported_by_its_path() {
    let p = view(r#"{ "morphs" : 1 }"#);
    let not_object = missing("morphs", Problem::Expected("an object"));
    assert_eq!(p.default_morphs, Err(not_object.clone()));
    assert_eq!(p.custom_morphs, Err(not_object));
}

#[test]
fn keys_are_found_by_their_text_whatever_their_escapes() {
    let p = view(r#"{ "\u0061ctor" : { "w\u0065ight" : 12.5 } }"#);
    assert_eq!(p.actor.unwrap().weight, Ok(12.5));
}

#[test]
fn the_view_ignores_keys_it_does_not_know_and_does_not_change_the_document() {
    let mut text = PRESET.replace(
        "   \"version\" : {",
        "   \"unknownSection\" : {\n      \"kept\" : true\n   },\n   \"version\" : {",
    );
    text = text.replace(
        "\"weight\" : 37.5",
        "\"extra\" : [ 1 ],\n      \"weight\" : 37.5",
    );
    let d = parse(text.as_bytes()).unwrap();
    let before = d.clone();
    let p = Preset::of(&d);
    assert!(p.problems().is_empty(), "{:?}", p.problems());
    assert_eq!(d, before);
    assert_eq!(write(&d).unwrap(), text.as_bytes());
}
