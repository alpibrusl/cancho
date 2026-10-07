//! The formatter: the canonical printer, with the comments put back.
//!
//! `print` renders an AST, and an AST has no comments, no blank lines and
//! no literal spellings (`print.rs`'s own header says so, and says why:
//! none of them may reach a hash). A formatter cannot work from the AST
//! alone, so this works from the *text* too, and does not try to be clever
//! about it:
//!
//! 1. `print` the parsed file. That is the layout.
//! 2. Tokenise the source and the printed text, and **align** the two token
//!    streams. They differ in exactly three ways: the printer drops
//!    redundant parentheses, may add or drop a trailing comma, and spells
//!    literals in one way (`'a'`, `0x10` and `1_000` print as `97`, `16`
//!    and `1000`). Anything else is not a layout difference, and the
//!    formatter stops rather than guess.
//! 3. Put each comment on the output line of the token it sat next to: at
//!    the end of that line if it was written at the end of one, above the
//!    line of the next token otherwise. Keep one blank line where the
//!    source had one.
//! 4. Check the result, and **refuse** if any check fails (§ below).
//!
//! The checks are the point. A formatter that is sometimes wrong is worse
//! than none, so the output is only returned if it
//!
//! * parses, to an AST that prints exactly as the input's does (so no
//!   token changed meaning),
//! * contains the same comments, in the same order, and
//! * is a fixed point: formatting it again changes nothing.
//!
//! A file that fails a check is left as it was and reported, never
//! half-formatted. `docs/formatting.md` records what each check caught.

use crate::ast::Ast;
use crate::lexer::{Token, TokenKind, tokenize_with_comments};
use crate::parser::parse;
use crate::print::print;
use crate::span::{Diagnostic, Span};

/// Why a file was not formatted.
#[derive(Debug)]
pub enum FormatError {
    /// The file does not parse. Nothing can be said about its layout.
    Parse(Diagnostic),
    /// The file parses, but formatting it would move or lose something.
    /// The sentence says what; the file is unchanged.
    Cannot(String),
}

/// Where an output token landed.
#[derive(Clone, Copy)]
struct Loc {
    line: usize,
    last_on_line: bool,
}

const INDENT: &str = "    ";

/// Format one file. The result is canonical: formatting it again is a no-op.
pub fn format(text: &str) -> Result<String, FormatError> {
    let once = format_once(text)?;
    let twice = format_once(&once)?;
    if twice != once {
        return Err(FormatError::Cannot(
            "formatting its own output changes it again; the formatter is not a fixed point \
             on this file, which is a bug in the formatter, not in the file"
                .to_owned(),
        ));
    }
    Ok(once)
}

fn tokens_of(text: &str) -> Result<(Vec<Token>, Vec<Span>), FormatError> {
    tokenize_with_comments(text).map_err(FormatError::Parse)
}

fn slice(text: &str, span: Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

fn is_literal(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Int | TokenKind::Float | TokenKind::Str)
}

fn format_once(text: &str) -> Result<String, FormatError> {
    let (mut in_tokens, comments) = tokens_of(text)?;
    in_tokens.pop(); // Eof
    let ast: Ast = parse(text).map_err(FormatError::Parse)?;
    let printed = print(&ast);

    // `print` has no edition marker to print (`docs/editions.md` §6.1), and
    // dropping it would change what the file means. It is carried across as
    // the first line.
    let mut edition_line: Option<String> = None;
    let mut skip = 0;
    if in_tokens.len() >= 3
        && in_tokens[0].kind == TokenKind::Ident
        && slice(text, in_tokens[0].span) == "edition"
        && in_tokens[1].kind == TokenKind::Int
        && in_tokens[2].kind == TokenKind::Semi
    {
        edition_line = Some(format!("edition {};", slice(text, in_tokens[1].span)));
        skip = 3;
    }

    let (mut out_tokens, _) = tokens_of(&printed)?;
    out_tokens.pop();

    // Align, rewriting each literal to the way the source spelled it.
    let mut map: Vec<Option<usize>> = vec![None; in_tokens.len()];
    let mut replacements: Vec<(Span, &str)> = Vec::new();
    // `else { if .. }` prints as `else if ..`: the braces have no counterpart.
    // Decided as the alignment reaches them, because only the printer's
    // output says whether the block held nothing but the `if`.
    let mut dropped_braces: Vec<usize> = Vec::new();
    // `pub extern fn`: the parser reads and discards the `pub` -- a foreign
    // declaration has no visibility to keep (`ExternDecl` has no such field).
    let dropped_pub: Vec<usize> = (0..in_tokens.len().saturating_sub(1))
        .filter(|&n| {
            in_tokens[n].kind == TokenKind::Pub && in_tokens[n + 1].kind == TokenKind::Extern
        })
        .collect();
    let (mut i, mut j) = (skip, 0);
    while i < in_tokens.len() {
        let (a, b) = (in_tokens[i], out_tokens.get(j).copied());
        // Known drops first: a dropped `}` looks exactly like the `}` the
        // printer writes, and taking it as a match would drop the wrong one.
        if a.kind == TokenKind::LBrace
            && i > 0
            && in_tokens[i - 1].kind == TokenKind::Else
            && b.is_some_and(|b| b.kind == TokenKind::If)
        {
            let mut depth = 0usize;
            for (m, token) in in_tokens.iter().enumerate().skip(i) {
                match token.kind {
                    TokenKind::LBrace => depth += 1,
                    TokenKind::RBrace => {
                        depth -= 1;
                        if depth == 0 {
                            dropped_braces.push(m);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
            continue;
        }
        if dropped_braces.contains(&i) || dropped_pub.contains(&i) {
            i += 1;
            continue;
        }
        match b {
            Some(b) if a.kind == b.kind && is_literal(a.kind) => {
                let (src, out) = (slice(text, a.span), slice(&printed, b.span));
                if src != out {
                    replacements.push((b.span, src));
                }
                map[i] = Some(j);
                i += 1;
                j += 1;
            }
            Some(b) if a.kind == b.kind && slice(text, a.span) == slice(&printed, b.span) => {
                map[i] = Some(j);
                i += 1;
                j += 1;
            }
            // A redundant parenthesis, or a trailing comma, the printer did not write.
            _ if matches!(a.kind, TokenKind::LParen | TokenKind::RParen | TokenKind::Comma) => {
                i += 1;
            }
            // A trailing comma the printer wrote and the source did not.
            Some(b) if b.kind == TokenKind::Comma => j += 1,
            _ => {
                let line = text[..a.span.start as usize].matches('\n').count() + 1;
                // The printer writes every `import` before the first
                // declaration; one written further down has nowhere to stay.
                if b.is_some_and(|b| b.kind == TokenKind::Import) {
                    let late = in_tokens[i..].iter().find(|t| t.kind == TokenKind::Import);
                    let at = late
                        .map_or(line, |t| text[..t.span.start as usize].matches('\n').count() + 1);
                    return Err(FormatError::Cannot(format!(
                        "line {at}: an `import` after the first declaration; the canonical form \
                         puts every import before it, so move this one up"
                    )));
                }
                return Err(FormatError::Cannot(format!(
                    "line {line}: `{}` has no counterpart in the canonical rendering; the layout \
                     is not something this formatter can reproduce",
                    slice(text, a.span)
                )));
            }
        }
    }
    if out_tokens[j..].iter().any(|t| t.kind != TokenKind::Comma) {
        return Err(FormatError::Cannot(
            "the canonical rendering has tokens the source does not".to_owned(),
        ));
    }

    // Apply the literal spellings, then locate every output token.
    let mut respelled = String::with_capacity(printed.len());
    let mut at = 0usize;
    for (span, replacement) in &replacements {
        respelled.push_str(&printed[at..span.start as usize]);
        respelled.push_str(replacement);
        at = span.end as usize;
    }
    respelled.push_str(&printed[at..]);
    let (mut located, _) = tokens_of(&respelled)?;
    located.pop();

    let mut lines: Vec<String> = Vec::new();
    let edition_offset = usize::from(edition_line.is_some());
    if let Some(line) = &edition_line {
        lines.push(line.clone());
    }
    lines.extend(respelled.lines().map(str::to_owned));

    let line_of = |offset: u32| respelled[..offset as usize].matches('\n').count() + edition_offset;
    let locs: Vec<Loc> = located
        .iter()
        .enumerate()
        .map(|(n, token)| {
            let line = line_of(token.span.start);
            let last_on_line =
                located.get(n + 1).is_none_or(|next| line_of(next.span.start) != line);
            Loc { line, last_on_line }
        })
        .collect();
    let mut token_loc: Vec<Option<Loc>> = map.iter().map(|m| m.map(|j| locs[j])).collect();
    // The edition marker's three tokens sit on the one synthetic line.
    for (n, slot) in token_loc.iter_mut().enumerate().take(skip) {
        *slot = Some(Loc { line: 0, last_on_line: n == skip - 1 });
    }

    // Per output line: what goes above it, and what goes at the end of it.
    let mut above: Vec<Vec<(String, bool)>> = vec![Vec::new(); lines.len() + 1];
    let mut at_end: Vec<Option<String>> = vec![None; lines.len() + 1];
    // Whether the source had a blank line before this output line's first token.
    let mut blank_before_line: Vec<bool> = vec![false; lines.len() + 1];

    // What precedes `start` in the source, as an end offset: the previous
    // token or comment, whichever is later.
    let preceding_end = |start: u32| -> u32 {
        let t = in_tokens.partition_point(|t| t.span.end <= start);
        let c = comments.partition_point(|c| c.end <= start);
        let t_end = t.checked_sub(1).map_or(0, |k| in_tokens[k].span.end);
        let c_end = c.checked_sub(1).map_or(0, |k| comments[k].end);
        t_end.max(c_end)
    };
    let blank_between =
        |from: u32, to: u32| text[from as usize..to as usize].matches('\n').count() >= 2;

    // Blank lines before each output line's first token.
    let mut seen_line = vec![false; lines.len() + 1];
    for (n, token) in in_tokens.iter().enumerate() {
        let Some(loc) = token_loc[n] else { continue };
        if seen_line[loc.line] {
            continue;
        }
        seen_line[loc.line] = true;
        blank_before_line[loc.line] =
            blank_between(preceding_end(token.span.start), token.span.start);
    }

    for comment in &comments {
        let body = slice(text, *comment).trim_end().to_owned();
        let prev_token = in_tokens.partition_point(|t| t.span.end <= comment.start).checked_sub(1);
        let next_token = Some(in_tokens.partition_point(|t| t.span.start < comment.end))
            .filter(|q| *q < in_tokens.len());

        // Written at the end of a line: stays at the end of the line its
        // previous token ended up on -- if that token ends an output line.
        if let Some(p) = prev_token {
            let same_line =
                !text[in_tokens[p].span.end as usize..comment.start as usize].contains('\n');
            if same_line {
                if let Some(loc) = token_loc[..=p].iter().rev().find_map(|l| *l) {
                    if loc.last_on_line && at_end[loc.line].is_none() {
                        at_end[loc.line] = Some(body);
                        continue;
                    }
                }
            }
        }

        // Otherwise, on its own line above the line of the next token --
        // a comment in the middle of an expression moves to above the
        // statement, which is the nearest place it can stand.
        let anchor = next_token
            .and_then(|q| token_loc[q..].iter().find_map(|l| *l))
            .map_or(lines.len(), |loc| loc.line);
        above[anchor].push((body, blank_between(preceding_end(comment.start), comment.start)));
    }

    // Assemble.
    let mut out: Vec<String> = Vec::new();
    let push_blank = |out: &mut Vec<String>| {
        if let Some(last) = out.last() {
            if !last.is_empty() && !last.trim_end().ends_with('{') {
                out.push(String::new());
            }
        }
    };
    for (n, line) in lines.iter().enumerate().chain(std::iter::once((lines.len(), &String::new())))
    {
        let closes = n < lines.len() && line.trim_start().starts_with('}');
        let indent_of_line = line.len() - line.trim_start().len();
        let comment_indent = if n == lines.len() {
            0
        } else if closes {
            indent_of_line + INDENT.len()
        } else {
            indent_of_line
        };
        for (text, blank_before) in &above[n] {
            if *blank_before {
                push_blank(&mut out);
            }
            out.push(format!("{}{}", " ".repeat(comment_indent), text));
        }
        if n == lines.len() {
            break;
        }
        if blank_before_line[n] && !closes && !line.is_empty() {
            push_blank(&mut out);
        }
        match &at_end[n] {
            Some(c) => out.push(format!("{line} {c}")),
            None => out.push(line.clone()),
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    let mut formatted = out.join("\n");
    if !formatted.is_empty() {
        formatted.push('\n');
    }

    verify(text, &printed, &comments, &formatted)?;
    Ok(formatted)
}

/// The checks that make the result safe to write over the user's file.
fn verify(
    original: &str,
    printed: &str,
    original_comments: &[Span],
    formatted: &str,
) -> Result<(), FormatError> {
    let reparsed = parse(formatted).map_err(|d| {
        FormatError::Cannot(format!(
            "the formatted text does not parse ({}); the file is left as it is",
            d.message
        ))
    })?;
    if print(&reparsed) != printed {
        return Err(FormatError::Cannot(
            "the formatted text means something other than the original; the file is left as it is"
                .to_owned(),
        ));
    }
    let (tokens, kept) = tokens_of(formatted)?;
    let _ = tokens;
    let before: Vec<&str> =
        original_comments.iter().map(|c| slice(original, *c).trim_end()).collect();
    let after: Vec<&str> = kept.iter().map(|c| slice(formatted, *c).trim_end()).collect();
    if before != after {
        return Err(FormatError::Cannot(
            "formatting would lose, duplicate or reorder a comment; the file is left as it is"
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(text: &str) -> String {
        match format(text) {
            Ok(formatted) => formatted,
            Err(FormatError::Parse(d)) => panic!("did not parse: {}", d.message),
            Err(FormatError::Cannot(why)) => panic!("refused: {why}"),
        }
    }

    fn refusal(text: &str) -> String {
        match format(text) {
            Err(FormatError::Cannot(why)) => why,
            Err(FormatError::Parse(d)) => panic!("did not parse: {}", d.message),
            Ok(formatted) => panic!("formatted:\n{formatted}"),
        }
    }

    #[test]
    fn comments_blank_lines_and_literals_survive() {
        let source = "\
// header

// second
import std.io as console;   // trailing import

// doc for f
fn f(a: int) -> [] int {   // after brace
    // leading
    let x = 'a' + 0x10 + (a);  // trailing x


    let y = 1.50;
    // dangling at end
    return x;
}
// end of file
";
        let expected = "\
// header

// second
import std.io as console; // trailing import

// doc for f
fn f(a: int) -> [] int { // after brace
    // leading
    let x = 'a' + 0x10 + a; // trailing x

    let y = 1.50;
    // dangling at end
    return x;
}
// end of file
";
        assert_eq!(fmt(source), expected);
    }

    #[test]
    fn a_canonical_file_is_returned_unchanged() {
        let source = "fn f() -> [] int {\n    // keep\n    return 0; // me\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn a_comment_inside_an_expression_moves_above_its_statement() {
        let source = "fn f() -> [] int {\n    return g(1, // one\n        2);\n}\n";
        assert_eq!(fmt(source), "fn f() -> [] int {\n    // one\n    return g(1, 2);\n}\n");
    }

    #[test]
    fn else_if_chains_stay_chains_and_braced_ones_become_chains() {
        let chain = "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else if a == 2 {\n        return 2;\n    } else {\n        return 3;\n    }\n}\n";
        assert_eq!(fmt(chain), chain);
        let braced = "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else {\n        if a == 2 {\n            return 2;\n        }\n    }\n    return 3;\n}\n";
        let formatted = fmt(braced);
        assert!(formatted.contains("} else if a == 2 {"), "{formatted}");
        // An `else` block that holds more than the `if` keeps its braces.
        let two = "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else {\n        if a == 2 {\n            return 2;\n        }\n        return 4;\n    }\n}\n";
        assert_eq!(fmt(two), two);
    }

    #[test]
    fn the_edition_marker_is_not_dropped() {
        let source = "edition 2;\n\nfn f() -> [] int {\n    return 0;\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn a_blank_line_is_never_added_after_an_open_brace_or_before_a_close() {
        let source = "fn f() -> [] int {\n\n    let x = 1;\n\n    return x;\n\n}\n";
        assert_eq!(fmt(source), "fn f() -> [] int {\n    let x = 1;\n\n    return x;\n}\n");
    }

    #[test]
    fn pub_on_an_extern_is_the_one_token_the_canonical_form_drops() {
        let source = "pub extern fn getpid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\n";
        assert_eq!(
            fmt(source),
            "extern fn getpid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\n"
        );
    }

    #[test]
    fn an_import_below_a_declaration_is_refused_with_its_line() {
        let why = refusal("fn f() -> [] int {\n    return 0;\n}\n\nimport std.io;\n");
        assert!(why.contains("line 5"), "{why}");
        assert!(why.contains("import"), "{why}");
    }

    #[test]
    fn a_file_that_does_not_parse_is_a_parse_error_not_a_refusal() {
        assert!(matches!(format("fn f( {"), Err(FormatError::Parse(_))));
    }

    #[test]
    fn empty_and_comment_only_files_format() {
        assert_eq!(fmt(""), "");
        assert_eq!(fmt("// only a comment   \n\n\n"), "// only a comment\n");
    }

    #[test]
    fn trailing_whitespace_and_crlf_are_normalised() {
        assert_eq!(
            fmt("fn f() -> [] int {   \r\n    return 0; // c  \r\n}\r\n"),
            "fn f() -> [] int {\n    return 0; // c\n}\n"
        );
    }

    #[test]
    fn a_comment_that_looks_like_code_in_a_string_is_not_a_comment() {
        let source = "fn f() -> [] int {\n    let s = \"// not a comment\";\n    return 0;\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn struct_fields_enum_variants_and_match_arms_keep_their_comments() {
        let source = "\
struct P {
    // the x
    x: int, // x end
    y: int,
}

enum E {
    A, // first
    // second
    B(int),
}

fn f(e: E) -> [] int {
    match e {
        // arm a
        E::A => {
            return 0; // zero
        }
        E::B(n) => {
            return n;
        }
    }
}
";
        assert_eq!(fmt(source), source);
    }
}
