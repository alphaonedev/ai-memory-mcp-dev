// Copyright (c) 2026 AlphaOne LLC. All rights reserved.
// Licensed under the MIT License. See LICENSE file in the project root.

//! Full-text search abstraction layer.
//!
//! `TextSearch` is a trait that any backend can implement.  The default
//! implementation, `SqliteFts5`, wraps the FTS5 query sanitization logic
//! previously embedded in `db.rs`.

/// Pluggable full-text search interface.
///
/// Backends implement this to provide query sanitization and any
/// engine-specific query translation.
pub trait TextSearch: Send + Sync {
    /// Sanitize user input into a safe full-text query string.
    ///
    /// * `use_or` — `true` for fuzzy/recall (OR semantics),
    ///              `false` for precise/search (AND semantics).
    fn sanitize_query(&self, input: &str, use_or: bool) -> String;
}

/// SQLite FTS5 implementation of `TextSearch`.
#[derive(Debug, Clone, Copy)]
pub struct SqliteFts5;

impl TextSearch for SqliteFts5 {
    fn sanitize_query(&self, input: &str, use_or: bool) -> String {
        sanitize_fts5_query(input, use_or)
    }
}

/// Core FTS5 sanitization: strips special characters, filters boolean
/// operators, wraps tokens in quotes, and joins with OR or AND.
pub fn sanitize_fts5_query(input: &str, use_or: bool) -> String {
    let joiner = if use_or { " OR " } else { " " };
    let tokens: Vec<String> = input
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .filter(|t| {
            let upper = t.to_uppercase();
            upper != "AND" && upper != "OR" && upper != "NOT" && upper != "NEAR"
        })
        .map(|token| {
            let clean: String = token
                .chars()
                .filter(|c| {
                    *c != '"'
                        && *c != '*'
                        && *c != '^'
                        && *c != '{'
                        && *c != '}'
                        && *c != '('
                        && *c != ')'
                        && *c != ':'
                        && *c != '-'
                        && *c != '|'
                })
                .collect();
            if clean.is_empty() {
                return String::new();
            }
            format!("\"{clean}\"")
        })
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return "\"_empty_\"".to_string();
    }
    tokens.join(joiner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_special_chars() {
        let q = sanitize_fts5_query("test* \"injection\" (drop)", true);
        assert!(!q.contains('*'));
        assert!(!q.contains('('));
        assert!(!q.contains(')'));
        assert!(q.contains("test"));
        assert!(q.contains("injection"));
        assert!(q.contains("drop"));
    }

    #[test]
    fn filters_boolean_operators() {
        let q = sanitize_fts5_query("hello AND world OR NOT NEAR test", true);
        assert!(q.contains("hello"));
        assert!(q.contains("world"));
        assert!(q.contains("test"));
        // Operators should not appear as standalone tokens
        assert!(!q.contains(" AND "));
        assert!(!q.contains(" NOT "));
        assert!(!q.contains(" NEAR "));
    }

    #[test]
    fn empty_returns_placeholder() {
        assert_eq!(sanitize_fts5_query("", true), "\"_empty_\"");
        assert_eq!(sanitize_fts5_query("   ", false), "\"_empty_\"");
    }

    #[test]
    fn or_join() {
        let q = sanitize_fts5_query("rust python", true);
        assert_eq!(q, "\"rust\" OR \"python\"");
    }

    #[test]
    fn and_join() {
        let q = sanitize_fts5_query("rust python", false);
        assert_eq!(q, "\"rust\" \"python\"");
    }

    #[test]
    fn trait_dispatch() {
        let fts = SqliteFts5;
        let q = fts.sanitize_query("hello world", true);
        assert_eq!(q, "\"hello\" OR \"world\"");
    }

    #[test]
    fn all_special_chars_yields_placeholder() {
        assert_eq!(sanitize_fts5_query("***---|||", true), "\"_empty_\"");
    }
}
