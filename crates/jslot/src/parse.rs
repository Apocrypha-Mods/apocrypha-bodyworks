//! Reading a preset into a [`Document`].

use crate::value::{hex4, Document, Number, Object, Str, Value};
use crate::{JslotError, MAX_DEPTH, MAX_INPUT};
use std::collections::BTreeSet;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Reads a preset. It must be one JSON value (RaceMenu's is an object) with
/// nothing after it but whitespace, optionally after a UTF-8 byte order
/// mark, which the document remembers. A file longer than [`MAX_INPUT`] is
/// refused before any of it is read.
///
/// Stricter than JSON in one way: an object may not name the same key
/// twice. RaceMenu's writer cannot produce that (its objects are sorted
/// maps), and which of the two a reader would take is not something to
/// guess. Comments, which some JSON readers allow, are refused too; no
/// preset has been seen with one. Either may be loaded by RaceMenu from a
/// hand-edited preset, so this can refuse a file the game accepts. String
/// bytes need not be UTF-8 and are kept as they are, but raw control
/// characters in them are refused, as JSON says.
///
/// Errors give a line and a column, both from 1, the column in bytes and
/// counted from after a byte order mark.
pub fn parse(bytes: &[u8]) -> Result<Document, JslotError> {
    if bytes.len() > MAX_INPUT {
        return Err(JslotError::TooLarge { limit: MAX_INPUT });
    }
    let bom = bytes.starts_with(BOM);
    let start = if bom { BOM.len() } else { 0 };
    let mut p = Parser {
        s: bytes,
        start,
        pos: start,
    };
    p.whitespace();
    let root = p.value(0)?;
    p.whitespace();
    if p.pos != bytes.len() {
        return Err(p.error("something follows the end of the preset"));
    }
    Ok(Document { root, bom })
}

struct Parser<'a> {
    s: &'a [u8],
    /// Where the JSON starts: after a byte order mark, if there is one.
    start: usize,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn error(&self, reason: &'static str) -> JslotError {
        self.error_at(self.pos, reason)
    }

    fn error_at(&self, pos: usize, reason: &'static str) -> JslotError {
        let before = &self.s[self.start..pos.clamp(self.start, self.s.len())];
        let line_start = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        JslotError::Malformed {
            line: before.iter().filter(|&&b| b == b'\n').count() + 1,
            column: before.len() - line_start + 1,
            reason,
        }
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, JslotError> {
        match self.peek() {
            Some(b'{') => self.object(depth + 1).map(Value::Object),
            Some(b'[') => self.array(depth + 1).map(Value::Array),
            Some(b'"') => self.string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.number().map(Value::Number),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            None => Err(self.error("the file ends where a value should be")),
            Some(_) => Err(self.error("expected a value")),
        }
    }

    fn literal(&mut self, word: &[u8], value: Value) -> Result<Value, JslotError> {
        if self.s[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Object, JslotError> {
        if depth > MAX_DEPTH {
            return Err(self.error("arrays and objects nest too deeply"));
        }
        self.pos += 1;
        let mut members = Vec::new();
        let mut seen = BTreeSet::new();
        self.whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Object::from_members(members));
        }
        loop {
            self.whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a key in quotes"));
            }
            let key_at = self.pos;
            let key = self.string()?;
            if !seen.insert(key.bytes().into_owned()) {
                return Err(self.error_at(key_at, "an object names the same key twice"));
            }
            self.whitespace();
            if self.peek() != Some(b':') {
                return Err(self.error("expected ':' after a key"));
            }
            self.pos += 1;
            self.whitespace();
            let value = self.value(depth)?;
            members.push((key, value));
            self.whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Object::from_members(members));
                }
                _ => return Err(self.error("expected ',' or '}' after a member")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Vec<Value>, JslotError> {
        if depth > MAX_DEPTH {
            return Err(self.error("arrays and objects nest too deeply"));
        }
        self.pos += 1;
        let mut items = Vec::new();
        self.whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(items);
        }
        loop {
            self.whitespace();
            items.push(self.value(depth)?);
            self.whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(items);
                }
                _ => return Err(self.error("expected ',' or ']' after an item")),
            }
        }
    }

    /// A string from its opening quote; the escapes are checked here so
    /// that [`Str`] can trust them.
    fn string(&mut self) -> Result<Str, JslotError> {
        self.pos += 1;
        let start = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.error_at(start - 1, "a string is not closed")),
                Some(b'"') => {
                    let s = Str::from_source(&self.s[start..self.pos]);
                    self.pos += 1;
                    return Ok(s);
                }
                Some(b'\\') => {
                    let at = self.pos;
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.pos += 1
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            let code = self.hex4(at)?;
                            if (0xDC00..0xE000).contains(&code) {
                                return Err(self.error_at(at, "a \\u escape is half a pair"));
                            }
                            if (0xD800..0xDC00).contains(&code) {
                                let low_at = self.pos;
                                if !self.s[self.pos..].starts_with(b"\\u") {
                                    return Err(self.error_at(at, "a \\u escape is half a pair"));
                                }
                                self.pos += 2;
                                let low = self.hex4(low_at)?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.error_at(at, "a \\u escape is half a pair"));
                                }
                            }
                        }
                        _ => return Err(self.error_at(at, "a string has an unknown escape")),
                    }
                }
                Some(0x00..=0x1F) => {
                    return Err(self.error("a string holds a control character"));
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn hex4(&mut self, escape_at: usize) -> Result<u32, JslotError> {
        let code = hex4(self.s.get(self.pos..self.pos + 4))
            .ok_or_else(|| self.error_at(escape_at, "a \\u escape needs four hex digits"))?;
        self.pos += 4;
        Ok(code)
    }

    /// JSON's number grammar: an optional `-`, `0` or digits not starting
    /// with `0`, then optionally `.` and digits, then optionally `e` or `E`,
    /// a sign and digits.
    fn number(&mut self) -> Result<Number, JslotError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.error_at(start, "a number is malformed")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error_at(start, "a number is malformed"));
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error_at(start, "a number is malformed"));
            }
            self.digits();
        }
        if matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
        ) {
            return Err(self.error_at(start, "a number is malformed"));
        }
        // Only ASCII has been accepted above.
        let text = std::str::from_utf8(&self.s[start..self.pos]).unwrap_or("0");
        Ok(Number::from_source(text))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(input: &[u8]) -> &'static str {
        match parse(input) {
            Err(JslotError::Malformed { reason, .. }) => reason,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn values_of_every_kind_are_read() {
        let d = parse(br#"{"a" : [ 1, -2.5e+3, "x", true, false, null, [], {} ]}"#).unwrap();
        assert!(!d.bom);
        let a = d
            .root
            .as_object()
            .unwrap()
            .get("a")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(a.len(), 8);
        assert_eq!(a[1].as_number().unwrap().text(), "-2.5e+3");
        assert_eq!(a[2].as_str().unwrap().text().unwrap(), "x");
        assert_eq!(a[3], Value::Bool(true));
        assert_eq!(a[4], Value::Bool(false));
        assert_eq!(a[5], Value::Null);
        assert_eq!(a[6], Value::Array(vec![]));
        assert_eq!(a[7], Value::Object(Object::new()));
    }

    #[test]
    fn a_byte_order_mark_is_accepted_and_remembered() {
        let d = parse(b"\xEF\xBB\xBF{}\n").unwrap();
        assert!(d.bom);
        assert_eq!(d.root, Value::Object(Object::new()));
    }

    #[test]
    fn number_text_is_kept_exactly() {
        for text in [
            "0",
            "-0",
            "0.3100000023841858",
            "1E5",
            "1e-05",
            "-0.0",
            "4294967295",
            "18446744073709551616",
        ] {
            let d = parse(format!("[{text}]").as_bytes()).unwrap();
            assert_eq!(
                d.root.as_array().unwrap()[0].as_number().unwrap().text(),
                text
            );
        }
    }

    #[test]
    fn malformed_numbers_are_refused() {
        for text in [
            "01", "-", "1.", ".5", "1e", "1e+", "+1", "--1", "1.2.3", "0x10", "1-2", "NaN",
            "Infinity",
        ] {
            assert!(parse(format!("[{text}]").as_bytes()).is_err(), "{text}");
        }
    }

    #[test]
    fn malformed_strings_are_refused() {
        assert_eq!(reason(b"[\"abc"), "a string is not closed");
        assert_eq!(reason(b"[\"a\\x\"]"), "a string has an unknown escape");
        assert_eq!(
            reason(b"[\"a\\u12\"]"),
            "a \\u escape needs four hex digits"
        );
        assert_eq!(reason(b"[\"a\\uDC00\"]"), "a \\u escape is half a pair");
        assert_eq!(reason(b"[\"a\\uD800\"]"), "a \\u escape is half a pair");
        assert_eq!(
            reason(b"[\"a\\uD800\\u0041\"]"),
            "a \\u escape is half a pair"
        );
        assert_eq!(reason(b"[\"a\nb\"]"), "a string holds a control character");
        assert!(parse(b"[\"a\\uD800\\uDC00\"]").is_ok());
    }

    #[test]
    fn bytes_that_are_not_utf8_are_kept_in_strings_only() {
        let d = parse(b"[\"caf\xe9\"]").unwrap();
        assert_eq!(
            d.root.as_array().unwrap()[0].as_str().unwrap().raw(),
            b"caf\xe9"
        );
        assert!(parse(b"[\xe9]").is_err());
    }

    #[test]
    fn broken_structure_is_refused_with_where_it_is() {
        assert_eq!(
            parse(b"{\n   \"a\" : 1,\n}"),
            Err(JslotError::Malformed {
                line: 3,
                column: 1,
                reason: "expected a key in quotes"
            })
        );
        assert_eq!(reason(b""), "the file ends where a value should be");
        assert_eq!(reason(b"   "), "the file ends where a value should be");
        assert_eq!(reason(b"[1,]"), "expected a value");
        assert_eq!(reason(b"[1 2]"), "expected ',' or ']' after an item");
        assert_eq!(reason(b"{\"a\" 1}"), "expected ':' after a key");
        assert_eq!(
            reason(b"{\"a\" : 1 \"b\" : 2}"),
            "expected ',' or '}' after a member"
        );
        assert_eq!(reason(b"{1 : 2}"), "expected a key in quotes");
        assert_eq!(reason(b"{} {}"), "something follows the end of the preset");
        assert_eq!(
            reason(b"{} // note"),
            "something follows the end of the preset"
        );
        assert_eq!(reason(b"tru"), "expected a value");
        assert_eq!(
            reason(b"{\"a\" : 1, \"a\" : 2}"),
            "an object names the same key twice"
        );
        assert_eq!(
            reason(b"{\"a\" : 1, \"\\u0061\" : 2}"),
            "an object names the same key twice"
        );
    }

    #[test]
    fn input_up_to_max_input_is_read_and_longer_is_refused() {
        // A string filling the file exactly: quotes, text and a line feed.
        let mut at_limit = vec![b'a'; MAX_INPUT];
        at_limit[0] = b'"';
        at_limit[MAX_INPUT - 2] = b'"';
        at_limit[MAX_INPUT - 1] = b'\n';
        assert!(parse(&at_limit).is_ok());
        at_limit.push(b' ');
        assert_eq!(
            parse(&at_limit),
            Err(JslotError::TooLarge { limit: MAX_INPUT })
        );
        // Refused before reading: even bytes that are not JSON at all.
        assert_eq!(
            parse(&vec![0xFF; MAX_INPUT + 1]),
            Err(JslotError::TooLarge { limit: MAX_INPUT })
        );
    }

    #[test]
    fn columns_count_from_after_a_byte_order_mark() {
        let at = |input: &[u8]| match parse(input) {
            Err(JslotError::Malformed { line, column, .. }) => (line, column),
            other => panic!("{other:?}"),
        };
        assert_eq!(at(b"\xEF\xBB\xBFx"), (1, 1));
        assert_eq!(at(b"\xEF\xBB\xBF[ 1 x ]"), (1, 5));
        assert_eq!(at(b"\xEF\xBB\xBF"), (1, 1));
        // Later lines are not affected.
        assert_eq!(at(b"\xEF\xBB\xBF[\n x ]"), (2, 2));
        // Without one, the first byte is column 1 as before.
        assert_eq!(at(b"x"), (1, 1));
        assert_eq!(
            parse(b"\xEF\xBB\xBF[ 01 ]").unwrap_err().to_string(),
            "can't read this preset: a number is malformed (line 1, column 3)"
        );
    }

    #[test]
    fn deep_nesting_is_refused_without_overflowing() {
        let ok = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(ok.as_bytes()).is_ok());
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert_eq!(
            reason(deep.as_bytes()),
            "arrays and objects nest too deeply"
        );
        assert_eq!(
            reason("[{\"a\":".repeat(100_000).as_bytes()),
            "arrays and objects nest too deeply"
        );
    }

    #[test]
    fn every_prefix_of_a_preset_is_refused_without_panicking() {
        let text = "\u{feff}{\n   \"a\" : [ 1, -2.5e+3, \"x\\\\y\\u00e9\\uD83D\\uDE00\" ],\n   \"b\" : { \"c\" : true, \"d\" : null }\n}\n";
        let bytes = text.as_bytes();
        assert!(parse(bytes).is_ok());
        for end in 0..bytes.len() {
            let r = parse(&bytes[..end]);
            // Cutting the last newline leaves a whole document.
            assert_eq!(r.is_ok(), end == bytes.len() - 1, "{end}");
        }
        // And every single byte changed to every other byte.
        let mut copy = bytes.to_vec();
        for i in 0..copy.len() {
            for b in 0..=255u8 {
                let was = copy[i];
                copy[i] = b;
                let _ = parse(&copy);
                copy[i] = was;
            }
        }
    }
}
