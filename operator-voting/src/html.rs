/// Allowlisted WYSIWYG subset. Scripts / event handlers are stripped.
pub fn sanitize_proposal_html(raw: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder
        .tags(
            [
                "p", "br", "strong", "b", "em", "i", "u", "h1", "h2", "h3", "ul", "ol", "li",
                "blockquote", "code", "pre", "a",
            ]
            .into_iter()
            .collect(),
        )
        .link_rel(Some("noopener noreferrer"))
        .url_schemes(["http", "https", "mailto"].into_iter().collect());
    builder.clean(raw).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_and_onerror() {
        let dirty = r#"<p>hi</p><script>alert(1)</script><img src=x onerror="alert(2)">"#;
        let clean = sanitize_proposal_html(dirty);
        assert!(!clean.contains("script"));
        assert!(!clean.contains("onerror"));
        assert!(!clean.contains("alert"));
        assert!(clean.contains("hi"));
    }
}
