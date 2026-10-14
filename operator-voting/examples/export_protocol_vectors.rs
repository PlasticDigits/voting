use operator_voting::proposal_sections::{prepare_sections, visible_len};
use serde_json::json;
fn main() {
    let mut out = Vec::new();
    for text in [
        "Unicode 🚀 e\u{0301} 한국어",
        "Entities &amp;lt; &AMP; &amp; &lt; &quot; &#x1F680; &#1114112; &#55296;",
        "<a href=\"https://example.com/?a=1&amp;b=2\">HTTPS link</a>",
        "Multiline\ntext\r\nwith tabs\tand apostrophe '",
    ] {
        let html = format!("<p>{text} has enough readable information for a proposal section.</p>");
        let raw = json!({"context":html,"problem":html,"pros_cons":html,"solution":html,"success_criteria":html,"summary":html});
        let p = prepare_sections(&raw).unwrap();
        out.push(json!({"raw":raw,"sections":p.sanitized,"canonical_json":p.canonical_json,"body_hash":p.body_hash,"visible_length":visible_len(p.sanitized["summary"].as_str().unwrap())}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"protocol_version":2,"vectors":out})).unwrap()
    );
}
