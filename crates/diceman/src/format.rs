// ABOUTME: Formatting logic for dice roll results into human-readable strings.
// ABOUTME: Converts roll data (dice values, modifiers, crits) into display expressions.

use crate::ast::{
    AnnotationRule, Compare, Condition, DieKind, EdgePolicy, MarvelOutcome, RollModifier,
    RollOutcome, RollPlan, ScoringMode,
};
use crate::roller::{DieResult, RollResult};
use std::fmt;

impl fmt::Display for RollResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expression)
    }
}

/// Format a dice roll plan into a human-readable expression string.
///
/// Produces output like "4d6kh3[6, 5, 4, (1)] = 15" showing the notation,
/// individual die values (with dropped/crit markers), and the total.
pub(crate) fn format_roll(plan: &RollPlan, dice: &[DieResult], outcome: RollOutcome) -> String {
    match outcome {
        RollOutcome::Marvel(marvel) => format_marvel_roll(plan, dice, marvel),
        _ => {
            let total = outcome.as_numeric().expect(
                "Sum/CountSuccesses/DigitConcatenate scoring always yields a numeric outcome",
            );
            match plan.scoring {
                ScoringMode::DigitConcatenate => format_digit_roll(plan, dice, total),
                ScoringMode::Sum | ScoringMode::CountSuccesses(_) => {
                    format_standard_roll(plan, dice, total)
                }
                // Routed to format_marvel_roll via the outer match.
                ScoringMode::MarvelMultiverse => unreachable!(),
            }
        }
    }
}

/// Format a Marvel Multiverse 3dMarvel roll.
///
/// Renders `3dMarvel[l, M, r] = total` where the middle die shows `M` when its
/// face is 1 (otherwise the face value), with an auto-fail or M-shown suffix.
fn format_marvel_roll(plan: &RollPlan, dice: &[DieResult], marvel: MarvelOutcome) -> String {
    let dice_str: String = dice
        .iter()
        .enumerate()
        .map(|(i, d)| {
            if i == 1 && d.face.as_numeric() == 1 {
                "M".to_string()
            } else {
                d.face.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let suffix = if marvel.auto_fail {
        " (auto-fail)"
    } else if marvel.m_shown {
        " (M shown)"
    } else {
        ""
    };

    let modifiers = modifiers_str(plan);
    format!(
        "{}dMarvel{}[{}] = {}{}",
        plan.pool.count, modifiers, dice_str, marvel.total, suffix
    )
}

/// Render the modifier portion of a roll notation (e.g., "kh3!p>4r<3").
fn modifiers_str(plan: &RollPlan) -> String {
    plan.modifiers
        .iter()
        .map(|m| match m {
            RollModifier::KeepHighest(n) => format!("kh{}", n),
            RollModifier::KeepLowest(n) => format!("kl{}", n),
            RollModifier::DropHighest(n) => format!("dh{}", n),
            RollModifier::DropLowest(n) => format!("dl{}", n),
            RollModifier::Explode {
                compounding,
                penetrating,
                condition,
            } => {
                let mut s = "!".to_string();
                if *compounding {
                    s.push('!');
                }
                if *penetrating {
                    s.push('p');
                }
                if let Some(c) = condition {
                    s.push_str(&format!("{}{}", c.compare, c.value));
                }
                s
            }
            RollModifier::Reroll { once, condition } => {
                let mut s = "r".to_string();
                if *once {
                    s.push('o');
                }
                if let Some(c) = condition {
                    s.push_str(&format!("{}{}", c.compare, c.value));
                }
                s
            }
            RollModifier::Edge { count, policy } => match policy {
                // Both policies render as the `eN` token: ChaseFantastic has no
                // notation form of its own (it is an API/CLI-only policy), so it
                // shares Edge's token. The arms are kept explicit so a future
                // EdgePolicy variant must decide its own rendering.
                EdgePolicy::RerollLowest => format!("e{}", count),
                EdgePolicy::ChaseFantastic => format!("e{}", count),
            },
            RollModifier::Trouble { count } => format!("t{}", count),
        })
        .collect()
}

/// Format a digit-concatenation roll (e.g., D66: "D66[3, 5] = 35").
///
/// Digit dice carry no modifiers or annotations from the parser, so dropped
/// dice and crit markers do not arise here; values render plain.
fn format_digit_roll(plan: &RollPlan, dice: &[DieResult], total: i64) -> String {
    let sides = match plan.pool.kind {
        DieKind::Number(n) => n.to_string(),
        // The parser only pairs DigitConcatenate with DieKind::Number.
        DieKind::Percent | DieKind::Fudge | DieKind::MarvelD6 => {
            unreachable!("DigitConcatenate requires DieKind::Number")
        }
    };
    let prefix = std::iter::repeat_n(sides.as_str(), plan.pool.count as usize).collect::<String>();
    let dice_str = dice
        .iter()
        .map(|d| d.face.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("D{}[{}] = {}", prefix, dice_str, total)
}

/// Format a standard (sum or success-counting) roll.
fn format_standard_roll(plan: &RollPlan, dice: &[DieResult], total: i64) -> String {
    let kind_str = match plan.pool.kind {
        DieKind::Number(n) => n.to_string(),
        DieKind::Percent => "%".to_string(),
        DieKind::Fudge => "F".to_string(),
        DieKind::MarvelD6 => "Marvel".to_string(),
    };

    let mut modifiers: String = modifiers_str(plan);

    // Success-counting scoring renders its condition after the modifiers.
    let success_condition: Option<&Condition> = match &plan.scoring {
        ScoringMode::CountSuccesses(cond) => {
            modifiers.push_str(&format!("{}{}", cond.compare, cond.value));
            Some(cond)
        }
        ScoringMode::Sum => None,
        // Routed to format_digit_roll or format_marvel_roll before reaching here.
        ScoringMode::DigitConcatenate | ScoringMode::MarvelMultiverse => unreachable!(),
    };

    // Render crit markers: cs before cf, at most one of each.
    let crit_str = format!(
        "{}{}",
        crit_success_str(&plan.annotation_rules),
        crit_failure_str(&plan.annotation_rules),
    );

    // Format dice, marking crits and successes
    let dice_str: String = dice
        .iter()
        .map(|d| {
            if d.dropped {
                format!("({})", d.face)
            } else if d.is_crit_success {
                format!("{}**", d.face)
            } else if d.is_crit_failure {
                format!("{}*", d.face)
            } else if let Some(condition) = success_condition {
                if condition
                    .compare
                    .check(d.face.as_numeric(), condition.value)
                {
                    format!("{}*", d.face) // Success counting marker
                } else {
                    d.face.to_string()
                }
            } else {
                d.face.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    if success_condition.is_some() {
        let success_word = if total == 1 { "success" } else { "successes" };
        format!(
            "{}d{}{}{}[{}] = {} {}",
            plan.pool.count, kind_str, modifiers, crit_str, dice_str, total, success_word
        )
    } else {
        format!(
            "{}d{}{}{}[{}] = {}",
            plan.pool.count, kind_str, modifiers, crit_str, dice_str, total
        )
    }
}

/// Render the first CriticalSuccess annotation rule as a `cs...` string, if any.
fn crit_success_str(rules: &[AnnotationRule]) -> String {
    rules
        .iter()
        .find_map(|r| match r {
            AnnotationRule::CriticalSuccess(c) => Some(condition_marker_str("cs", c)),
            _ => None,
        })
        .unwrap_or_default()
}

/// Render the first CriticalFailure annotation rule as a `cf...` string, if any.
fn crit_failure_str(rules: &[AnnotationRule]) -> String {
    rules
        .iter()
        .find_map(|r| match r {
            AnnotationRule::CriticalFailure(c) => Some(condition_marker_str("cf", c)),
            _ => None,
        })
        .unwrap_or_default()
}

/// Format a crit marker string, omitting the `=` for Equal comparisons.
fn condition_marker_str(prefix: &str, c: &Condition) -> String {
    if c.compare == Compare::Equal {
        format!("{}{}", prefix, c.value)
    } else {
        format!("{}{}{}", prefix, c.compare, c.value)
    }
}
