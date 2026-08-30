//! Structured proposal template and independent-analysis schema (issues #10 / #11).
//!
//! Canonical `body_hash` is SHA-256 of compact UTF-8 JSON with **sorted keys**.
//! Values are ammonia-sanitized HTML. See [`docs/OPERATOR_VOTING.md`](../../docs/OPERATOR_VOTING.md).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::config::MAX_BODY_BYTES;
use crate::error::{VotingError, VotingResult};
use crate::html::sanitize_proposal_html;
use crate::payload::body_hash;

/// Visible-character floor after strip-tags + whitespace collapse (issue #10).
pub const MIN_SECTION_VISIBLE: usize = 40;
/// TL;DR must stay scannable on the list.
pub const MAX_SUMMARY_VISIBLE: usize = 500;

pub const SECTION_PROBLEM: &str = "problem";
pub const SECTION_CONTEXT: &str = "context";
pub const SECTION_SOLUTION: &str = "solution";
pub const SECTION_PROS_CONS: &str = "pros_cons";
pub const SECTION_SUMMARY: &str = "summary";
pub const SECTION_SUCCESS: &str = "success_criteria";

/// Sorted key order used for canonical JSON (must match the TypeScript helper).
pub const CANONICAL_SECTION_KEYS: [&str; 6] = [
    SECTION_CONTEXT,
    SECTION_PROBLEM,
    SECTION_PROS_CONS,
    SECTION_SOLUTION,
    SECTION_SUCCESS,
    SECTION_SUMMARY,
];

pub const REQUIRED_SECTION_KEYS: [&str; 5] = [
    SECTION_PROBLEM,
    SECTION_SOLUTION,
    SECTION_PROS_CONS,
    SECTION_SUMMARY,
    SECTION_SUCCESS,
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalSections {
    #[serde(default)]
    pub problem: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub solution: String,
    #[serde(default)]
    pub pros_cons: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub success_criteria: String,
}

impl ProposalSections {
    pub fn get(&self, key: &str) -> &str {
        match key {
            SECTION_PROBLEM => &self.problem,
            SECTION_CONTEXT => &self.context,
            SECTION_SOLUTION => &self.solution,
            SECTION_PROS_CONS => &self.pros_cons,
            SECTION_SUMMARY => &self.summary,
            SECTION_SUCCESS => &self.success_criteria,
            _ => "",
        }
    }

    pub fn set_sanitized(&mut self, key: &str, html: String) {
        match key {
            SECTION_PROBLEM => self.problem = html,
            SECTION_CONTEXT => self.context = html,
            SECTION_SOLUTION => self.solution = html,
            SECTION_PROS_CONS => self.pros_cons = html,
            SECTION_SUMMARY => self.summary = html,
            SECTION_SUCCESS => self.success_criteria = html,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisSections {
    #[serde(default)]
    pub what: String,
    #[serde(default)]
    pub benefits: String,
    #[serde(default)]
    pub risks: String,
    #[serde(default)]
    pub short_term: String,
    #[serde(default)]
    pub long_term: String,
}

pub const CANONICAL_ANALYSIS_KEYS: [&str; 5] = ["benefits", "long_term", "risks", "short_term", "what"];

impl AnalysisSections {
    pub fn get(&self, key: &str) -> &str {
        match key {
            "what" => &self.what,
            "benefits" => &self.benefits,
            "risks" => &self.risks,
            "short_term" => &self.short_term,
            "long_term" => &self.long_term,
            _ => "",
        }
    }

    pub fn set_sanitized(&mut self, key: &str, html: String) {
        match key {
            "what" => self.what = html,
            "benefits" => self.benefits = html,
            "risks" => self.risks = html,
            "short_term" => self.short_term = html,
            "long_term" => self.long_term = html,
            _ => {}
        }
    }
}

/// Strip tags / collapse Unicode whitespace / drop zero-width padding.
pub fn visible_text(html: &str) -> String {
    let stripped = ammonia::Builder::new()
        .tags(HashSet::new())
        .clean(html)
        .to_string();
    let mapped: String = stripped
        .chars()
        .filter(|c| !matches!(*c, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}'))
        .map(|c| if c.is_whitespace() || c == '\u{00a0}' { ' ' } else { c })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn visible_len(html: &str) -> usize {
    visible_text(html).chars().count()
}

pub fn sanitize_proposal_sections(raw: &ProposalSections) -> ProposalSections {
    let mut out = ProposalSections::default();
    for key in CANONICAL_SECTION_KEYS {
        out.set_sanitized(key, sanitize_proposal_html(raw.get(key)));
    }
    out
}

pub fn sanitize_analysis_sections(raw: &AnalysisSections) -> AnalysisSections {
    let mut out = AnalysisSections::default();
    for key in CANONICAL_ANALYSIS_KEYS {
        out.set_sanitized(key, sanitize_proposal_html(raw.get(key)));
    }
    out
}

pub fn canonicalize_map(pairs: &[(&str, &str)]) -> String {
    let mut map = BTreeMap::new();
    for (k, v) in pairs {
        map.insert(*k, *v);
    }
    serde_json::to_string(&map).expect("canonical section json")
}

pub fn canonicalize_proposal_sections(sections: &ProposalSections) -> String {
    canonicalize_map(&[
        (SECTION_CONTEXT, sections.get(SECTION_CONTEXT)),
        (SECTION_PROBLEM, sections.get(SECTION_PROBLEM)),
        (SECTION_PROS_CONS, sections.get(SECTION_PROS_CONS)),
        (SECTION_SOLUTION, sections.get(SECTION_SOLUTION)),
        (SECTION_SUCCESS, sections.get(SECTION_SUCCESS)),
        (SECTION_SUMMARY, sections.get(SECTION_SUMMARY)),
    ])
}

pub fn canonicalize_analysis_sections(sections: &AnalysisSections) -> String {
    canonicalize_map(&[
        ("benefits", sections.get("benefits")),
        ("long_term", sections.get("long_term")),
        ("risks", sections.get("risks")),
        ("short_term", sections.get("short_term")),
        ("what", sections.get("what")),
    ])
}

pub fn sections_hash(sections: &ProposalSections) -> String {
    body_hash(&canonicalize_proposal_sections(sections))
}

pub fn analysis_hash(sections: &AnalysisSections) -> String {
    body_hash(&canonicalize_analysis_sections(sections))
}

pub fn combined_section_bytes(sections: &ProposalSections) -> usize {
    CANONICAL_SECTION_KEYS
        .iter()
        .map(|k| sections.get(k).len())
        .sum()
}

pub fn combined_analysis_bytes(sections: &AnalysisSections) -> usize {
    CANONICAL_ANALYSIS_KEYS
        .iter()
        .map(|k| sections.get(k).len())
        .sum()
}

pub fn validate_proposal_sections(sections: &ProposalSections) -> VotingResult<()> {
    if combined_section_bytes(sections) > MAX_BODY_BYTES {
        return Err(VotingError::BadRequest("proposal sections exceed 64 KiB".into()));
    }
    for key in REQUIRED_SECTION_KEYS {
        let n = visible_len(sections.get(key));
        if n < MIN_SECTION_VISIBLE {
            return Err(VotingError::BadRequest(format!(
                "section {key} must contain at least {MIN_SECTION_VISIBLE} visible characters"
            )));
        }
    }
    let summary_n = visible_len(sections.get(SECTION_SUMMARY));
    if summary_n > MAX_SUMMARY_VISIBLE {
        return Err(VotingError::BadRequest(format!(
            "summary must be at most {MAX_SUMMARY_VISIBLE} visible characters"
        )));
    }
    Ok(())
}

pub fn validate_analysis_sections(sections: &AnalysisSections) -> VotingResult<()> {
    if combined_analysis_bytes(sections) > MAX_BODY_BYTES {
        return Err(VotingError::BadRequest("analysis exceeds 64 KiB".into()));
    }
    for key in CANONICAL_ANALYSIS_KEYS {
        if visible_len(sections.get(key)) < MIN_SECTION_VISIBLE {
            return Err(VotingError::BadRequest(format!(
                "analysis {key} must contain at least {MIN_SECTION_VISIBLE} visible characters"
            )));
        }
    }
    Ok(())
}

pub fn render_proposal_html(sections: &ProposalSections) -> String {
    format!(
        "<h2>Summary</h2>{}<h2>The idea or problem to be solved</h2>{}<h2>Supporting context</h2>{}<h2>Proposed solution</h2>{}<h2>Pros and cons</h2>{}<h2>Success criteria</h2>{}",
        sections.summary,
        sections.problem,
        sections.context,
        sections.solution,
        sections.pros_cons,
        sections.success_criteria,
    )
}

pub fn render_analysis_html(sections: &AnalysisSections) -> String {
    format!(
        "<h2>What is proposed</h2>{}<h2>Benefits</h2>{}<h2>Risks</h2>{}<h2>Short-term impact</h2>{}<h2>Long-term impact</h2>{}",
        sections.what, sections.benefits, sections.risks, sections.short_term, sections.long_term,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_enough(label: &str) -> String {
        format!("<p>{} needs forty visible characters in this fixture text.</p>", label)
    }

    fn valid() -> ProposalSections {
        ProposalSections {
            problem: long_enough("problem"),
            context: String::new(),
            solution: long_enough("solution"),
            pros_cons: long_enough("tradeoffs"),
            summary: long_enough("summary"),
            success_criteria: long_enough("success"),
        }
    }

    #[test]
    fn empty_and_nbsp_fail_required() {
        let mut s = valid();
        s.problem = "<p></p>".into();
        assert!(validate_proposal_sections(&s).is_err());
        s.problem = "<p>&nbsp;&nbsp;&nbsp;</p>".into();
        assert!(validate_proposal_sections(&s).is_err());
        s.problem = "<p>   </p>".into();
        assert!(validate_proposal_sections(&s).is_err());
    }

    #[test]
    fn canonical_is_sorted_and_stable() {
        let s = valid();
        let a = canonicalize_proposal_sections(&s);
        let b = canonicalize_proposal_sections(&s);
        assert_eq!(a, b);
        assert!(a.starts_with("{\"context\":\"\""));
        assert!(a.contains("\"problem\":"));
        assert!(a.contains("\"pros_cons\":"));
        assert!(a.contains("\"solution\":"));
        assert!(a.contains("\"success_criteria\":"));
        assert!(a.contains("\"summary\":"));
    }

    #[test]
    fn key_order_does_not_change_hash() {
        let s = valid();
        let h1 = sections_hash(&s);
        let rebuilt = ProposalSections {
            success_criteria: s.success_criteria.clone(),
            summary: s.summary.clone(),
            solution: s.solution.clone(),
            problem: s.problem.clone(),
            context: s.context.clone(),
            pros_cons: s.pros_cons.clone(),
        };
        assert_eq!(h1, sections_hash(&rebuilt));
    }

    #[test]
    fn summary_cap() {
        let mut s = valid();
        s.summary = format!("<p>{}</p>", "s".repeat(MAX_SUMMARY_VISIBLE + 1));
        assert!(validate_proposal_sections(&s).is_err());
        s.summary = format!("<p>{}</p>", "s".repeat(MAX_SUMMARY_VISIBLE));
        assert!(validate_proposal_sections(&s).is_ok());
    }

    #[test]
    fn optional_context_may_be_empty() {
        assert!(validate_proposal_sections(&valid()).is_ok());
    }
}
