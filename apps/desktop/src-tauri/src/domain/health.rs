//! Account health grading from normalized quota values.
//!
//! Thresholds follow the PRD comprehensive-status rules: an account needs
//! attention once any reported window's remaining ratio drops below 25%, and
//! is critical below 10%. Missing or unmeasurable values never grade as
//! exhausted — unknown is not zero.

/// Remaining-ratio bound below which an account needs attention.
pub const ATTENTION_RATIO: f64 = 0.25;
/// Remaining-ratio bound below which an account is critical.
pub const CRITICAL_RATIO: f64 = 0.10;

/// Data-driven severity of an account's latest quota values, ordered weakest
/// to strongest so [`worst_grade`] can take the maximum.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum HealthGrade {
    Healthy,
    Attention,
    Critical,
}

/// Remaining share of a quota window, or `None` when the provider reports no
/// usable pair. Prefers `remaining`/`limit` and falls back to deriving the
/// rest from `used`/`limit`. Non-finite or non-positive limits yield `None`;
/// results are clamped into `0.0..=1.0` so bogus inputs can only soften, not
/// sharpen, a grade.
pub fn remaining_ratio(
    remaining: Option<f64>,
    used: Option<f64>,
    limit: Option<f64>,
) -> Option<f64> {
    let limit = limit?;
    if !limit.is_finite() || limit <= 0.0 {
        return None;
    }
    if let Some(remaining) = remaining.filter(|value| value.is_finite()) {
        return Some(remaining.clamp(0.0, limit) / limit);
    }
    let used = used.filter(|value| value.is_finite() && *value >= 0.0)?;
    Some((limit - used).clamp(0.0, limit) / limit)
}

/// Maps a remaining ratio onto the PRD thresholds (strictly below the bound).
pub fn grade_for_ratio(ratio: f64) -> HealthGrade {
    if ratio < CRITICAL_RATIO {
        HealthGrade::Critical
    } else if ratio < ATTENTION_RATIO {
        HealthGrade::Attention
    } else {
        HealthGrade::Healthy
    }
}

/// Worst grade across every measurable ratio; `None` when nothing is
/// measurable and the caller must keep the pre-existing status.
pub fn worst_grade<I>(ratios: I) -> Option<HealthGrade>
where
    I: IntoIterator<Item = Option<f64>>,
{
    ratios.into_iter().flatten().map(grade_for_ratio).max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_threshold_is_strict() {
        assert_eq!(grade_for_ratio(0.25), HealthGrade::Healthy);
        assert_eq!(grade_for_ratio(0.249_999), HealthGrade::Attention);
    }

    #[test]
    fn critical_threshold_is_strict() {
        assert_eq!(grade_for_ratio(0.10), HealthGrade::Attention);
        assert_eq!(grade_for_ratio(0.099_999), HealthGrade::Critical);
        assert_eq!(grade_for_ratio(0.0), HealthGrade::Critical);
    }

    #[test]
    fn ratio_prefers_remaining_and_falls_back_to_used() {
        assert_eq!(
            remaining_ratio(Some(20.0), Some(80.0), Some(100.0)),
            Some(0.2)
        );
        assert_eq!(remaining_ratio(None, Some(80.0), Some(100.0)), Some(0.2));
        assert_eq!(remaining_ratio(Some(20.0), None, Some(100.0)), Some(0.2));
    }

    #[test]
    fn missing_or_invalid_limit_is_unmeasurable() {
        assert_eq!(remaining_ratio(Some(1.0), None, None), None);
        assert_eq!(remaining_ratio(Some(1.0), None, Some(0.0)), None);
        assert_eq!(remaining_ratio(Some(1.0), None, Some(-5.0)), None);
        assert_eq!(remaining_ratio(Some(1.0), None, Some(f64::INFINITY)), None);
    }

    #[test]
    fn non_finite_values_do_not_invent_data() {
        assert_eq!(
            remaining_ratio(Some(f64::NAN), Some(80.0), Some(100.0)),
            Some(0.2)
        );
        assert_eq!(remaining_ratio(None, Some(f64::NAN), Some(100.0)), None);
        assert_eq!(remaining_ratio(None, Some(-10.0), Some(100.0)), None);
    }

    #[test]
    fn ratio_is_clamped_into_unit_range() {
        assert_eq!(remaining_ratio(Some(150.0), None, Some(100.0)), Some(1.0));
        assert_eq!(remaining_ratio(Some(-5.0), None, Some(100.0)), Some(0.0));
        assert_eq!(remaining_ratio(None, Some(-10.0), Some(100.0)), None);
        assert_eq!(remaining_ratio(None, Some(250.0), Some(100.0)), Some(0.0));
    }

    #[test]
    fn worst_grade_takes_the_strongest_signal() {
        assert_eq!(worst_grade([]), None);
        assert_eq!(worst_grade([None]), None);
        assert_eq!(
            worst_grade([
                remaining_ratio(Some(0.5), None, Some(1.0)),
                remaining_ratio(Some(0.2), None, Some(1.0)),
                remaining_ratio(Some(0.05), None, Some(1.0)),
            ]),
            Some(HealthGrade::Critical)
        );
        assert_eq!(
            worst_grade([remaining_ratio(Some(0.3), None, Some(1.0)), None]),
            Some(HealthGrade::Healthy)
        );
    }
}
