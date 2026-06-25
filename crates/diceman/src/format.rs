// ABOUTME: Formatting logic for dice roll results into human-readable strings.
// ABOUTME: Converts roll data (dice values, modifiers, crits) into display expressions.

use crate::ast::{Compare, Condition, Modifier, Roll, DieKind};
use crate::roller::{DieResult, RollResult};
use std::fmt;

impl fmt::Display for RollResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expression)
    }
}

/// Format a dice roll into a human-readable expression string.
///
/// Produces output like "4d6kh3[6, 5, 4, (1)] = 15" showing the notation,
/// individual die values (with dropped/crit markers), and the total.
pub(crate) fn format_roll(
    roll: &Roll,
    dice: &[DieResult],
    total: i64,
    success_condition: Option<&Condition>,
) -> String {
    let kind_str = match roll.kind {
        DieKind::Number(n) => n.to_string(),
        DieKind::Percent => "%".to_string(),
        DieKind::Fudge => "F".to_string(),
    };

    let modifiers_str: String = roll
        .modifiers
        .iter()
        .map(|m| match m {
            Modifier::KeepHighest(n) => format!("kh{}", n),
            Modifier::KeepLowest(n) => format!("kl{}", n),
            Modifier::DropHighest(n) => format!("dh{}", n),
            Modifier::DropLowest(n) => format!("dl{}", n),
            Modifier::Explode {
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
            Modifier::Reroll { once, condition } => {
                let mut s = "r".to_string();
                if *once {
                    s.push('o');
                }
                if let Some(c) = condition {
                    s.push_str(&format!("{}{}", c.compare, c.value));
                }
                s
            }
            Modifier::CountSuccesses(c) => {
                format!("{}{}", c.compare, c.value)
            }
        })
        .collect();

    let crit_str = format!(
        "{}{}",
        roll.crit_success.as_ref().map_or(String::new(), |c| {
            if c.compare == Compare::Equal {
                format!("cs{}", c.value)
            } else {
                format!("cs{}{}", c.compare, c.value)
            }
        }),
        roll.crit_failure.as_ref().map_or(String::new(), |c| {
            if c.compare == Compare::Equal {
                format!("cf{}", c.value)
            } else {
                format!("cf{}{}", c.compare, c.value)
            }
        }),
    );

    // Format dice, marking crits and successes
    let dice_str: String = dice
        .iter()
        .map(|d| {
            if d.dropped {
                format!("({})", d.value)
            } else if d.is_crit_success {
                format!("{}**", d.value)
            } else if d.is_crit_failure {
                format!("{}*", d.value)
            } else if let Some(condition) = success_condition {
                if condition.compare.check(d.value, condition.value) {
                    format!("{}*", d.value) // Success counting marker
                } else {
                    d.value.to_string()
                }
            } else {
                d.value.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    if success_condition.is_some() {
        let success_word = if total == 1 { "success" } else { "successes" };
        format!(
            "{}d{}{}{}[{}] = {} {}",
            roll.count, kind_str, modifiers_str, crit_str, dice_str, total, success_word
        )
    } else {
        format!(
            "{}d{}{}{}[{}] = {}",
            roll.count, kind_str, modifiers_str, crit_str, dice_str, total
        )
    }
}
