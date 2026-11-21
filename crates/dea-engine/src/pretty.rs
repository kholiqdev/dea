//! Re-indents response bodies for the "Pretty" view. Both functions move
//! whitespace only: they never parse values and write them back, so numbers,
//! escapes and key order stay exactly as the server sent them.

use serde::de::IgnoredAny;

const INDENT: &str = "  ";
/// An element holding only text stays on one line up to this many characters.
const INLINE_TEXT_MAX: usize = 80;
const VOID_ELEMENTS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];
/// Elements whose content is copied untouched.
const RAW_ELEMENTS: [&str; 4] = ["pre", "textarea", "script", "style"];

/// Indents JSON with two spaces. `None` when the text is not valid JSON.
pub fn pretty_json(text: &str) -> Option<String> {
    serde_json::from_str::<IgnoredAny>(text).ok()?;
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                while let Some(s) = chars.next() {
                    out.push(s);
                    if s == '\\' {
                        out.extend(chars.next());
                    } else if s == '"' {
                        break;
                    }
                }
            }
            '{' | '[' => {
                out.push(c);
                let close = if c == '{' { '}' } else { ']' };
                // Skip whitespace to see whether the container is empty.
                while chars.next_if(|w| w.is_whitespace()).is_some() {}
                if chars.peek() == Some(&close) {
                    out.push(close);
                    chars.next();
                } else {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(',');
                newline(&mut out, depth);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    Some(out)
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

enum Token<'a> {
    Text(&'a str),
    Open {
        name: &'a str,
        tag: &'a str,
        closed: bool,
    },
    Close {
        name: &'a str,
        tag: &'a str,
    },
    /// Comments, CDATA, declarations and raw elements, copied as they are.
    Verbatim(&'a str),
}

/// Re-indents HTML or XML, one tag per line. Best effort: it follows the tags
/// it sees and never checks that they balance.
pub fn pretty_markup(text: &str, html: bool) -> String {
    let tokens = tokenize(text);
    let mut lines: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < tokens.len() {
        let pad = INDENT.repeat(depth);
        match &tokens[i] {
            Token::Text(t) => lines.push(format!("{pad}{t}")),
            Token::Verbatim(v) => lines.push(format!("{pad}{v}")),
            Token::Close { tag, .. } => {
                depth = depth.saturating_sub(1);
                lines.push(format!("{}{tag}", INDENT.repeat(depth)));
            }
            Token::Open { name, tag, closed } => {
                let void = *closed || (html && is_one_of(name, &VOID_ELEMENTS));
                if void {
                    lines.push(format!("{pad}{tag}"));
                } else if let (
                    Some(Token::Text(t)),
                    Some(Token::Close {
                        name: end,
                        tag: end_tag,
                    }),
                ) = (tokens.get(i + 1), tokens.get(i + 2))
                    && end.eq_ignore_ascii_case(name)
                    && t.chars().count() <= INLINE_TEXT_MAX
                    && !t.contains('\n')
                {
                    lines.push(format!("{pad}{tag}{t}{end_tag}"));
                    i += 2;
                } else {
                    lines.push(format!("{pad}{tag}"));
                    depth += 1;
                }
            }
        }
        i += 1;
    }
    lines.join("\n")
}

fn is_one_of(name: &str, list: &[&str]) -> bool {
    list.iter().any(|n| n.eq_ignore_ascii_case(name))
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if !rest.starts_with('<') {
            let end = rest.find('<').unwrap_or(rest.len());
            push_text(&mut tokens, &rest[..end]);
            rest = &rest[end..];
            continue;
        }
        let skip = |marker: &str| rest.find(marker).map_or(rest.len(), |p| p + marker.len());
        let len = if rest.starts_with("<!--") {
            skip("-->")
        } else if rest.starts_with("<![CDATA[") {
            skip("]]>")
        } else if rest.starts_with("<?") {
            skip("?>")
        } else {
            match tag_end(rest) {
                Some(end) => end,
                None => {
                    // No closing `>`: keep the rest as it is.
                    push_text(&mut tokens, rest);
                    break;
                }
            }
        };
        let tag = &rest[..len];
        rest = &rest[len..];
        match classify(tag) {
            Token::Open {
                name,
                closed: false,
                ..
            } if is_one_of(name, &RAW_ELEMENTS) => {
                // Copy through the matching close tag, content untouched.
                let end = find_close(rest, name).unwrap_or(rest.len());
                tokens.push(Token::Verbatim(tag_with(text, tag, rest, end)));
                rest = &rest[end..];
            }
            token => tokens.push(token),
        }
    }
    tokens
}

/// The slice of `text` from `tag` through the first `end` bytes of `rest`.
fn tag_with<'a>(text: &'a str, tag: &'a str, rest: &'a str, end: usize) -> &'a str {
    let start = tag.as_ptr() as usize - text.as_ptr() as usize;
    let stop = rest.as_ptr() as usize - text.as_ptr() as usize + end;
    &text[start..stop]
}

fn push_text<'a>(tokens: &mut Vec<Token<'a>>, text: &'a str) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        tokens.push(Token::Text(trimmed));
    }
}

/// Byte length of the tag at the start of `s`, skipping `>` inside quotes.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

fn classify(tag: &str) -> Token<'_> {
    if let Some(inner) = tag.strip_prefix("</") {
        let name = inner.trim_end_matches('>').trim();
        return Token::Close { name, tag };
    }
    let inner = tag.trim_start_matches('<');
    if inner.starts_with(['!', '?']) {
        return Token::Verbatim(tag);
    }
    let name_len = inner
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(inner.len());
    Token::Open {
        name: &inner[..name_len],
        tag,
        closed: tag.ends_with("/>"),
    }
}

/// Byte offset just past the next `</name>` in `rest`, case-insensitively.
fn find_close(rest: &str, name: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut from = 0;
    while let Some(p) = rest[from..].find("</") {
        let start = from + p;
        let after = start + 2;
        let tail = bytes.get(after..after + name.len())?;
        if tail.eq_ignore_ascii_case(name.as_bytes()) {
            let end = rest[after + name.len()..].find('>')?;
            return Some(after + name.len() + end + 1);
        }
        from = after;
    }
    None
}
