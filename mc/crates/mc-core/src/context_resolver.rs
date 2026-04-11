use std::path::{Path, PathBuf};

/// Resolvedcontext.
pub struct ResolvedContext {
    pub path: String,
    pub content: String,
    pub token_estimate: usize,
}

/// Contextresolver.
pub struct ContextResolver {
    workspace_root: PathBuf,
}

impl ContextResolver {
    #[must_use]
    /// New.
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Parse `@path` mentions from input, resolve to file contents.
    /// Returns `(cleaned_input, resolved_contexts)`.
    #[must_use]
    /// Resolve.
    pub fn resolve(&self, input: &str) -> (String, Vec<ResolvedContext>) {
        let mut contexts = Vec::new();
        let mut cleaned = String::with_capacity(input.len());
        let mut chars = input.char_indices().peekable();

        while let Some((i, ch)) = chars.next() {
            if ch == '@' {
                // Extract path after @
                let start = i + 1;
                let mut end = start;
                while let Some(&(j, c)) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    end = j + c.len_utf8();
                    chars.next();
                }
                let path_str = &input[start..end];
                if let Some(ctx) = self.resolve_path(path_str) {
                    contexts.push(ctx);
                } else {
                    // Not a valid file, keep original text
                    cleaned.push('@');
                    cleaned.push_str(path_str);
                }
            } else {
                cleaned.push(ch);
            }
        }

        (cleaned.trim().to_string(), contexts)
    }

    /// Build the augmented user message with file contents prepended.
    #[must_use]
    /// Build message.
    pub fn build_message(input: &str, contexts: &[ResolvedContext]) -> String {
        if contexts.is_empty() {
            return input.to_string();
        }
        let mut parts: Vec<String> = contexts
            .iter()
            .map(|ctx| format!("[File: {}]\n```\n{}\n```", ctx.path, ctx.content))
            .collect();
        parts.push(input.to_string());
        parts.join("\n\n")
    }

    fn resolve_path(&self, path_str: &str) -> Option<ResolvedContext> {
        if path_str.is_empty() {
            return None;
        }
        let full = if Path::new(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            self.workspace_root.join(path_str)
        };
        // Security: ensure resolved path stays within workspace
        let canonical = full.canonicalize().ok()?;
        let workspace_canonical = self.workspace_root.canonicalize().ok()?;
        if !canonical.starts_with(&workspace_canonical) {
            return None;
        }

        let content = std::fs::read_to_string(&canonical).ok()?;
        let token_estimate = content.len().div_ceil(4);
        // Truncate if too large (>40K chars ≈ 10K tokens)
        let content = if content.len() > 40_000 {
            format!(
                "{}...\n[truncated, {} total chars]",
                &content[..content
                    .char_indices()
                    .take_while(|&(i, _)| i < 40_000)
                    .last()
                    .map_or(0, |(i, _)| i)],
                content.len()
            )
        } else {
            content
        };
        Some(ResolvedContext {
            path: path_str.to_string(),
            content,
            token_estimate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_at_mention() {
        let dir = std::env::temp_dir().join(format!("mc-ctx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.rs"), "fn main() {}").unwrap();

        let resolver = ContextResolver::new(dir.clone());
        let (cleaned, contexts) = resolver.resolve("@test.rs fix the bug");
        assert_eq!(cleaned, "fix the bug");
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].path, "test.rs");
        assert!(contexts[0].content.contains("fn main"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn nonexistent_file_kept_as_text() {
        let resolver = ContextResolver::new(std::env::temp_dir());
        let (cleaned, contexts) = resolver.resolve("@nonexistent.xyz hello");
        assert!(cleaned.contains("@nonexistent.xyz"));
        assert!(contexts.is_empty());
    }

    #[test]
    fn multiple_mentions() {
        let dir = std::env::temp_dir().join(format!("mc-ctx2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "aaa").unwrap();
        std::fs::write(dir.join("b.rs"), "bbb").unwrap();

        let resolver = ContextResolver::new(dir.clone());
        let (_, contexts) = resolver.resolve("@a.rs @b.rs compare");
        assert_eq!(contexts.len(), 2);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn build_message_with_contexts() {
        let contexts = vec![ResolvedContext {
            path: "src/main.rs".into(),
            content: "fn main() {}".into(),
            token_estimate: 3,
        }];
        let msg = ContextResolver::build_message("fix this", &contexts);
        assert!(msg.contains("[File: src/main.rs]"));
        assert!(msg.contains("fn main"));
        assert!(msg.ends_with("fix this"));
    }

    #[test]
    fn no_mentions() {
        let resolver = ContextResolver::new(std::env::temp_dir());
        let (cleaned, contexts) = resolver.resolve("just a normal prompt");
        assert_eq!(cleaned, "just a normal prompt");
        assert!(contexts.is_empty());
    }
}

    #[test]
    fn build_message_formats_correctly() {
        let ctx = vec![ResolvedContext {
            path: "test.rs".into(),
            content: "fn main() {}".into(),
            token_estimate: 5,
        }];
        let msg = ContextResolver::build_message("do something", &ctx);
        assert!(msg.contains("do something"));
        assert!(msg.contains("test.rs"));
        assert!(msg.contains("fn main()"));
    }
