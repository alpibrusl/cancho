//! The JSON value, its reader and its writer
//! (`docs/structured-ingest.md` \u00a72).
//!
//! Kept apart from the codec so each file is one concern: this file is
//! JSON the format \u2014 a value tree with byte positions, one diagnostic
//! for the first thing that is not JSON \u2014 and `ingest.rs` is the
//! translation between that tree and the AST. A position is carried on
//! every value because a refusal has to point into the text the reader
//! was handed, which is the only file there is.

use crate::rules::Rule;
use crate::span::{Diagnostic, Span};
use std::fmt::Write as _;

// ---- the JSON value --------------------------------------------------------

/// A JSON value, kept with the byte range it came from so a refusal can
/// point at the text the reader actually wrote.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null(Span),
    Bool(bool, Span),
    Int(i64, Span),
    /// A number with a fraction or an exponent, or one out of `i64`
    /// range: an integer node has no use for it, and every other field
    /// is a string, so it is kept as its text and refused by the field
    /// that wanted something else.
    Number(String, Span),
    Str(String, Span),
    Array(Vec<Json>, Span),
    Object(Vec<(String, Json)>, Span),
}

impl Json {
    /// The byte range this value occupies in the source JSON.
    pub fn span(&self) -> Span {
        match self {
            Json::Null(s)
            | Json::Bool(_, s)
            | Json::Int(_, s)
            | Json::Number(_, s)
            | Json::Str(_, s)
            | Json::Array(_, s)
            | Json::Object(_, s) => *s,
        }
    }

    /// The value a key names. `None` covers both an absent key and a
    /// non-object, which the caller distinguishes by its own arity
    /// check on the object it already required.
    /// The value a key names. `None` covers both an absent key and a
    /// non-object, which the caller distinguishes by its own arity
    // check on the object it already required.
    pub(crate) fn field(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries, _) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

// ---- the reader ------------------------------------------------------------

/// Read JSON text into a [`Json`] tree. One diagnostic for the first
/// thing that is not JSON, positioned in the text that was read.
pub fn read_json(text: &str) -> Result<Json, Diagnostic> {
    let mut at = 0usize;
    let value = read_value(text.as_bytes(), text, &mut at)?;
    skip_ws(text.as_bytes(), &mut at);
    if at != text.len() {
        return Err(Diagnostic::new(
            Rule::IngestJson,
            "a second JSON value where the file ends",
            Span::new(at as u32, at as u32 + 1),
        ));
    }
    Ok(value)
}

fn skip_ws(bytes: &[u8], at: &mut usize) {
    while *at < bytes.len() && bytes[*at].is_ascii_whitespace() {
        *at += 1;
    }
}

fn read_value(bytes: &[u8], text: &str, at: &mut usize) -> Result<Json, Diagnostic> {
    skip_ws(bytes, at);
    let start = *at;
    let Some(&b) = bytes.get(start) else {
        return Err(Diagnostic::new(
            Rule::IngestJson,
            "the JSON ends where a value must begin",
            Span::new(start as u32, start as u32 + 1),
        ));
    };
    match b {
        b'{' => {
            *at += 1;
            let mut entries = Vec::new();
            skip_ws(bytes, at);
            if bytes.get(*at) == Some(&b'}') {
                *at += 1;
                return Ok(Json::Object(entries, Span::new(start as u32, *at as u32)));
            }
            loop {
                skip_ws(bytes, at);
                let key = match read_value(bytes, text, at)? {
                    Json::Str(key, _) => key,
                    other => {
                        return Err(Diagnostic::new(
                            Rule::IngestJson,
                            "an object's key must be a string",
                            other.span(),
                        ));
                    }
                };
                if entries.iter().any(|(k, _): &(String, Json)| *k == key) {
                    return Err(Diagnostic::new(
                        Rule::IngestJson,
                        format!("the key `{key}` is written twice"),
                        Span::new(start as u32, *at as u32),
                    ));
                }
                skip_ws(bytes, at);
                if bytes.get(*at) != Some(&b':') {
                    return Err(Diagnostic::new(
                        Rule::IngestJson,
                        "an object's key must be followed by `:`",
                        Span::new(*at as u32, *at as u32 + 1),
                    ));
                }
                *at += 1;
                let value = read_value(bytes, text, at)?;
                entries.push((key, value));
                skip_ws(bytes, at);
                match bytes.get(*at) {
                    Some(&b',') => *at += 1,
                    Some(&b'}') => {
                        *at += 1;
                        return Ok(Json::Object(entries, Span::new(start as u32, *at as u32)));
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            Rule::IngestJson,
                            "an object's entries are separated by `,` and closed by `}`",
                            Span::new(*at as u32, *at as u32 + 1),
                        ));
                    }
                }
            }
        }
        b'[' => {
            *at += 1;
            let mut items = Vec::new();
            skip_ws(bytes, at);
            if bytes.get(*at) == Some(&b']') {
                *at += 1;
                return Ok(Json::Array(items, Span::new(start as u32, *at as u32)));
            }
            loop {
                items.push(read_value(bytes, text, at)?);
                skip_ws(bytes, at);
                match bytes.get(*at) {
                    Some(&b',') => *at += 1,
                    Some(&b']') => {
                        *at += 1;
                        return Ok(Json::Array(items, Span::new(start as u32, *at as u32)));
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            Rule::IngestJson,
                            "an array's items are separated by `,` and closed by `]`",
                            Span::new(*at as u32, *at as u32 + 1),
                        ));
                    }
                }
            }
        }
        b'"' => {
            *at += 1;
            let mut out = String::new();
            while let Some(&b) = bytes.get(*at) {
                match b {
                    b'"' => {
                        *at += 1;
                        return Ok(Json::Str(out, Span::new(start as u32, *at as u32)));
                    }
                    b'\\' => {
                        *at += 1;
                        let Some(&esc) = bytes.get(*at) else {
                            return Err(Diagnostic::new(
                                Rule::IngestJson,
                                "a string's escape has nothing after it",
                                Span::new(*at as u32, *at as u32),
                            ));
                        };
                        *at += 1;
                        match esc {
                            b'"' => out.push('"'),
                            b'\\' => out.push('\\'),
                            b'/' => out.push('/'),
                            b'b' => out.push('\u{8}'),
                            b'f' => out.push('\u{c}'),
                            b'n' => out.push('\n'),
                            b'r' => out.push('\r'),
                            b't' => out.push('\t'),
                            b'u' => {
                                let mut code = 0u32;
                                for _ in 0..4 {
                                    let Some(&hex) = bytes.get(*at) else {
                                        return Err(Diagnostic::new(
                                            Rule::IngestJson,
                                            "a `\\u` escape needs four hex digits",
                                            Span::new(*at as u32, *at as u32 + 1),
                                        ));
                                    };
                                    let Some(digit) = (hex as char).to_digit(16) else {
                                        return Err(Diagnostic::new(
                                            Rule::IngestJson,
                                            "a `\\u` escape needs four hex digits",
                                            Span::new(*at as u32, *at as u32 + 1),
                                        ));
                                    };
                                    code = code * 16 + digit;
                                    *at += 1;
                                }
                                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            }
                            other => {
                                return Err(Diagnostic::new(
                                    Rule::IngestJson,
                                    format!("`\\{}` is not a JSON escape", other as char),
                                    Span::new(*at as u32 - 1, *at as u32),
                                ));
                            }
                        }
                    }
                    _ => {
                        let ch = text[*at..].chars().next().unwrap_or('\u{fffd}');
                        *at += ch.len_utf8();
                        out.push(ch);
                    }
                }
            }
            Err(Diagnostic::new(
                Rule::IngestJson,
                "a string ends where the file ends",
                Span::new(start as u32, *at as u32),
            ))
        }
        b't' | b'f' | b'n' => {
            for (spelling, value) in [
                ("true", Json::Bool(true, Span::new(start as u32, start as u32 + 4))),
                ("false", Json::Bool(false, Span::new(start as u32, start as u32 + 5))),
                ("null", Json::Null(Span::new(start as u32, start as u32 + 4))),
            ] {
                if text[start..].starts_with(spelling) {
                    *at += spelling.len();
                    return Ok(value);
                }
            }
            Err(Diagnostic::new(
                Rule::IngestJson,
                format!("`{}` is not JSON", text[start..].chars().take(8).collect::<String>()),
                Span::new(start as u32, start as u32 + 1),
            ))
        }
        _ if b == b'-' || b.is_ascii_digit() => {
            while *at < bytes.len()
                && (bytes[*at].is_ascii_digit()
                    || matches!(bytes[*at], b'-' | b'+' | b'.' | b'e' | b'E'))
            {
                *at += 1;
            }
            let word = &text[start..*at];
            // An integer that fits `i64` is one; anything else — a bit
            // pattern above `i64::MAX`, a fraction, an exponent — keeps its
            // text, and the field that reads it decides whether that is a
            // refusal (`ingest-arity` there, with its own message) rather
            // than the reader guessing what the writer meant.
            if word.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
                match word.parse::<i64>() {
                    Ok(value) => Ok(Json::Int(value, Span::new(start as u32, *at as u32))),
                    Err(_) => {
                        Ok(Json::Number(word.to_owned(), Span::new(start as u32, *at as u32)))
                    }
                }
            } else {
                Ok(Json::Number(word.to_owned(), Span::new(start as u32, *at as u32)))
            }
        }
        other => Err(Diagnostic::new(
            Rule::IngestJson,
            format!("`{}` is not the start of a JSON value", other as char),
            Span::new(start as u32, start as u32 + 1),
        )),
    }
}

// ---- the writer ------------------------------------------------------------

/// Render a [`Json`] tree back to text, for a fixture or a golden file.
pub fn write_json(json: &Json) -> String {
    let mut out = String::new();
    write_at(json, 0, &mut out);
    out
}

fn write_at(json: &Json, depth: usize, out: &mut String) {
    match json {
        Json::Null(_) => out.push_str("null"),
        Json::Bool(value, _) => out.push_str(if *value { "true" } else { "false" }),
        Json::Int(value, _) => {
            let _ = write!(out, "{value}");
        }
        Json::Number(text, _) => out.push_str(text),
        Json::Str(text, _) => write_json_string(text, out),
        Json::Array(items, _) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                indent(out, depth + 1);
                write_at(item, depth + 1, out);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push(']');
        }
        Json::Object(entries, _) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (index, (key, value)) in entries.iter().enumerate() {
                indent(out, depth + 1);
                write_json_string(key, out);
                out.push_str(": ");
                write_at(value, depth + 1, out);
                if index + 1 < entries.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push('}');
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// A JSON string with only the escapes JSON itself needs. The six
/// `.cho` escapes are a *text* convention (`strings.md` §4); this is
/// the data convention, and the two never meet.
fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}
