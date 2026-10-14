//! Paired binary comparisons with observation-weighted family bootstrapping.

use std::collections::HashMap;

/// One paired observation: the same task and replicate under arms X and Y.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedOutcome {
    pub family: String,
    pub task: String,
    pub replicate: u32,
    pub x_pass: bool,
    pub y_pass: bool,
}

/// Paired rates, a 95% family-bootstrap lift interval, and discordance statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedSummary {
    pub n: usize,
    pub families: usize,
    pub x_rate: f64,
    pub y_rate: f64,
    pub lift: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub discordant_x_only: usize,
    pub discordant_y_only: usize,
    pub mcnemar_exact_p: f64,
}

/// Evidence relative to a caller-specified minimum effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultClass {
    Supported,
    RuledOut,
    Inconclusive,
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}

/// Draw `iterations` whole-family resamples, deterministically given `seed`.
/// Families are indexed in first-observation order. Every task and replicate
/// contributes equally; selecting a family again duplicates all its observations.
/// Empty input returns zero rates and CI, with p = 1. With zero iterations,
/// the CI collapses to the point estimate (no bootstrap uncertainty estimated).
pub fn paired_summary(obs: &[PairedOutcome], iterations: usize, seed: u64) -> PairedSummary {
    let mut family_ids = HashMap::new();
    // Sufficient statistics preserve whole-family, observation-weighted draws.
    let mut clusters: Vec<(usize, usize, usize)> = Vec::new();
    let (mut x, mut y, mut b, mut c) = (0usize, 0usize, 0usize, 0usize);
    for observation in obs {
        let next = clusters.len();
        let id = *family_ids
            .entry(observation.family.as_str())
            .or_insert(next);
        if id == next {
            clusters.push((0, 0, 0));
        }
        let xp = usize::from(observation.x_pass);
        let yp = usize::from(observation.y_pass);
        clusters[id].0 += 1;
        clusters[id].1 += xp;
        clusters[id].2 += yp;
        x += xp;
        y += yp;
        b += usize::from(observation.x_pass && !observation.y_pass);
        c += usize::from(observation.y_pass && !observation.x_pass);
    }
    let n = obs.len();
    let families = clusters.len();
    let x_rate = if n == 0 { 0.0 } else { x as f64 / n as f64 };
    let y_rate = if n == 0 { 0.0 } else { y as f64 / n as f64 };
    let lift = x_rate - y_rate;
    let (mut ci_low, mut ci_high) = (lift, lift);
    // A single family always reproduces the original sample, so its CI collapses.
    if families > 1 && iterations > 0 {
        let mut rng = SplitMix64(seed);
        let mut samples = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let (mut size, mut sx, mut sy) = (0usize, 0usize, 0usize);
            for _ in 0..families {
                let (count, cx, cy) = clusters[(rng.next_u64() % families as u64) as usize];
                size += count;
                sx += cx;
                sy += cy;
            }
            samples.push(sx as f64 / size as f64 - sy as f64 / size as f64);
        }
        samples.sort_by(f64::total_cmp);
        let last = (iterations - 1) as f64;
        ci_low = samples[(0.025 * last).floor() as usize];
        ci_high = samples[(0.975 * last).ceil() as usize];
    }
    PairedSummary {
        n,
        families,
        x_rate,
        y_rate,
        lift,
        ci_low,
        ci_high,
        discordant_x_only: b,
        discordant_y_only: c,
        mcnemar_exact_p: mcnemar_exact(b, c),
    }
}

/// Two-sided sign-flip p-value using whole-family summed differences.
/// Enumerates all assignments for <= 16 families; otherwise uses SplitMix64
/// draws plus the observed assignment. Empty or all-zero sums return 1.
pub fn family_sign_flip_p(obs: &[PairedOutcome], iterations: usize, seed: u64) -> f64 {
    let mut family_ids = HashMap::new();
    let mut sums: Vec<i64> = Vec::new();
    for observation in obs {
        let next = sums.len();
        let id = *family_ids
            .entry(observation.family.as_str())
            .or_insert(next);
        if id == next {
            sums.push(0);
        }
        sums[id] += i64::from(observation.x_pass) - i64::from(observation.y_pass);
    }
    let observed = sums.iter().sum::<i64>().abs();
    if observed == 0 {
        return 1.0;
    }
    if sums.len() <= 16 {
        let assignments = 1usize << sums.len();
        let mut extreme = 0usize;
        for mask in 0..assignments {
            let statistic: i64 = sums
                .iter()
                .enumerate()
                .map(|(id, &sum)| if mask & (1 << id) == 0 { sum } else { -sum })
                .sum();
            extreme += usize::from(statistic.abs() >= observed);
        }
        extreme as f64 / assignments as f64
    } else {
        let mut rng = SplitMix64(seed);
        let mut extreme = 0usize;
        for _ in 0..iterations {
            let statistic: i64 = sums
                .iter()
                .map(|&sum| if rng.next_u64() & 1 == 0 { sum } else { -sum })
                .sum();
            extreme += usize::from(statistic.abs() >= observed);
        }
        (1.0 + extreme as f64) / (1.0 + iterations as f64)
    }
}

/// Inconclusive below the family floor or without discordance; otherwise classify.
pub fn classify_guarded(
    summary: &PairedSummary,
    min_effect: f64,
    min_families: usize,
) -> ResultClass {
    if summary.families < min_families
        || (summary.discordant_x_only == 0 && summary.discordant_y_only == 0)
    {
        ResultClass::Inconclusive
    } else {
        classify(summary, min_effect)
    }
}

/// Supported if `ci_low > 0` and `lift >= min_effect`; ruled out if
/// `ci_high < min_effect`; otherwise inconclusive. Supported takes precedence.
pub fn classify(summary: &PairedSummary, min_effect: f64) -> ResultClass {
    if summary.ci_low > 0.0 && summary.lift >= min_effect {
        ResultClass::Supported
    } else if summary.ci_high < min_effect {
        ResultClass::RuledOut
    } else {
        ResultClass::Inconclusive
    }
}

/// Exact two-sided McNemar p-value: min(1, 2 P(Bin(b+c, 0.5) <= min(b,c))).
/// No discordance returns 1. Log-space recurrence avoids intermediate
/// underflow, including for totals up to 2000; very small final p may be zero.
pub fn mcnemar_exact(b: usize, c: usize) -> f64 {
    if b == c {
        return 1.0;
    }
    let n = b as f64 + c as f64;
    let mut log_term = -n * std::f64::consts::LN_2;
    let mut log_tail = log_term;
    for k in 1..=b.min(c) {
        log_term += (n - k as f64 + 1.0).ln() - (k as f64).ln();
        let larger = log_tail.max(log_term);
        log_tail = larger + (-(log_tail - log_term).abs()).exp().ln_1p();
    }
    (log_tail + std::f64::consts::LN_2).exp().min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(family: usize, task: usize, x_pass: bool, y_pass: bool) -> PairedOutcome {
        PairedOutcome {
            family: family.to_string(),
            task: task.to_string(),
            replicate: task as u32,
            x_pass,
            y_pass,
        }
    }

    #[test]
    fn exact_mcnemar() {
        assert_eq!(mcnemar_exact(0, 0), 1.0);
        assert!((mcnemar_exact(0, 5) - 0.0625).abs() < 1e-12);
        assert!((mcnemar_exact(1, 9) - 0.021484375).abs() < 1e-12);
        assert_eq!(mcnemar_exact(5, 5), 1.0);
        for (b, c) in [(0, 5), (1, 9), (900, 1100)] {
            assert_eq!(mcnemar_exact(b, c), mcnemar_exact(c, b));
        }
        let p = mcnemar_exact(900, 1100);
        assert!(p.is_finite() && p > 0.0 && p < 1.0);
        assert!((mcnemar_exact(999, 1001) - 0.9821609888541463).abs() < 1e-10);
    }

    #[test]
    fn family_sign_flip_exact() {
        let strong: Vec<_> = (0..10)
            .flat_map(|f| (0..3).map(move |t| outcome(f, t, true, false)))
            .collect();
        assert_eq!(family_sign_flip_p(&strong, 0, 42), 2.0 / 1024.0);
        let symmetric: Vec<_> = (0..10).map(|f| outcome(f, 0, f < 5, f >= 5)).collect();
        assert_eq!(family_sign_flip_p(&symmetric, 1000, 42), 1.0);
        assert_eq!(family_sign_flip_p(&[], 1000, 42), 1.0);
        let cancelling = [outcome(0, 0, true, false), outcome(0, 1, false, true)];
        assert_eq!(family_sign_flip_p(&cancelling, 1000, 42), 1.0);
    }

    #[test]
    fn family_sign_flip_monte_carlo() {
        let strong: Vec<_> = (0..20).map(|f| outcome(f, 0, true, false)).collect();
        let p = family_sign_flip_p(&strong, 10_000, 42);
        assert!(p > 0.0 && p < 0.01);
        assert_eq!(p, family_sign_flip_p(&strong, 10_000, 42));
        assert_eq!(family_sign_flip_p(&strong, 0, 42), 1.0);
        let symmetric: Vec<_> = (0..20).map(|f| outcome(f, 0, f < 10, f >= 10)).collect();
        assert_eq!(family_sign_flip_p(&symmetric, 1000, 42), 1.0);
    }

    #[test]
    fn guarded_classification() {
        let single = paired_summary(&[outcome(0, 0, true, false)], 100, 42);
        assert_eq!(classify(&single, 0.1), ResultClass::Supported);
        assert_eq!(
            classify_guarded(&single, 0.1, 10),
            ResultClass::Inconclusive
        );
        let saturated: Vec<_> = (0..12).map(|f| outcome(f, 0, true, true)).collect();
        let summary = paired_summary(&saturated, 100, 42);
        assert_eq!(classify(&summary, 0.1), ResultClass::RuledOut);
        assert_eq!(
            classify_guarded(&summary, 0.1, 10),
            ResultClass::Inconclusive
        );
        let strong: Vec<_> = (0..12).map(|f| outcome(f, 0, true, false)).collect();
        let summary = paired_summary(&strong, 100, 42);
        assert_eq!(classify_guarded(&summary, 0.1, 10), ResultClass::Supported);
    }

    #[test]
    fn empty_equal_and_single_family() {
        let empty = paired_summary(&[], 100, 1);
        assert_eq!((empty.n, empty.families), (0, 0));
        assert_eq!((empty.x_rate, empty.y_rate, empty.lift), (0.0, 0.0, 0.0));
        assert_eq!(
            (empty.ci_low, empty.ci_high, empty.mcnemar_exact_p),
            (0.0, 0.0, 1.0)
        );
        let equal: Vec<_> = (0..10)
            .map(|i| outcome(i, 0, i % 2 == 0, i % 2 == 0))
            .collect();
        let summary = paired_summary(&equal, 1000, 2);
        assert_eq!(
            (summary.lift, summary.ci_low, summary.ci_high),
            (0.0, 0.0, 0.0)
        );
        let single = [outcome(0, 0, true, false), outcome(0, 1, false, false)];
        let summary = paired_summary(&single, 100, 3);
        assert_eq!(
            (summary.lift, summary.ci_low, summary.ci_high),
            (0.5, 0.5, 0.5)
        );
        assert_eq!(paired_summary(&single, 0, 3), summary);
    }

    #[test]
    fn perfect_treatment_supported() {
        let obs: Vec<_> = (0..10).map(|i| outcome(i, 0, true, false)).collect();
        let s = paired_summary(&obs, 1000, 10);
        assert_eq!(
            (s.n, s.families, s.discordant_x_only, s.discordant_y_only),
            (10, 10, 10, 0)
        );
        assert_eq!(
            (s.x_rate, s.y_rate, s.lift, s.ci_low, s.ci_high),
            (1.0, 0.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(classify(&s, 0.10), ResultClass::Supported);
    }

    #[test]
    fn whole_family_resampling_and_determinism() {
        let obs: Vec<_> = (0..2)
            .flat_map(|f| (0..30).map(move |t| outcome(f, t, f == 0, f == 1)))
            .collect();
        let s = paired_summary(&obs, 2000, 42);
        assert_eq!(s, paired_summary(&obs, 2000, 42));
        assert!(s.ci_low <= -0.4 && s.ci_high >= 0.4);
        assert!(s.ci_low < 0.0 && s.ci_high > 0.0);
        assert_eq!(classify(&s, 0.10), ResultClass::Inconclusive);
        let draws: Vec<_> = (0..10)
            .map(|seed| paired_summary(&obs, 1, seed).ci_low)
            .collect();
        assert!(draws.iter().any(|v| *v != draws[0]));
    }

    #[test]
    fn small_effect_ruled_out() {
        let obs: Vec<_> = (0..200).map(|f| outcome(f, 0, f < 4, false)).collect();
        let s = paired_summary(&obs, 2000, 23);
        assert!((s.lift - 0.02).abs() < 1e-12);
        assert!(s.ci_high < 0.10);
        assert_eq!(classify(&s, 0.10), ResultClass::RuledOut);
    }

    #[test]
    fn unequal_family_sizes_are_observation_weighted() {
        let mut obs = vec![outcome(0, 0, true, false)];
        obs.extend((0..3).map(|t| outcome(1, t, false, true)));
        let s = paired_summary(&obs, 1, 0);
        assert_eq!((s.x_rate, s.y_rate, s.lift), (0.25, 0.75, -0.5));
        // SplitMix64(0)'s first two draws select family 1, then family 0.
        assert_eq!((s.ci_low, s.ci_high), (-0.5, -0.5));
        let wide = paired_summary(&obs, 1000, 0);
        assert_eq!((wide.ci_low, wide.ci_high), (-1.0, 1.0));
    }

    #[test]
    fn classification_boundaries() {
        let mut s = paired_summary(&[], 0, 0);
        s.lift = 0.10;
        s.ci_high = 0.10;
        assert_eq!(classify(&s, 0.10), ResultClass::Inconclusive);
        s.ci_low = 0.01;
        assert_eq!(classify(&s, 0.10), ResultClass::Supported);
        s.lift = 0.09;
        assert_eq!(classify(&s, 0.10), ResultClass::Inconclusive);
        s.ci_high = 0.099;
        assert_eq!(classify(&s, 0.10), ResultClass::RuledOut);
    }
}
