// ABOUTME: Formatting logic for dice roll results into human-readable strings.
// ABOUTME: Converts roll data (dice values, modifiers, crits) into display expressions.

use crate::ast::{
    AnnotationRule, Compare, Condition, DieKind, RollModifier, RollPlan, ScoringMode,
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
pub(crate) fn format_roll(plan: &RollPlan, dice: &[DieResult], total: i64) -> String {
    match plan.scoring {
        ScoringMode::DigitConcatenate => format_digit_roll(plan, dice, total),
        ScoringMode::Sum | ScoringMode::CountSuccesses(_) => {
            format_standard_roll(plan, dice, total)
        }
    }
}

/// Format a digit-concatenation roll (e.g., D66: "D66[3, 5] = 35").
///
/// Digit dice carry no modifiers or annotations from the parser, so dropped
/// dice and crit markers do not arise here; values render plain.
fn format_digit_roll(plan: &RollPlan, dice: &[DieResult], total: i64) -> String {
    let sides = match plan.pool.kind {
        DieKind::Number(n) => n.to_string(),
        // The parser only pairs DigitConcatenate with DieKind::Number.
        DieKind::Percent | DieKind::Fudge => {
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
    };

    let mut modifiers_str: String = plan
        .modifiers
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
        })
        .collect();

    // Success-counting scoring renders its condition after the modifiers.
    let success_condition: Option<&Condition> = match &plan.scoring {
        ScoringMode::CountSuccesses(cond) => {
            modifiers_str.push_str(&format!("{}{}", cond.compare, cond.value));
            Some(cond)
        }
        ScoringMode::Sum => None,
        // Routed to format_digit_roll before reaching here.
        ScoringMode::DigitConcatenate => unreachable!(),
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
            plan.pool.count, kind_str, modifiers_str, crit_str, dice_str, total, success_word
        )
    } else {
        format!(
            "{}d{}{}{}[{}] = {}",
            plan.pool.count, kind_str, modifiers_str, crit_str, dice_str, total
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
