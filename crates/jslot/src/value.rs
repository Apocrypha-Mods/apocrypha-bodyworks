//! The document model: JSON values that remember how they were written.

use std::borrow::Cow;

/// A whole preset as read: its top value, and whether the file started
/// with a UTF-8 byte order mark (RaceMenu's own files do not; one that does
/// gets it back when written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub root: Value,
    pub bom: bool,
}

/// One JSON value. Equality is textual: `1` and `1.0` are different numbers,
/// and `"A"` and `"A"` different strings, because they write
/// differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(Str),
    Array(Vec<Value>),
    Object(Object),
}

impl Value {
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&Str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

/// A number, kept as its text. A read number is always valid JSON number
/// text; a new one is written the way RaceMenu writes its own.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Number(String);

impl Number {
    /// Text the parser has already checked against JSON's number grammar.
    pub(crate) fn from_source(text: &str) -> Number {
        Number(text.to_string())
    }

    /// The number as it is written.
    pub fn text(&self) -> &str {
        &self.0
    }

    /// The nearest double. A number too large for one is infinite.
    pub fn as_f64(&self) -> f64 {
        self.0.parse().unwrap_or(f64::NAN)
    }

    /// The value, when it is written as a whole number (no `.` or exponent)
    /// that fits.
    pub fn as_i64(&self) -> Option<i64> {
        self.is_whole().then(|| self.0.parse().ok()).flatten()
    }

    /// The value, when it is written as a whole number that fits.
    pub fn as_u64(&self) -> Option<u64> {
        self.is_whole().then(|| self.0.parse().ok()).flatten()
    }

    fn is_whole(&self) -> bool {
        !self.0.contains(['.', 'e', 'E'])
    }

    /// A whole number, as plain decimal.
    pub fn from_i64(value: i64) -> Number {
        Number(value.to_string())
    }

    /// A whole number, as plain decimal (`4294967295`, not `-1`).
    pub fn from_u64(value: u64) -> Number {
        Number(value.to_string())
    }

    /// A double as RaceMenu writes one: C's `%.16g`, so `0.31` is `0.31`,
    /// `2.0` is `2` and `1e-5` is `1e-05`. None for NaN and the infinities,
    /// which JSON cannot hold.
    pub fn from_f64(value: f64) -> Option<Number> {
        value.is_finite().then(|| Number(g16(value)))
    }

    /// A float as RaceMenu writes the floats it stores (slider values,
    /// weights, scales): widened to a double and written by
    /// [`Number::from_f64`], so `0.31` is `0.3100000023841858`.
    pub fn from_f32(value: f32) -> Option<Number> {
        Number::from_f64(f64::from(value))
    }
}

/// C's `%.16g`: 16 significant digits, trailing zeros dropped, in
/// exponent form (`e+38`, `e-05`: a sign and at least two digits) when the
/// exponent is below -4 or at least 16, plain otherwise. Rust's `{:e}`
/// rounds exactly, as C's does, so taking the digits from it gives the same
/// ones.
fn g16(value: f64) -> String {
    const PRECISION: i32 = 16;
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let e = format!("{:.*e}", (PRECISION - 1) as usize, value.abs());
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let mut out = String::new();
    if value < 0.0 {
        out.push('-');
    }
    if !(-4..PRECISION).contains(&exp) {
        let (first, rest) = digits.split_at(1);
        out.push_str(first);
        let rest = rest.trim_end_matches('0');
        if !rest.is_empty() {
            out.push('.');
            out.push_str(rest);
        }
        out.push('e');
        out.push(if exp < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", exp.unsigned_abs()));
    } else if exp >= 0 {
        let (whole, fraction) = digits.split_at(exp as usize + 1);
        out.push_str(whole);
        let fraction = fraction.trim_end_matches('0');
        if !fraction.is_empty() {
            out.push('.');
            out.push_str(fraction);
        }
    } else {
        out.push_str("0.");
        for _ in 0..(-exp - 1) {
            out.push('0');
        }
        out.push_str(digits.trim_end_matches('0'));
    }
    out
}

/// A string, kept as the bytes between its quotes, escapes and all. The
/// bytes need not be UTF-8: a preset written on a system using another code
/// page could hold any, and they are kept as they are.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Str(Vec<u8>);

impl Str {
    /// Bytes the parser has already checked: no raw control characters, and
    /// only JSON's escapes, with `\u` surrogates in pairs.
    pub(crate) fn from_source(raw: &[u8]) -> Str {
        Str(raw.to_vec())
    }

    /// Text as RaceMenu's writer escapes it: `"` and `\` with a backslash,
    /// backspace, form feed, line feed, carriage return and tab as `\b`,
    /// `\f`, `\n`, `\r`, `\t`, other control characters as `\u` and four
    /// upper-case hex digits, and everything else, `/` and non-ASCII
    /// included, as it is. Only `\\` has been seen in a preset; the rest is
    /// jsoncpp's documented behaviour.
    pub fn new(text: &str) -> Str {
        let mut raw = Vec::with_capacity(text.len());
        for &b in text.as_bytes() {
            match b {
                b'"' => raw.extend_from_slice(b"\\\""),
                b'\\' => raw.extend_from_slice(b"\\\\"),
                0x08 => raw.extend_from_slice(b"\\b"),
                0x0C => raw.extend_from_slice(b"\\f"),
                b'\n' => raw.extend_from_slice(b"\\n"),
                b'\r' => raw.extend_from_slice(b"\\r"),
                b'\t' => raw.extend_from_slice(b"\\t"),
                0x00..=0x1F => raw.extend_from_slice(format!("\\u{b:04X}").as_bytes()),
                _ => raw.push(b),
            }
        }
        Str(raw)
    }

    /// The bytes between the quotes, as written.
    pub fn raw(&self) -> &[u8] {
        &self.0
    }

    /// The string's bytes with its escapes undone (`\u` escapes become
    /// UTF-8).
    pub fn bytes(&self) -> Cow<'_, [u8]> {
        if self.0.contains(&b'\\') {
            Cow::Owned(unescape(&self.0))
        } else {
            Cow::Borrowed(&self.0)
        }
    }

    /// The string as text, or None when its bytes are not UTF-8.
    pub fn text(&self) -> Option<String> {
        String::from_utf8(self.bytes().into_owned()).ok()
    }

    /// Whether the string, escapes undone, is `key`.
    pub(crate) fn is(&self, key: &str) -> bool {
        self.bytes().as_ref() == key.as_bytes()
    }
}

/// Undoes JSON escapes. The parser has checked them, so nothing here is out
/// of place; it still never indexes past the end.
fn unescape(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        i += 1;
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let Some(&e) = raw.get(i) else { break };
        i += 1;
        match e {
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'u' => {
                let high = hex4(raw.get(i..i + 4)).unwrap_or(0xFFFD);
                i += 4;
                let mut code = high;
                if (0xD800..0xDC00).contains(&high) {
                    // A low surrogate follows as `\uXXXX`.
                    let low = hex4(raw.get(i + 2..i + 6)).unwrap_or(0xDC00);
                    i += 6;
                    code = 0x10000 + ((high - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
                }
                let c = char::from_u32(code).unwrap_or('\u{FFFD}');
                out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
            other => out.push(other),
        }
    }
    out
}

/// Four hex digits as a number.
pub(crate) fn hex4(digits: Option<&[u8]>) -> Option<u32> {
    let digits = digits?;
    if digits.len() != 4 {
        return None;
    }
    digits
        .iter()
        .try_fold(0u32, |n, &d| Some(n * 16 + (d as char).to_digit(16)?))
}

/// An object's members, in the order they were read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Object(Vec<(Str, Value)>);

impl Object {
    pub fn new() -> Object {
        Object(Vec::new())
    }

    /// Members the parser has read, already checked for repeated keys.
    pub(crate) fn from_members(members: Vec<(Str, Value)>) -> Object {
        Object(members)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The members in order.
    pub fn iter(&self) -> impl Iterator<Item = (&Str, &Value)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k.is(key)).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0.iter_mut().find(|(k, _)| k.is(key)).map(|(_, v)| v)
    }

    /// Sets `key` to `value`, giving back the value it replaces. A key that
    /// is already there keeps its place. A new one goes before the first key
    /// that sorts after it byte by byte, which is where RaceMenu's sorted
    /// objects have it.
    pub fn insert(&mut self, key: &str, value: Value) -> Option<Value> {
        if let Some(old) = self.get_mut(key) {
            return Some(std::mem::replace(old, value));
        }
        let at = self
            .0
            .iter()
            .position(|(k, _)| k.bytes().as_ref() > key.as_bytes())
            .unwrap_or(self.0.len());
        self.0.insert(at, (Str::new(key), value));
        None
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let at = self.0.iter().position(|(k, _)| k.is(key))?;
        Some(self.0.remove(at).1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_are_written_as_c_writes_sixteen_significant_digits() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "-0"),
            (1.0, "1"),
            (-1.0, "-1"),
            (2.5, "2.5"),
            (100.0, "100"),
            (0.1, "0.1"),
            (1e15, "1000000000000000"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.25e-7, "1.25e-07"),
            (1e100, "1e+100"),
            (1e-300, "1e-300"),
            (123456789.125, "123456789.125"),
            (1.0 / 3.0, "0.3333333333333333"),
            (2.0 / 3.0, "0.6666666666666666"),
            (-2.0 / 3.0, "-0.6666666666666666"),
            (4503599627370497.0, "4503599627370497"),
            (0.12345678901234567, "0.1234567890123457"),
            // The largest exponent written plainly is 15.
            (9999999999999998.0, "9999999999999998"),
            (9.999999999999999e-5, "9.999999999999999e-05"),
        ];
        for &(value, text) in cases {
            assert_eq!(Number::from_f64(value).unwrap().text(), text, "{value:e}");
        }
        assert_eq!(Number::from_f64(f64::NAN), None);
        assert_eq!(Number::from_f64(f64::INFINITY), None);
        assert_eq!(Number::from_f64(f64::NEG_INFINITY), None);
    }

    #[test]
    fn floats_are_widened_before_being_written() {
        assert_eq!(Number::from_f32(0.31).unwrap().text(), "0.3100000023841858");
        assert_eq!(Number::from_f32(1.05).unwrap().text(), "1.049999952316284");
        assert_eq!(
            Number::from_f32(f32::MAX).unwrap().text(),
            "3.402823466385289e+38"
        );
        assert_eq!(
            Number::from_f32(-0.01).unwrap().text(),
            "-0.009999999776482582"
        );
        assert_eq!(Number::from_f32(21.0).unwrap().text(), "21");
    }

    #[test]
    fn whole_numbers_are_plain_decimals() {
        assert_eq!(Number::from_u64(4294967295).text(), "4294967295");
        assert_eq!(Number::from_i64(-2236963).text(), "-2236963");
        assert_eq!(Number::from_u64(u64::MAX).as_u64(), Some(u64::MAX));
    }

    #[test]
    fn number_values_follow_their_text() {
        let n = Number::from_source("4294967295");
        assert_eq!(n.as_u64(), Some(4294967295));
        assert_eq!(n.as_i64(), Some(4294967295));
        assert_eq!(n.as_f64(), 4294967295.0);
        let n = Number::from_source("-1");
        assert_eq!(n.as_u64(), None);
        assert_eq!(n.as_i64(), Some(-1));
        let n = Number::from_source("2.0");
        assert_eq!(n.as_i64(), None);
        assert_eq!(n.as_f64(), 2.0);
        let n = Number::from_source("1e2");
        assert_eq!(n.as_u64(), None);
        assert_eq!(n.as_f64(), 100.0);
        assert_eq!(Number::from_source("1e400").as_f64(), f64::INFINITY);
        assert_eq!(Number::from_source("99999999999999999999").as_u64(), None);
    }

    #[test]
    fn new_strings_are_escaped_as_racemenu_escapes_them() {
        let s = Str::new("a\"b\\c/d\u{8}\u{c}\n\r\t\u{1}\u{1f}\u{7f}é");
        assert_eq!(
            s.raw(),
            "a\\\"b\\\\c/d\\b\\f\\n\\r\\t\\u0001\\u001F\u{7f}é".as_bytes()
        );
        assert_eq!(
            s.text().unwrap(),
            "a\"b\\c/d\u{8}\u{c}\n\r\t\u{1}\u{1f}\u{7f}é"
        );
    }

    #[test]
    fn escapes_are_undone() {
        let s = Str::from_source(br#"\"\\\/\b\f\n\r\t\u0041\u00e9\uD83D\uDE00"#);
        assert_eq!(s.text().unwrap(), "\"\\/\u{8}\u{c}\n\r\tAé\u{1F600}");
        let plain = Str::from_source(b"Actors");
        assert!(matches!(plain.bytes(), Cow::Borrowed(_)));
    }

    #[test]
    fn strings_that_are_not_utf8_are_kept_but_have_no_text() {
        let s = Str::from_source(b"caf\xe9");
        assert_eq!(s.bytes().as_ref(), b"caf\xe9");
        assert_eq!(s.text(), None);
    }

    #[test]
    fn new_keys_go_where_byte_order_puts_them_and_old_ones_stay() {
        let mut o = Object::new();
        for key in ["mods", "actor", "modNames", "Zed", "version"] {
            assert_eq!(o.insert(key, Value::Null), None);
        }
        let keys: Vec<String> = o.iter().map(|(k, _)| k.text().unwrap()).collect();
        assert_eq!(keys, ["Zed", "actor", "modNames", "mods", "version"]);
        let old = o.insert("actor", Value::Bool(true));
        assert_eq!(old, Some(Value::Null));
        assert_eq!(o.iter().nth(1).unwrap().1, &Value::Bool(true));
        assert_eq!(o.remove("Zed"), Some(Value::Null));
        assert_eq!(o.remove("Zed"), None);
        assert_eq!(o.len(), 4);
    }

    #[test]
    fn keys_are_found_by_their_text_not_their_escapes() {
        let o = Object::from_members(vec![(Str::from_source(br"ab"), Value::Null)]);
        assert_eq!(o.get("ab"), Some(&Value::Null));
    }
}
