// ABOUTME: Formatting logic for dice roll results into human-readable strings.
// ABOUTME: Converts roll data (dice values, modifiers, crits) into display expressions.

use crate::ast::{
    AnnotationRule, Compare, Condition, DicePool, DieKind, EdgePolicy, MarvelOutcome, RollModifier,
    RollOutcome, RollPlan, ScoringMode, SymbolsOutcome,
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
        RollOutcome::Symbols(symbols) => format_narrative_roll(plan, dice, symbols),
        RollOutcome::Numeric(total) | RollOutcome::Successes(total) => match plan.scoring() {
            ScoringMode::DigitConcatenate => format_digit_roll(plan, dice, total),
            ScoringMode::Sum | ScoringMode::CountSuccesses(_) => {
                format_standard_roll(plan, dice, total)
            }
            // Marvel routes via the outer match's Marvel arm, SymbolCancel via
            // its Symbols arm; neither yields a Numeric/Successes outcome.
            ScoringMode::MarvelMultiverse | ScoringMode::SymbolCancel => unreachable!(),
        },
    }
}

/// Format a Marvel Multiverse 3dMarvel roll.
///
/// Renders `3dMarvel[l, M, r] = total` where each die renders via its face
/// identity (the M face renders `M`), with an auto-fail or M-shown suffix.
fn format_marvel_roll(plan: &RollPlan, dice: &[DieResult], marvel: MarvelOutcome) -> String {
    let dice_str: String = dice
        .iter()
        .map(|d| d.face.to_string())
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
        plan.pools()[0].count,
        modifiers,
        dice_str,
        marvel.total,
        suffix
    )
}

/// Format a narrative (Genesys/Star Wars) symbol-cancellation roll.
///
/// Renders `<notation>[<group faces>] = <net facts>`, e.g.
/// `2dAbility&1dDifficulty[SA, AA | Th] = 1 success, 2 advantages`. Group
/// boundaries are re-derived from the pool counts (see `narrative_dice_str`).
fn format_narrative_roll(plan: &RollPlan, dice: &[DieResult], outcome: SymbolsOutcome) -> String {
    format!(
        "{}[{}] = {}",
        narrative_notation(plan.pools()),
        narrative_dice_str(plan.pools(), dice),
        narrative_outcome_str(&outcome),
    )
}

/// Render the pool notation for a narrative roll, e.g. `2dAbility&1dDifficulty`.
fn narrative_notation(pools: &[DicePool]) -> String {
    pools
        .iter()
        .map(|pool| match pool.kind {
            DieKind::Narrative(die) => format!("{}d{}", pool.count, die),
            // Narrative scoring only ever holds narrative pool groups.
            DieKind::Number(_) | DieKind::Percent | DieKind::Fudge | DieKind::MarvelD6 => {
                unreachable!("narrative formatting requires narrative die kinds")
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Render the per-group die faces of a narrative roll, groups separated by
/// ` | ` in pool order and faces within a group by `, `.
///
/// Group boundaries are re-derived from `pools()` counts. This is valid
/// precisely because narrative pools admit no dice-count-changing modifiers;
/// if a narrative modifier is ever added, this derivation must be revisited.
fn narrative_dice_str(pools: &[DicePool], dice: &[DieResult]) -> String {
    let mut groups = Vec::with_capacity(pools.len());
    let mut start = 0;
    for pool in pools {
        let end = start + pool.count as usize;
        let faces = dice[start..end]
            .iter()
            .map(|d| d.face.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        groups.push(faces);
        start = end;
    }
    groups.join(" | ")
}

/// Render a narrative outcome as a comma-separated list of net facts, or
/// `wash` when all six outcome fields are zero.
fn narrative_outcome_str(o: &SymbolsOutcome) -> String {
    let mut parts = Vec::new();
    if o.successes > 0 {
        parts.push(count_word(o.successes, "success", "successes"));
    } else if o.successes < 0 {
        parts.push(count_word(-o.successes, "failure", "failures"));
    }
    if o.advantages > 0 {
        parts.push(count_word(o.advantages, "advantage", "advantages"));
    } else if o.advantages < 0 {
        parts.push(count_word(-o.advantages, "threat", "threats"));
    }
    if o.triumphs > 0 {
        parts.push(count_word(o.triumphs as i64, "triumph", "triumphs"));
    }
    if o.despairs > 0 {
        parts.push(count_word(o.despairs as i64, "despair", "despairs"));
    }
    if o.light > 0 {
        parts.push(format!("{} light", o.light));
    }
    if o.dark > 0 {
        parts.push(format!("{} dark", o.dark));
    }
    if parts.is_empty() {
        "wash".to_string()
    } else {
        parts.join(", ")
    }
}

/// Format a count with singular/plural noun, e.g. `1 success`, `2 successes`.
fn count_word(n: i64, singular: &str, plural: &str) -> String {
    format!("{} {}", n, if n == 1 { singular } else { plural })
}

/// Render the modifier portion of a roll notation (e.g., "kh3!p>4r<3").
fn modifiers_str(plan: &RollPlan) -> String {
    plan.modifiers()
        .iter()
        .map(|m| match m {
            RollModifier::KeepHighest(n) => format!("kh{}", n),
            RollModifier::KeepLowest(n) => format!("kl{}", n),
            RollModifier::DropHighest(n) => format!("dh{}", n),
            RollModifier::DropLowest(n) => format!("dl{}", n),
            RollModifier::Explode {
                compounding,
                penetrating,
                limit,
                condition,
            } => {
                let mut s = "!".to_string();
                if *compounding {
                    s.push('!');
                }
                if *penetrating {
                    s.push('p');
                }
                if let Some(n) = limit {
                    s.push_str(&n.to_string());
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
    let sides = match plan.pools()[0].kind {
        DieKind::Number(n) => n.to_string(),
        // The parser only pairs DigitConcatenate with DieKind::Number.
        DieKind::Percent | DieKind::Fudge | DieKind::MarvelD6 => {
            unreachable!("DigitConcatenate requires DieKind::Number")
        }
        // Narrative dice never reach digit formatting.
        DieKind::Narrative(_) => unreachable!("narrative dice do not use digit formatting"),
    };
    let prefix =
        std::iter::repeat_n(sides.as_str(), plan.pools()[0].count as usize).collect::<String>();
    let dice_str = dice
        .iter()
        .map(|d| d.face.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("D{}[{}] = {}", prefix, dice_str, total)
}

/// Format a standard (sum or success-counting) roll.
fn format_standard_roll(plan: &RollPlan, dice: &[DieResult], total: i64) -> String {
    let kind_str = match plan.pools()[0].kind {
        DieKind::Number(n) => n.to_string(),
        DieKind::Percent => "%".to_string(),
        DieKind::Fudge => "F".to_string(),
        DieKind::MarvelD6 => "Marvel".to_string(),
        // Narrative dice never reach standard formatting.
        DieKind::Narrative(_) => unreachable!("narrative dice do not use standard formatting"),
    };

    let mut modifiers: String = modifiers_str(plan);

    // Success-counting scoring renders its condition after the modifiers.
    let success_condition: Option<&Condition> = match plan.scoring() {
        ScoringMode::CountSuccesses(cond) => {
            modifiers.push_str(&format!("{}{}", cond.compare, cond.value));
            Some(cond)
        }
        ScoringMode::Sum => None,
        // Routed to format_digit_roll, format_marvel_roll, or
        // format_narrative_roll before reaching here.
        ScoringMode::DigitConcatenate
        | ScoringMode::MarvelMultiverse
        | ScoringMode::SymbolCancel => unreachable!(),
    };

    // Render crit markers: cs before cf, at most one of each.
    let crit_str = format!(
        "{}{}",
        crit_success_str(plan.annotation_rules()),
        crit_failure_str(plan.annotation_rules()),
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
                    .check(d.face.numeric_value(), condition.value)
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
            plan.pools()[0].count,
            kind_str,
            modifiers,
            crit_str,
            dice_str,
            total,
            success_word
        )
    } else {
        format!(
            "{}d{}{}{}[{}] = {}",
            plan.pools()[0].count,
            kind_str,
            modifiers,
            crit_str,
            dice_str,
            total
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, NarrativeDie};
    use crate::roller::evaluate_with_rng;
    use crate::test_support::TestRng;

    // format.rs had zero tests despite 95.8% incidental region coverage --
    // exercised only via roller/parser tests asserting on totals, not
    // rendered text. Every assertion here pins the exact rendered string.

    #[test]
    fn display_impl_matches_expression() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![1, 2, 3, 4]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        // fmt-23: the Display impl can write the empty string and nothing
        // that only asserts on `.expression` would notice.
        assert_eq!(result.to_string(), result.expression);
        assert_eq!(result.to_string(), "4d6[1, 2, 3, 4] = 10");
    }

    #[test]
    fn percent_die_renders_percent_symbol() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Percent,
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![42]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "1d%[42] = 42");
    }

    #[test]
    fn fudge_die_renders_f_symbol() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Fudge,
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![1, 2, 3, 2]); // -1, 0, 1, 0
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4dF[-1, 0, 1, 0] = 0");
    }

    #[test]
    fn drop_lowest_modifier_renders_lowercase_dl() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::DropLowest(1)],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![1, 5, 3, 6]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4d6dl1[(1), 5, 3, 6] = 14");
    }

    #[test]
    fn keep_lowest_modifier_renders_lowercase_kl() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::KeepLowest(1)],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![1, 5, 3, 6]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4d6kl1[1, (5), (3), (6)] = 1");
    }

    #[test]
    fn drop_highest_modifier_renders_lowercase_dh() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::DropHighest(1)],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![1, 5, 3, 6]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4d6dh1[1, 5, 3, (6)] = 9");
    }

    #[test]
    fn reroll_once_flag_renders_lowercase_o() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Reroll {
                once: true,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![1, 5, 3, 6]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4d6ro[1, 5, 3, 6] = 15");
    }

    #[test]
    fn crit_success_condition_marker_omits_equal_operator() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 6,
            })],
        );
        let mut rng = TestRng::new(vec![1, 6, 3, 4]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        // fmt-18: condition_marker_str must render "cs6", not "cs=6".
        assert_eq!(result.expression, "4d6cs6[1, 6**, 3, 4] = 14");
    }

    #[test]
    fn crit_success_condition_marker_includes_non_equal_operator() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::GreaterOrEqual,
                value: 5,
            })],
        );
        let mut rng = TestRng::new(vec![1, 6, 3, 4]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4d6cs>=5[1, 6**, 3, 4] = 14");
    }

    #[test]
    fn crit_failure_marker_appears_in_notation() {
        // fmt-25: crit_failure_str always returning None drops the cf
        // marker from the echoed notation while the per-die `*` marker
        // (already asserted elsewhere) stays intact.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalFailure(Condition {
                compare: Compare::Equal,
                value: 1,
            })],
        );
        let mut rng = TestRng::new(vec![1, 6, 3, 4]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "4d6cf1[1*, 6, 3, 4] = 14");
    }

    #[test]
    fn narrative_light_label_is_lowercase() {
        // fmt-21: `dark` is asserted elsewhere in the suite but `light`
        // was not -- Force roll 8 renders a single Light pip.
        let plan = RollPlan::new_unchecked_pools(
            vec![DicePool {
                count: 1,
                kind: DieKind::Narrative(NarrativeDie::Force),
            }],
            vec![],
            ScoringMode::SymbolCancel,
            vec![AnnotationRule::Triumph, AnnotationRule::Despair],
        );
        let mut rng = TestRng::new(vec![8]);
        let result = evaluate_with_rng(&Expr::Roll(plan), &mut rng).unwrap();

        assert_eq!(result.expression, "1dForce[L] = 1 light");
    }
}
