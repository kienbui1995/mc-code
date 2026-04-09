use crate::error::ToolError;

const MAX_BODY_BYTES: usize = 100_000;
const TIMEOUT_SECS: u64 = 30;

fn shared_client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .user_agent("magic-code/0.4")
            .build()
            .expect("failed to build HTTP client")
    })
}

/// Fetch content from a URL, strip HTML tags.
pub struct WebFetchTool;

impl WebFetchTool {
    /// Fetch URL content, strip HTML tags, return plain text (truncated).
    pub async fn execute(url: &str) -> Result<String, ToolError> {
        let client = shared_client();

        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("fetch failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ToolError::ExecutionFailed(format!(
                "HTTP {status} for {url}"
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let text = strip_html(&body);
        if text.len() > MAX_BODY_BYTES {
            let end = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= MAX_BODY_BYTES)
                .last()
                .unwrap_or(0);
            Ok(format!(
                "{}...\n[truncated, {} bytes total]",
                &text[..end],
                text.len()
            ))
        } else {
            Ok(text)
        }
    }
}

/// Search `DuckDuckGo` instant answers.
pub struct WebSearchTool;

impl WebSearchTool {
    /// Search `DuckDuckGo` instant answers (no API key required).
    pub async fn execute(query: &str) -> Result<String, ToolError> {
        let client = shared_client();

        let resp = client
            .get("https://api.duckduckgo.com/")
            .query(&[("q", query), ("format", "json"), ("no_html", "1")])
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("search failed: {e}")))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let mut results = Vec::new();

        // Abstract (main answer)
        if let Some(abs) = body["AbstractText"].as_str().filter(|s| !s.is_empty()) {
            results.push(format!("Answer: {abs}"));
            if let Some(src) = body["AbstractSource"].as_str().filter(|s| !s.is_empty()) {
                results.push(format!("Source: {src}"));
            }
        }

        // Related topics
        if let Some(topics) = body["RelatedTopics"].as_array() {
            for topic in topics.iter().take(5) {
                if let Some(text) = topic["Text"].as_str().filter(|s| !s.is_empty()) {
                    results.push(format!("- {text}"));
                }
            }
        }

        if results.is_empty() {
            Ok(format!(
                "No instant results for \"{query}\". Try web_fetch with a specific URL."
            ))
        } else {
            Ok(results.join("\n"))
        }
    }
}

/// Naive HTML tag stripper — removes tags, decodes common entities, collapses whitespace.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }

    // Decode common entities
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace
    let mut result = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(if c == '\n' { '\n' } else { ' ' });
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_tags() {
        assert_eq!(strip_html("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(strip_html("a &amp; b &lt; c"), "a & b < c");
    }

    #[tokio::test]
    async fn fetch_rejects_bad_url() {
        let err = WebFetchTool::execute("http://localhost:1/nope")
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }
}
