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

/// Returns `true` for invisible/zero-width Unicode characters that could
/// bypass search matching or inject directional overrides.
fn is_invisible_unicode(c: char) -> bool {
    matches!(c,
        '\u{200B}'          // zero-width space
        | '\u{200C}'        // zero-width non-joiner
        | '\u{200D}'        // zero-width joiner
        | '\u{FEFF}'        // BOM / zero-width no-break space
        | '\u{200E}'        // LTR mark
        | '\u{200F}'        // RTL mark
        | '\u{202A}'..='\u{202E}' // directional overrides
        | '\u{2066}'..='\u{2069}' // directional isolates
    )
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
                        && *c != '\\'
                        && !is_invisible_unicode(*c)
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
        return "\"__aimemory_empty_query__\"".to_string();
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
        assert_eq!(sanitize_fts5_query("", true), "\"__aimemory_empty_query__\"");
        assert_eq!(sanitize_fts5_query("   ", false), "\"__aimemory_empty_query__\"");
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
        assert_eq!(sanitize_fts5_query("***---|||", true), "\"__aimemory_empty_query__\"");
    }

    // RT-20: Backslash is stripped
    #[test]
    fn strips_backslash() {
        let q = sanitize_fts5_query("test\\injection", true);
        assert!(!q.contains('\\'));
        assert!(q.contains("testinjection"));
    }

    // RT-10: Zero-width unicode chars are stripped
    #[test]
    fn strips_zero_width_chars() {
        let q = sanitize_fts5_query("he\u{200B}llo wo\u{200D}rld", true);
        assert!(q.contains("hello"));
        assert!(q.contains("world"));
        assert!(!q.contains('\u{200B}'));
    }

    // RT-10: Bidi override chars are stripped
    #[test]
    fn strips_bidi_overrides() {
        let q = sanitize_fts5_query("test\u{202A}inject\u{202C}ion", true);
        assert!(q.contains("testinjection"));
    }

    // RT-42: Unique sentinel for empty queries
    #[test]
    fn empty_sentinel_is_unique() {
        let q = sanitize_fts5_query("", true);
        assert!(q.contains("__aimemory_empty_query__"));
    }
}
