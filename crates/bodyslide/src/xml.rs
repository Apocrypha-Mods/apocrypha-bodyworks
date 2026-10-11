//! The little of XML these files need: reading through `roxmltree`, and
//! writing text and attributes with what must be escaped, escaped.

use crate::Error;

/// Parse a document, with or without a byte-order mark, whatever its
/// declared encoding claims: these files are UTF-8, or Windows-1252 from
/// older tools, and a byte that is not UTF-8 is read as the character it
/// most likely was rather than refusing the whole file.
pub fn text_of(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|b| *b as char).collect(),
    };
    spaced(&without_comments(&text))
}

/// Files written by hand also run one attribute into the next
/// (`name="a"displayname="b"`), which the original program reads and a strict
/// reader refuses. A space is put back between them.
fn spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let (mut in_tag, mut quote) = (false, None::<char>);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        match (in_tag, quote) {
            (false, _) if c == '<' => in_tag = true,
            (true, None) if c == '>' => in_tag = false,
            (true, None) if c == '"' || c == '\'' => quote = Some(c),
            (true, Some(q)) if c == q => {
                quote = None;
                if chars.peek().is_some_and(|n| n.is_alphabetic()) {
                    out.push(' ');
                }
            }
            _ => {}
        }
    }
    out
}

/// The text with its comments taken out. Files written by hand have comments
/// a strict reader refuses (`--` inside one), and nothing here reads them.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            // Never closed: the rest of the file is the comment.
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

pub fn parse(text: &str) -> Result<roxmltree::Document<'_>, Error> {
    roxmltree::Document::parse(text).map_err(|e| Error::Xml(e.to_string()))
}

/// Text made safe to put between tags or inside a quoted attribute.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// "true" and "false", as these files spell them, in any letter-case.
pub fn flag(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_are_dropped_even_ones_a_strict_reader_refuses() {
        let text = text_of(b"\xef\xbb\xbf<a><!-- one -- two --><b/><!-- never closed");
        assert_eq!(text, "<a><b/>");
        assert!(parse("<a><b/></a>").is_ok());
    }

    #[test]
    fn attributes_run_together_are_parted() {
        let text = text_of(b"<a><b name=\"x\"shown=\"y\" /><c>\"quoted\"text</c></a>");
        assert_eq!(
            text,
            "<a><b name=\"x\" shown=\"y\" /><c>\"quoted\"text</c></a>"
        );
        let doc = parse(&text).unwrap();
        let b = doc.root_element().first_element_child().unwrap();
        assert_eq!(b.attribute("shown"), Some("y"));
    }

    #[test]
    fn text_is_escaped_for_tags_and_attributes() {
        assert_eq!(
            escape("Tom & \"Jerry\" <3>"),
            "Tom &amp; &quot;Jerry&quot; &lt;3&gt;"
        );
        assert!(flag(Some("TRUE")) && !flag(Some("no")) && !flag(None));
    }
}
