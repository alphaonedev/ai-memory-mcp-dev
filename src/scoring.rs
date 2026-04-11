// Copyright (c) 2026 AlphaOne LLC. All rights reserved.
// Licensed under the MIT License. See LICENSE file in the project root.

//! Pure Rust scoring functions for memory ranking.
//!
//! Replaces SQL-side `julianday()`, `CASE`, and inline scoring expressions
//! with portable Rust code that any `StorageBackend` can use.

use crate::models::Tier;

/// Tier bonus: permanent memories rank higher.
pub fn tier_bonus(tier: &Tier) -> f64 {
    match tier {
        Tier::Long => 3.0,
        Tier::Mid => 1.0,
        Tier::Short => 0.0,
    }
}

/// Recency decay: `1 / (1 + days_old * 0.1)`.
/// Parses `updated_at` as RFC 3339; falls back to 100-year decay on parse failure
/// so corrupt/unparseable timestamps get minimum recency (near-zero score).
pub fn recency_decay(updated_at: &str) -> f64 {
    let days_old = chrono::DateTime::parse_from_rfc3339(updated_at)
        .map(|dt| {
            let secs = (chrono::Utc::now() - dt.with_timezone(&chrono::Utc))
                .num_seconds()
                .max(0);
            secs as f64 / 86_400.0
        })
        .unwrap_or(36500.0);
    1.0 / (1.0 + days_old * 0.1)
}

/// 6-factor recall score (FTS rank + priority + frequency + confidence + tier + recency).
pub fn recall_score(
    fts_rank: f64,
    priority: i32,
    access_count: i64,
    confidence: f64,
    tier: &Tier,
    updated_at: &str,
) -> f64 {
    (-fts_rank)
        + (priority as f64 * 0.5)
        + (access_count.min(50) as f64 * 0.1)
        + (confidence * 2.0)
        + tier_bonus(tier)
        + recency_decay(updated_at)
}

/// 5-factor search score (like recall but without tier bonus).
pub fn search_score(
    fts_rank: f64,
    priority: i32,
    access_count: i64,
    confidence: f64,
    updated_at: &str,
) -> f64 {
    (-fts_rank)
        + (priority as f64 * 0.5)
        + (access_count.min(50) as f64 * 0.1)
        + (confidence * 2.0)
        + recency_decay(updated_at)
}

/// Adaptive hybrid blend of semantic cosine similarity and normalized FTS score.
///
/// Short content (≤ 500 chars) → 50 % semantic / 50 % FTS.
/// Long  content (≥ 5 000 chars) → 15 % semantic / 85 % FTS.
/// In between: linear interpolation.
pub fn hybrid_blend(cosine: f64, norm_fts: f64, content_len: usize) -> f64 {
    let len = content_len as f64;
    let semantic_weight = if len <= 500.0 {
        0.50
    } else if len >= 5000.0 {
        0.15
    } else {
        0.50 - 0.35 * ((len - 500.0) / 4500.0)
    };
    semantic_weight * cosine + (1.0 - semantic_weight) * norm_fts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_bonus_values() {
        assert!((tier_bonus(&Tier::Long) - 3.0).abs() < f64::EPSILON);
        assert!((tier_bonus(&Tier::Mid) - 1.0).abs() < f64::EPSILON);
        assert!((tier_bonus(&Tier::Short) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn recency_decay_now_is_one() {
        let now = chrono::Utc::now().to_rfc3339();
        let decay = recency_decay(&now);
        // Should be very close to 1.0 (within a few seconds of 'now')
        assert!(decay > 0.99, "decay={decay}");
    }

    #[test]
    fn recency_decay_old_is_small() {
        // 100 days old → 1 / (1 + 10) ≈ 0.0909
        let old = (chrono::Utc::now() - chrono::Duration::days(100)).to_rfc3339();
        let decay = recency_decay(&old);
        assert!((decay - 1.0 / 11.0).abs() < 0.01, "decay={decay}");
    }

    #[test]
    fn recency_decay_bad_input_fallback() {
        let decay = recency_decay("not-a-date");
        // Bad input falls back to 36500 days (100 years) → near-zero score
        // 1 / (1 + 36500 * 0.1) = 1 / 3651 ≈ 0.000274
        assert!(decay < 0.001, "decay={decay} should be near zero for bad input");
    }

    #[test]
    fn recall_score_basic() {
        let now = chrono::Utc::now().to_rfc3339();
        let score = recall_score(-5.0, 8, 20, 0.9, &Tier::Long, &now);
        // fts_rank*-1 = 5.0, priority*0.5 = 4.0, access*0.1 = 2.0,
        // confidence*2 = 1.8, tier = 3.0, recency ≈ 1.0  → ≈ 15.8
        assert!(score > 15.0);
        assert!(score < 17.0);
    }

    #[test]
    fn search_score_no_tier_bonus() {
        let now = chrono::Utc::now().to_rfc3339();
        let r = recall_score(-5.0, 8, 20, 0.9, &Tier::Long, &now);
        let s = search_score(-5.0, 8, 20, 0.9, &now);
        assert!((r - s - 3.0).abs() < 0.02); // difference is tier_bonus(Long) = 3.0
    }

    #[test]
    fn hybrid_blend_short_content() {
        let b = hybrid_blend(0.8, 0.6, 100);
        // 50/50 → 0.5*0.8 + 0.5*0.6 = 0.7
        assert!((b - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn hybrid_blend_long_content() {
        let b = hybrid_blend(0.8, 0.6, 6000);
        // 15/85 → 0.15*0.8 + 0.85*0.6 = 0.12 + 0.51 = 0.63
        assert!((b - 0.63).abs() < f64::EPSILON);
    }

    #[test]
    fn hybrid_blend_mid_content() {
        let b = hybrid_blend(1.0, 0.0, 2750);
        // midpoint: semantic_weight = 0.50 - 0.35 * 0.5 = 0.325
        assert!((b - 0.325).abs() < 0.001);
    }

    #[test]
    fn access_count_capped_at_50() {
        let now = chrono::Utc::now().to_rfc3339();
        let s1 = recall_score(-1.0, 5, 50, 1.0, &Tier::Mid, &now);
        let s2 = recall_score(-1.0, 5, 1000, 1.0, &Tier::Mid, &now);
        assert!((s1 - s2).abs() < f64::EPSILON);
    }

    // RT-11: Bad timestamps get minimum recency (near zero), not maximum
    #[test]
    fn recency_decay_bad_input_gets_minimum() {
        let decay = recency_decay("not-a-date");
        assert!(decay < 0.001, "bad input should get near-zero recency, got {}", decay);
    }

    // RT-11: Very old date gets low recency
    #[test]
    fn recency_decay_ancient_is_near_zero() {
        let ancient = "2000-01-01T00:00:00+00:00";
        let decay = recency_decay(ancient);
        assert!(decay < 0.02, "ancient date should have very low recency, got {}", decay);
    }
}
