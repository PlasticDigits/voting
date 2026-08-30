//! Structured proposal body (issue #10).
//!
//! Invariants: [`docs/OPERATOR_VOTING.md`](../../docs/OPERATOR_VOTING.md) (OV-S1–S8).
//!
//! `body_hash` for `purpose=propose` is SHA-256 (hex) of the **canonical JSON**
//! of ammonia-sanitized section HTML. Key order is sorted ASCII. Values are
//! JSON strings with no insignificant whitespace in the object encoding.

use serde_json::{Map, Value};

use crate::config::MAX_BODY_BYTES;
use crate::error::{VotingError, VotingResult};
use crate::html::sanitize_proposal_html;
use crate::payload::body_hash;

/// Alphabetical key order — the hash serializer. Do not reorder.
pub const SECTION_KEYS_SORTED: [&str; 6] = [
    "context",
    "problem",
    "pros_cons",
    "solution",
    "success_criteria",
    "summary",
];

/// Voter-facing render order. Summary (TL;DR) first.
pub const SECTION_DISPLAY_ORDER: [&str; 6] = [
    "summary",
    "problem",
    "context",
    "solution",
    "pros_cons",
    "success_criteria",
];

pub const REQUIRED_SECTION_KEYS: [&str; 5] = [
    "problem",
    "solution",
    "pros_cons",
    "summary",
    "success_criteria",
];

pub const MIN_SECTION_VISIBLE_CHARS: usize = 40;
pub const MAX_SUMMARY_VISIBLE_CHARS: usize = 500;

pub const SECTION_LABELS: &[(&str, &str)] = &[
    ("summary", "Summary (TL;DR)"),
    ("problem", "The idea or problem to be solved"),
    (
        "context",
        "Supporting context, real-world issues, and alternatives",
    ),
    ("solution", "Proposed solution and supporting evidence"),
    ("pros_cons", "Pros and cons / trade-offs"),
    ("success_criteria", "Success criteria / goalposts"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSections {
    pub sanitized: Map<String, Value>,
    pub canonical_json: String,
    pub body_hash: String,
    pub body_html: String,
}

pub fn section_label(key: &str) -> &str {
    SECTION_LABELS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| *label)
        .unwrap_or(key)
}

/// Strip tags, decode a small entity set, drop zero-width, collapse Unicode whitespace.
pub fn visible_text(html: &str) -> String {
    let stripped = strip_tags(html);
    let decoded = decode_entities(&stripped);
    let mut buf = String::with_capacity(decoded.len());
    let mut prev_space = true;
    for c in decoded.chars() {
        if is_zero_width(c) {
            continue;
        }
        if c.is_whitespace() {
            if !prev_space {
                buf.push(' ');
                prev_space = true;
            }
            continue;
        }
        buf.push(c);
        prev_space = false;
    }
    if buf.ends_with(' ') {
        buf.pop();
    }
    buf
}

pub fn visible_len(html: &str) -> usize {
    visible_text(html).chars().count()
}

/// Sanitize, empty-normalize, enforce minima, emit canonical JSON + concatenated HTML.
pub fn prepare_sections(raw: &Value) -> VotingResult<PreparedSections> {
    let object = raw
        .as_object()
        .ok_or_else(|| VotingError::BadRequest("body_sections must be an object".into()))?;
    for key in object.keys() {
        if !SECTION_KEYS_SORTED.contains(&key.as_str()) {
            return Err(VotingError::BadRequest(format!(
                "unknown proposal section {key}"
            )));
        }
    }

    let mut sanitized = Map::new();
    let mut combined = 0usize;
    for key in SECTION_KEYS_SORTED {
        let submitted = match object.get(key) {
            None if key == "context" => "",
            None => {
                return Err(VotingError::BadRequest(format!(
                    "missing required section {key}"
                )));
            }
            Some(Value::String(s)) => s.as_str(),
            Some(_) => {
                return Err(VotingError::BadRequest(format!(
                    "section {key} must be a string"
                )));
            }
        };
        combined = combined.saturating_add(submitted.len());
        if combined > MAX_BODY_BYTES {
            return Err(VotingError::BadRequest("proposal too large".into()));
        }
        let mut clean = sanitize_proposal_html(submitted);
        if visible_len(&clean) == 0 {
            clean.clear();
        }
        combined = combined.saturating_add(clean.len());
        if combined > MAX_BODY_BYTES || clean.len() > MAX_BODY_BYTES {
            return Err(VotingError::BadRequest("proposal too large".into()));
        }
        sanitized.insert(key.to_string(), Value::String(clean));
    }

    for key in REQUIRED_SECTION_KEYS {
        let html = sanitized[key].as_str().unwrap_or("");
        let n = visible_len(html);
        if n < MIN_SECTION_VISIBLE_CHARS {
            return Err(VotingError::BadRequest(format!(
                "section {key} needs at least {MIN_SECTION_VISIBLE_CHARS} characters"
            )));
        }
        if key == "summary" && n > MAX_SUMMARY_VISIBLE_CHARS {
            return Err(VotingError::BadRequest(format!(
                "summary must be at most {MAX_SUMMARY_VISIBLE_CHARS} characters"
            )));
        }
    }

    let canonical_json = serde_json::to_string(&Value::Object(sanitized.clone()))
        .map_err(|e| VotingError::BadRequest(format!("canonicalize sections: {e}")))?;
    if canonical_json.len() > MAX_BODY_BYTES {
        return Err(VotingError::BadRequest("proposal too large".into()));
    }
    let hash = body_hash(&canonical_json);
    let body_html = render_body_html(&sanitized);
    Ok(PreparedSections {
        sanitized,
        canonical_json,
        body_hash: hash,
        body_html,
    })
}

pub fn summary_html(sections: &Map<String, Value>) -> Option<String> {
    sections
        .get("summary")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn render_body_html(sections: &Map<String, Value>) -> String {
    let mut out = String::new();
    for key in SECTION_DISPLAY_ORDER {
        let html = sections.get(key).and_then(Value::as_str).unwrap_or("");
        if html.is_empty() {
            continue;
        }
        out.push_str("<section><h2>");
        out.push_str(section_label(key));
        out.push_str("</h2>");
        out.push_str(html);
        out.push_str("</section>");
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn is_zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
    )
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('&') {
            if let Some((ch, consumed)) = parse_entity(stripped) {
                out.push(ch);
                rest = &stripped[consumed..];
                continue;
            }
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// `s` is the slice after `&`. Returns (char, bytes consumed after `&`).
fn parse_entity(s: &str) -> Option<(char, usize)> {
    if s.starts_with('#') {
        let hex = s.starts_with("#x") || s.starts_with("#X");
        let digits_start = if hex { 2 } else { 1 };
        let rest = s.get(digits_start..)?;
        let end = rest.find(';')?;
        let num_str = &rest[..end];
        if num_str.is_empty() {
            return None;
        }
        let num = if hex {
            u32::from_str_radix(num_str, 16).ok()?
        } else {
            num_str.parse().ok()?
        };
        let ch = char::from_u32(num)?;
        return Some((ch, digits_start + end + 1));
    }
    const NAMED: &[(&str, char)] = &[
        ("nbsp;", '\u{00A0}'),
        ("amp;", '&'),
        ("lt;", '<'),
        ("gt;", '>'),
        ("quot;", '"'),
        ("apos;", '\''),
    ];
    for (name, ch) in NAMED {
        if s.starts_with(name) {
            return Some((*ch, name.len()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fill(ch: char) -> String {
        format!(
            "<p>{}</p>",
            ch.to_string().repeat(MIN_SECTION_VISIBLE_CHARS)
        )
    }

    fn valid_raw() -> Value {
        json!({
            "problem": fill('p'),
            "context": "",
            "solution": fill('s'),
            "pros_cons": fill('t'),
            "summary": fill('m'),
            "success_criteria": fill('c'),
        })
    }

    #[test]
    fn visible_len_strips_empty_html_and_nbsp() {
        assert_eq!(visible_len(""), 0);
        assert_eq!(visible_len("<p></p>"), 0);
        assert_eq!(visible_len("<p><br></p>"), 0);
        assert_eq!(visible_len("<p> </p>"), 0);
        assert_eq!(visible_len("&nbsp;"), 0);
        assert_eq!(visible_len("<p>&nbsp;</p>"), 0);
        assert_eq!(visible_len("<p>&#160;</p>"), 0);
        assert_eq!(visible_len("<p>&#xA0;</p>"), 0);
        assert_eq!(visible_len("<p>\u{200B}\u{200B}</p>"), 0);
        assert_eq!(visible_len("<p>hello</p>"), 5);
        assert_eq!(visible_len("<p>  hello   world  </p>"), 11);
    }

    #[test]
    fn empty_required_rejected() {
        for empty in ["", "   ", "<p></p>", "<p><br></p>", "<p>&nbsp;</p>"] {
            let mut raw = valid_raw();
            raw["pros_cons"] = json!(empty);
            let err = prepare_sections(&raw).unwrap_err();
            let msg = err.to_string();
            assert!(
                matches!(err, VotingError::BadRequest(_)) && msg.contains("pros_cons"),
                "{empty:?} -> {msg}"
            );
        }
    }

    #[test]
    fn missing_required_rejected() {
        let mut raw = valid_raw();
        raw.as_object_mut().unwrap().remove("pros_cons");
        assert!(matches!(
            prepare_sections(&raw),
            Err(VotingError::BadRequest(_))
        ));
    }

    #[test]
    fn optional_context_may_be_empty() {
        let prepared = prepare_sections(&valid_raw()).unwrap();
        assert_eq!(prepared.sanitized["context"], json!(""));
    }

    #[test]
    fn summary_max_enforced() {
        let mut raw = valid_raw();
        raw["summary"] = json!(format!(
            "<p>{}</p>",
            "s".repeat(MAX_SUMMARY_VISIBLE_CHARS + 1)
        ));
        let err = prepare_sections(&raw).unwrap_err();
        assert!(matches!(err, VotingError::BadRequest(_)) && err.to_string().contains("summary"));
        raw["summary"] = json!(format!("<p>{}</p>", "s".repeat(MAX_SUMMARY_VISIBLE_CHARS)));
        assert!(prepare_sections(&raw).is_ok());
    }

    #[test]
    fn unknown_key_rejected() {
        let mut raw = valid_raw();
        raw["independent_analysis"] = json!("<p>nope</p>");
        let err = prepare_sections(&raw).unwrap_err();
        assert!(matches!(err, VotingError::BadRequest(_)) && err.to_string().contains("unknown"));
    }

    #[test]
    fn key_order_does_not_change_hash() {
        let a = prepare_sections(&valid_raw()).unwrap();
        let shuffled = json!({
            "summary": fill('m'),
            "success_criteria": fill('c'),
            "pros_cons": fill('t'),
            "solution": fill('s'),
            "problem": fill('p'),
            "context": "",
        });
        let b = prepare_sections(&shuffled).unwrap();
        assert_eq!(a.canonical_json, b.canonical_json);
        assert_eq!(a.body_hash, b.body_hash);
        for (i, key) in SECTION_KEYS_SORTED.iter().enumerate() {
            if i + 1 < SECTION_KEYS_SORTED.len() {
                let here = a.canonical_json.find(&format!("\"{key}\"")).unwrap();
                let next = SECTION_KEYS_SORTED[i + 1];
                let there = a.canonical_json.find(&format!("\"{next}\"")).unwrap();
                assert!(here < there, "{key} should precede {next}");
            }
        }
    }

    #[test]
    fn letter_change_changes_hash() {
        let a = prepare_sections(&valid_raw()).unwrap();
        let mut raw = valid_raw();
        raw["problem"] = json!(fill('P'));
        let b = prepare_sections(&raw).unwrap();
        assert_ne!(a.body_hash, b.body_hash);
    }

    #[test]
    fn xss_stripped_before_hash() {
        let mut raw = valid_raw();
        let dirty = format!(
            "<p>{}</p><script>alert(1)</script><img src=x onerror=\"alert(2)\">",
            "x".repeat(MIN_SECTION_VISIBLE_CHARS)
        );
        raw["problem"] = json!(dirty);
        let prepared = prepare_sections(&raw).unwrap();
        let problem = prepared.sanitized["problem"].as_str().unwrap();
        assert!(!problem.contains("script"));
        assert!(!problem.contains("onerror"));
        assert!(!problem.contains("alert"));
        let dirty_json = {
            let mut m = Map::new();
            for key in SECTION_KEYS_SORTED {
                m.insert(key.to_string(), raw[key].clone());
            }
            serde_json::to_string(&Value::Object(m)).unwrap()
        };
        assert_ne!(body_hash(&dirty_json), prepared.body_hash);
    }

    #[test]
    fn combined_size_cap() {
        let huge = format!("<p>{}</p>", "a".repeat(MAX_BODY_BYTES));
        let mut raw = valid_raw();
        raw["problem"] = json!(huge);
        let err = prepare_sections(&raw).unwrap_err();
        assert!(matches!(err, VotingError::BadRequest(_)) && err.to_string().contains("too large"));
    }

    #[test]
    fn golden_canonical_json_and_hash() {
        let raw = json!({
            "problem": "<p>Holders often vote without enough information about impact and risks.</p>",
            "context": "",
            "solution": "<p>Require labeled sections with server-enforced minimums.</p>",
            "pros_cons": "<p>Pros: proposals are comparable. Cons: writing takes more care.</p>",
            "summary": "<p>Standard sections so every proposal is scannable before a vote.</p>",
            "success_criteria": "<p>Voters can scan TL;DR, trade-offs, and goalposts before they sign.</p>",
        });
        let prepared = prepare_sections(&raw).unwrap();
        let expected = "{\"context\":\"\",\"problem\":\"<p>Holders often vote without enough information about impact and risks.</p>\",\"pros_cons\":\"<p>Pros: proposals are comparable. Cons: writing takes more care.</p>\",\"solution\":\"<p>Require labeled sections with server-enforced minimums.</p>\",\"success_criteria\":\"<p>Voters can scan TL;DR, trade-offs, and goalposts before they sign.</p>\",\"summary\":\"<p>Standard sections so every proposal is scannable before a vote.</p>\"}";
        assert_eq!(prepared.canonical_json, expected);
        assert_eq!(
            prepared.body_hash,
            "85b0e3985805be8d192178665cf0b745ddcb14cb5ccfc37d2b49c137fe0f5ed1"
        );
        assert_eq!(prepared.body_hash, body_hash(expected));
        assert!(prepared.body_html.contains("Summary (TL;DR)"));
        assert!(prepared.body_html.contains("Success criteria / goalposts"));
        assert!(!prepared.body_html.contains("Supporting context"));
    }
}
