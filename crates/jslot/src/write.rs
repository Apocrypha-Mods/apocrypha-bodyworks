//! Writing a [`Document`] in RaceMenu's layout (described in the crate's
//! documentation, with the evidence for each rule).

use crate::value::{Document, Value};
use crate::{JslotError, MAX_DEPTH};

/// One step of indent: three spaces (every indented line in the sample is a
/// multiple of three).
const INDENT: &[u8] = b"   ";

/// jsoncpp's documented right margin. An array of scalars goes on one line
/// when `[ `, its items, `, ` between them and ` ]` come to fewer bytes
/// than this. (jsoncpp also puts arrays of a third as many items or more one
/// to a line, but no such array is short enough to fit: 25 items of one
/// byte come to 77.) The sample has one-line arrays up to 25 bytes and two
/// arrays of 80 and 133 that are one item per line.
const RIGHT_MARGIN: usize = 74;

/// The most a written preset may come to, 64 MiB. A real one is about
/// 300 KB; documents built in code have no other bound, and deep indents
/// multiply quickly.
pub const MAX_OUTPUT: usize = 64 * 1024 * 1024;

/// The preset's bytes: a byte order mark if the document was read with one,
/// the top value, and one line feed.
///
/// Refuses a document whose arrays and objects nest deeper than
/// [`MAX_DEPTH`] (one read by [`crate::parse`] never does, but one built in
/// code can), and stops once the output would pass [`MAX_OUTPUT`].
pub fn write(doc: &Document) -> Result<Vec<u8>, JslotError> {
    let mut out = Out(Vec::new());
    if doc.bom {
        out.put(b"\xEF\xBB\xBF")?;
    }
    value(&mut out, &doc.root, 0)?;
    out.put(b"\n")?;
    Ok(out.0)
}

/// The output so far, which refuses to grow past [`MAX_OUTPUT`].
struct Out(Vec<u8>);

impl Out {
    fn put(&mut self, bytes: &[u8]) -> Result<(), JslotError> {
        if self.0.len() + bytes.len() > MAX_OUTPUT {
            return Err(JslotError::TooLarge { limit: MAX_OUTPUT });
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

/// Writes `v`, whose first line has already been placed (after `"key" : `,
/// or after the indent of an array item); its later lines are indented for
/// `level`, which is also how many containers hold it.
fn value(out: &mut Out, v: &Value, level: usize) -> Result<(), JslotError> {
    if matches!(v, Value::Array(_) | Value::Object(_)) && level >= MAX_DEPTH {
        return Err(JslotError::TooDeep { limit: MAX_DEPTH });
    }
    match v {
        Value::Null => out.put(b"null"),
        Value::Bool(true) => out.put(b"true"),
        Value::Bool(false) => out.put(b"false"),
        Value::Number(n) => out.put(n.text().as_bytes()),
        Value::String(s) => quoted(out, s.raw()),
        Value::Array(items) if items.is_empty() => out.put(b"[]"),
        Value::Array(items) if one_line(items) => {
            out.put(b"[ ")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.put(b", ")?;
                }
                value(out, item, level + 1)?;
            }
            out.put(b" ]")
        }
        Value::Array(items) => {
            out.put(b"[")?;
            for (i, item) in items.iter().enumerate() {
                line(out, level + 1)?;
                value(out, item, level + 1)?;
                if i + 1 < items.len() {
                    out.put(b",")?;
                }
            }
            line(out, level)?;
            out.put(b"]")
        }
        Value::Object(o) if o.is_empty() => out.put(b"{}"),
        Value::Object(o) => {
            out.put(b"{")?;
            for (i, (key, item)) in o.iter().enumerate() {
                line(out, level + 1)?;
                quoted(out, key.raw())?;
                out.put(b" : ")?;
                value(out, item, level + 1)?;
                if i + 1 < o.len() {
                    out.put(b",")?;
                }
            }
            line(out, level)?;
            out.put(b"}")
        }
    }
}

/// Ends the line and indents the next one for `level`.
fn line(out: &mut Out, level: usize) -> Result<(), JslotError> {
    out.put(b"\n")?;
    for _ in 0..level {
        out.put(INDENT)?;
    }
    Ok(())
}

fn quoted(out: &mut Out, raw: &[u8]) -> Result<(), JslotError> {
    out.put(b"\"")?;
    out.put(raw)?;
    out.put(b"\"")
}

/// Whether an array goes on one line. Lengths are in bytes, as written. An
/// empty array counts as fitting.
fn one_line(items: &[Value]) -> bool {
    let mut length = 4 + items.len().saturating_sub(1) * 2;
    for item in items {
        length += match item {
            Value::Null | Value::Bool(true) => 4,
            Value::Bool(false) => 5,
            Value::Number(n) => n.text().len(),
            Value::String(s) => s.raw().len() + 2,
            Value::Array(a) if a.is_empty() => 2,
            Value::Object(o) if o.is_empty() => 2,
            Value::Array(_) | Value::Object(_) => return false,
        };
        if length >= RIGHT_MARGIN {
            return false;
        }
    }
    length < RIGHT_MARGIN
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::value::{Number, Object, Str};
    use crate::{JslotError, MAX_DEPTH};

    fn num(text: &str) -> Value {
        Value::Number(Number::from_source(text))
    }

    fn doc(root: Value) -> Document {
        Document { root, bom: false }
    }

    fn text(root: Value) -> String {
        String::from_utf8(write(&doc(root)).unwrap()).unwrap()
    }

    /// Reads `input` and checks that writing it back gives `input` byte for
    /// byte.
    fn round(input: &str) {
        let d = parse(input.as_bytes()).unwrap();
        assert_eq!(String::from_utf8(write(&d).unwrap()).unwrap(), input);
    }

    #[test]
    fn objects_are_one_member_a_line_with_three_space_indent() {
        let mut inner = Object::new();
        inner.insert("hairColor", num("5"));
        inner.insert("weight", num("21"));
        let mut top = Object::new();
        top.insert("actor", Value::Object(inner));
        top.insert("z", Value::String(Str::new("a\\b")));
        let t = text(Value::Object(top));
        assert_eq!(
            t,
            "{\n   \"actor\" : {\n      \"hairColor\" : 5,\n      \"weight\" : 21\n   },\n   \"z\" : \"a\\\\b\"\n}\n"
        );
        round(&t);
    }

    #[test]
    fn short_arrays_of_scalars_go_on_one_line() {
        let a = Value::Array(vec![num("0"), num("4294967295"), num("0"), num("0")]);
        assert_eq!(text(a), "[ 0, 4294967295, 0, 0 ]\n");
        let mixed = Value::Array(vec![
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            Value::String(Str::new("x")),
            Value::Array(vec![]),
            Value::Object(Object::new()),
        ]);
        assert_eq!(text(mixed), "[ null, true, false, \"x\", [], {} ]\n");
        round("{\n   \"k\" : [ 1 ]\n}\n");
    }

    /// `n` items of `width` characters each.
    fn array_of(n: usize, width: usize) -> Value {
        Value::Array((0..n).map(|_| num(&"1".repeat(width))).collect())
    }

    #[test]
    fn an_array_of_73_characters_fits_on_a_line_and_one_of_74_does_not() {
        // `[ `, ` ]` and two `, ` are 8 characters; the items 65 and 66.
        let fits = text(Value::Array(vec![
            num(&"1".repeat(21)),
            num(&"1".repeat(22)),
            num(&"1".repeat(22)),
        ]));
        assert_eq!(fits.len(), 73 + 1);
        assert_eq!(fits.lines().count(), 1);
        let too_long = text(array_of(3, 22));
        assert_eq!(too_long.lines().count(), 5);
        assert!(too_long.starts_with("[\n   1111"));
    }

    #[test]
    fn strings_count_their_quotes_and_escapes_towards_the_line() {
        // 10 characters of brackets, commas and quotes, and 63 between the
        // quotes as written (62 once the escape is undone).
        let fits = Value::Array(vec![
            Value::String(Str::new(&"a".repeat(30))),
            Value::String(Str::new(&format!("{}\\", "a".repeat(31)))),
        ]);
        assert_eq!(text(fits).lines().count(), 1);
        // 64 as written, though 63 once undone would fit.
        let over = Value::Array(vec![
            Value::String(Str::new(&"a".repeat(30))),
            Value::String(Str::new(&format!("{}\\", "a".repeat(32)))),
        ]);
        assert_eq!(text(over).lines().count(), 4);
    }

    #[test]
    fn twenty_three_one_character_items_fit_and_twenty_four_do_not() {
        // 4 + 22 * 2 + 23 is 71; 4 + 23 * 2 + 24 is 74.
        assert_eq!(text(array_of(23, 1)).lines().count(), 1);
        assert_eq!(text(array_of(24, 1)).lines().count(), 26);
    }

    #[test]
    fn long_arrays_are_one_item_a_line() {
        // On one line this would be 92 characters.
        let t = "{\n   \"morphs\" : [\n      0,\n      -1,\n      3.402823466385289e+38,\n      0.3100000023841858,\n      0.3100000023841858,\n      0.3100000023841858\n   ]\n}\n";
        round(t);
    }

    #[test]
    fn arrays_holding_containers_are_one_item_a_line() {
        round("[\n   [ 1, 2, 3, 4 ],\n   [ 5, -6, 7, 8 ]\n]\n");
        round("[\n   {\n      \"a\" : 1\n   },\n   [ 1 ]\n]\n");
        round("{\n   \"a\" : [\n      [\n         {\n            \"b\" : []\n         }\n      ]\n   ]\n}\n");
        // An empty container does not make its array multi-line.
        round("[ [], {}, 1 ]\n");
    }

    #[test]
    fn empty_containers_are_written_closed() {
        round("{}\n");
        round("[]\n");
        round("{\n   \"a\" : [],\n   \"b\" : {}\n}\n");
        round("[\n   {},\n   [ 1, 2 ],\n   {\n      \"c\" : null\n   }\n]\n");
    }

    #[test]
    fn scalars_at_the_top_are_written_alone() {
        round("1\n");
        round("\"x\"\n");
        round("null\n");
    }

    #[test]
    fn strings_and_numbers_come_back_as_they_were_written() {
        round("[ \"\\u0041\\/\\n\", 1E5, -0, 2.50 ]\n");
        let d = parse(b"[\"caf\xe9\"]").unwrap();
        assert_eq!(write(&d).unwrap(), b"[ \"caf\xe9\" ]\n");
    }

    #[test]
    fn keys_keep_the_order_they_were_read_in() {
        round("{\n   \"b\" : 1,\n   \"a\" : 2\n}\n");
    }

    #[test]
    fn a_byte_order_mark_is_written_back() {
        let d = parse(b"\xEF\xBB\xBF{}\n").unwrap();
        assert_eq!(write(&d).unwrap(), b"\xEF\xBB\xBF{}\n");
    }

    /// Arrays nested `n` deep around `inner`.
    fn nested(n: usize, inner: Value) -> Value {
        (0..n).fold(inner, |v, _| Value::Array(vec![v]))
    }

    #[test]
    fn documents_built_too_deep_are_refused() {
        let at_limit = doc(nested(MAX_DEPTH, Value::Null));
        let written = write(&at_limit).unwrap();
        assert_eq!(parse(&written).unwrap(), at_limit);
        // An empty container counts as a level too, as it does when read.
        let empty_at_limit = doc(nested(MAX_DEPTH - 1, Value::Array(vec![])));
        assert!(write(&empty_at_limit).is_ok());
        for past in [
            nested(MAX_DEPTH + 1, Value::Null),
            nested(MAX_DEPTH, Value::Object(Object::new())),
            nested(1000, num("1")),
        ] {
            assert_eq!(
                write(&doc(past)),
                Err(JslotError::TooDeep { limit: MAX_DEPTH })
            );
        }
        let mut deep_member = Object::new();
        deep_member.insert("a", nested(MAX_DEPTH, Value::Null));
        assert_eq!(
            write(&doc(Value::Object(deep_member))),
            Err(JslotError::TooDeep { limit: MAX_DEPTH })
        );
    }

    #[test]
    fn output_past_max_output_is_refused() {
        // Indent multiplies: 700,000 nulls 31 levels in are about 99 bytes
        // a line, some 69 MB, from well under that in memory.
        let inner = Value::Array(vec![Value::Null; 700_000]);
        let d = doc(nested(MAX_DEPTH - 2, inner));
        assert_eq!(write(&d), Err(JslotError::TooLarge { limit: MAX_OUTPUT }));
        // One long string, ending exactly at the limit with its quotes and
        // line feed, and one byte longer.
        let fits = doc(Value::String(Str::new(&"a".repeat(MAX_OUTPUT - 3))));
        assert_eq!(write(&fits).unwrap().len(), MAX_OUTPUT);
        let over = doc(Value::String(Str::new(&"a".repeat(MAX_OUTPUT - 2))));
        assert_eq!(
            write(&over),
            Err(JslotError::TooLarge { limit: MAX_OUTPUT })
        );
    }

    #[test]
    fn other_layouts_are_rewritten_in_racemenu_s() {
        let d = parse(b"{\"b\":[1,2],\r\n\t\"a\":{}}").unwrap();
        assert_eq!(
            write(&d).unwrap(),
            b"{\n   \"b\" : [ 1, 2 ],\n   \"a\" : {}\n}\n"
        );
    }
}
