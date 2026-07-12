// ABOUTME: Recursive descent parser for dice notation expressions.
// ABOUTME: Converts token streams into an AST.

use crate::ast::{
    AnnotationRule, Compare, Condition, DicePool, DieKind, EdgePolicy, Expr, NarrativeDie, Op,
    RollModifier, RollPlan, ScoringMode,
};
use crate::error::{Error, Result};
use crate::lexer::{Lexer, Token};
use crate::roller::MAX_REROLLS;

/// Parser for dice notation expressions.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    /// Create a new parser for the given input.
    pub fn new(input: &'a str) -> Result<Self> {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token()?;
        Ok(Self { lexer, current })
    }

    /// Parse the input into an expression.
    pub fn parse(&mut self) -> Result<Expr> {
        let expr = self.expression()?;
        if self.current != Token::Eof {
            return Err(Error::Expected {
                expected: "end of input".to_string(),
                found: format!("{:?}", self.current),
            });
        }
        Ok(expr)
    }

    fn advance(&mut self) -> Result<Token> {
        let prev = std::mem::replace(&mut self.current, self.lexer.next_token()?);
        Ok(prev)
    }

    fn expect(&mut self, expected: Token) -> Result<()> {
        if self.current == expected {
            self.advance()?;
            Ok(())
        } else {
            Err(Error::Expected {
                expected: format!("{:?}", expected),
                found: format!("{:?}", self.current),
            })
        }
    }

    /// Parse an expression (handles + and -).
    fn expression(&mut self) -> Result<Expr> {
        let mut left = self.term()?;

        loop {
            let op = match self.current {
                Token::Plus => Op::Add,
                Token::Minus => Op::Sub,
                _ => break,
            };
            self.advance()?;
            let right = self.term()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse a term (handles * and /).
    fn term(&mut self) -> Result<Expr> {
        let mut left = self.factor()?;

        loop {
            let op = match self.current {
                Token::Star => Op::Mul,
                Token::Slash => Op::Div,
                _ => break,
            };
            self.advance()?;
            let right = self.factor()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse a factor (number, roll, or parenthesized expression).
    fn factor(&mut self) -> Result<Expr> {
        match &self.current {
            Token::Number(_) => self.roll_or_number(),
            Token::D => self.roll_or_number(),
            Token::DigitD => self.digit_roll(),
            Token::LParen => {
                self.advance()?;
                let expr = self.expression()?;
                self.expect(Token::RParen)?;
                Ok(Expr::Group(Box::new(expr)))
            }
            Token::Minus => {
                self.advance()?;
                let expr = self.factor()?;
                Ok(Expr::BinOp {
                    op: Op::Sub,
                    left: Box::new(Expr::Number(0)),
                    right: Box::new(expr),
                })
            }
            _ => Err(Error::Expected {
                expected: "number, dice roll, or '('".to_string(),
                found: format!("{:?}", self.current),
            }),
        }
    }

    /// Parse a dice roll or plain number.
    fn roll_or_number(&mut self) -> Result<Expr> {
        // Get the optional count
        let count = if let Token::Number(n) = self.current {
            self.advance()?;
            n
        } else {
            1 // Default to 1 die
        };

        // Check if this is a dice roll or just a number
        if self.current != Token::D {
            return Ok(Expr::Number(count as i64));
        }

        // It's a dice roll - consume the 'd'
        self.advance()?;

        // Parse the die kind
        let kind = self.kind()?;

        // Narrative die kinds enter the pool-union production instead of the
        // regular single-pool modifier/scoring/annotation pipeline.
        if matches!(kind, DieKind::Narrative(_)) {
            return self.narrative_roll(count, kind);
        }

        // Parse any modifiers (and success-counting scoring)
        let (modifiers, scoring) = self.modifiers()?;

        // Parse critical markers (cs and cf can appear in any order)
        let mut annotation_rules = self.annotation_rules()?;

        // Validate: cs/cf cannot combine with success counting
        if matches!(scoring, ScoringMode::CountSuccesses(_)) && !annotation_rules.is_empty() {
            return Err(Error::CritWithSuccessCounting);
        }

        let scoring = if kind == DieKind::MarvelD6 {
            Self::validate_marvel_roll(count, &modifiers, &scoring, &annotation_rules)?;
            annotation_rules.push(AnnotationRule::MarvelFantastic);
            ScoringMode::MarvelMultiverse
        } else {
            if modifiers
                .iter()
                .any(|m| matches!(m, RollModifier::Edge { .. } | RollModifier::Trouble { .. }))
            {
                return Err(Error::InvalidMarvelRoll(
                    "Edge/Trouble modifiers require a 3dMarvel pool".to_string(),
                ));
            }
            scoring
        };

        Ok(Expr::Roll(RollPlan::new_unchecked(
            DicePool { count, kind },
            modifiers,
            scoring,
            annotation_rules,
        )))
    }

    /// Parse a digit dice roll (D66, D666, D44, etc.).
    ///
    /// Lowered into a `RollPlan` whose pool rolls `count` dice of `sides` sides
    /// and whose scoring concatenates the results as digits.
    fn digit_roll(&mut self) -> Result<Expr> {
        // Consume the 'D'
        self.advance()?;

        // Read the number after D
        let value = if let Token::Number(n) = self.current {
            self.advance()?;
            n
        } else {
            return Err(Error::Expected {
                expected: "number after 'D' for digit dice".to_string(),
                found: format!("{:?}", self.current),
            });
        };

        // Decompose into digits and validate all are the same
        let (sides, count) = Self::decompose_digit_dice(value)?;

        Ok(Expr::Roll(RollPlan::new_unchecked(
            DicePool {
                count,
                kind: DieKind::Number(sides),
            },
            vec![],
            ScoringMode::DigitConcatenate,
            vec![],
        )))
    }

    /// Decompose a digit dice value into (sides, count).
    /// All digits must be the same (e.g., 66 → (6, 2), 444 → (4, 3)).
    fn decompose_digit_dice(value: u32) -> Result<(u32, u32)> {
        if value == 0 {
            return Err(Error::InvalidDigitDice(value));
        }

        let mut digits = Vec::new();
        let mut n = value;
        while n > 0 {
            digits.push(n % 10);
            n /= 10;
        }

        let first = digits[0];
        if first == 0 || !digits.iter().all(|&d| d == first) {
            return Err(Error::InvalidDigitDice(value));
        }

        Ok((first, digits.len() as u32))
    }

    /// Parse the die kind (number, %, F, or Marvel).
    fn kind(&mut self) -> Result<DieKind> {
        match &self.current {
            Token::Number(n) => {
                let n = *n;
                self.advance()?;
                Ok(DieKind::Number(n))
            }
            Token::Percent => {
                self.advance()?;
                Ok(DieKind::Percent)
            }
            Token::Fudge => {
                self.advance()?;
                Ok(DieKind::Fudge)
            }
            Token::Marvel => {
                self.advance()?;
                Ok(DieKind::MarvelD6)
            }
            Token::Ability => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Ability))
            }
            Token::Boost => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Boost))
            }
            Token::Setback => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Setback))
            }
            Token::Difficulty => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Difficulty))
            }
            Token::Proficiency => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Proficiency))
            }
            Token::Challenge => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Challenge))
            }
            Token::Force => {
                self.advance()?;
                Ok(DieKind::Narrative(NarrativeDie::Force))
            }
            _ => Err(Error::Expected {
                expected: "dice sides (number, %, F, Marvel, or a narrative die name)".to_string(),
                found: format!("{:?}", self.current),
            }),
        }
    }

    /// Parse the pool-union production: `group ('&' group)*` where the first
    /// group's kind (already parsed as narrative) is `first_count`/`first_kind`
    /// and each subsequent group is `count 'd' narrative_kind`.
    ///
    /// Binds tighter than all arithmetic — it never leaves this production.
    /// A non-narrative kind in a later group is rejected here as
    /// `InvalidNarrativeRoll`; a leading '&' after a non-narrative first
    /// group is never reached (that path returns from the regular
    /// single-pool arm in `roll_or_number` instead, leaving '&' for the
    /// existing trailing-garbage check in `parse`).
    fn narrative_roll(&mut self, first_count: u32, first_kind: DieKind) -> Result<Expr> {
        let mut pools = vec![DicePool {
            count: first_count,
            kind: first_kind,
        }];

        while self.current == Token::Ampersand {
            self.advance()?;

            let count = if let Token::Number(n) = self.current {
                self.advance()?;
                n
            } else {
                1
            };
            self.expect(Token::D)?;
            let kind = self.kind()?;
            if !matches!(kind, DieKind::Narrative(_)) {
                return Err(Error::InvalidNarrativeRoll(format!(
                    "expected a narrative die kind after '&', found {kind:?}"
                )));
            }
            pools.push(DicePool { count, kind });
        }

        // Modifiers, success-counting conditions, and crit markers are
        // parsed (not just left as trailing garbage) so they can be
        // rejected with a narrative-specific error message.
        let (modifiers, scoring) = self.modifiers()?;
        let annotation_rules = self.annotation_rules()?;

        Self::validate_narrative_roll(&pools, &modifiers, &scoring, &annotation_rules)?;

        Ok(Expr::Roll(RollPlan::new_unchecked_pools(
            pools,
            vec![],
            ScoringMode::SymbolCancel,
            vec![AnnotationRule::Triumph, AnnotationRule::Despair],
        )))
    }

    /// Parser-boundary mirror of the Task 5 narrative `RollPlan` invariants
    /// (the Marvel precedent: `validate_marvel_roll`), checked before the
    /// `[Triumph, Despair]` annotation rules are auto-pushed so notation
    /// errors surface with parse-time messages.
    ///
    /// `ast.rs`'s `RollPlan::validate` is the source of truth for the
    /// invariant set; keep the two in sync when either changes.
    fn validate_narrative_roll(
        pools: &[DicePool],
        modifiers: &[RollModifier],
        scoring: &ScoringMode,
        annotation_rules: &[AnnotationRule],
    ) -> Result<()> {
        for pool in pools {
            if pool.count == 0 {
                return Err(Error::InvalidNarrativeRoll(
                    "narrative pool groups must roll at least one die".to_string(),
                ));
            }
        }
        if !modifiers.is_empty() {
            return Err(Error::InvalidNarrativeRoll(
                "narrative rolls do not support modifiers".to_string(),
            ));
        }
        if !matches!(scoring, ScoringMode::Sum) {
            return Err(Error::InvalidNarrativeRoll(
                "narrative rolls do not support success-counting conditions".to_string(),
            ));
        }
        if !annotation_rules.is_empty() {
            return Err(Error::InvalidNarrativeRoll(
                "narrative rolls do not support critical annotations".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_marvel_roll(
        count: u32,
        modifiers: &[RollModifier],
        scoring: &ScoringMode,
        annotation_rules: &[AnnotationRule],
    ) -> Result<()> {
        if count != 3 {
            return Err(Error::InvalidMarvelRoll(format!(
                "expected 3 dice, found {count}"
            )));
        }
        for modifier in modifiers {
            if !matches!(
                modifier,
                RollModifier::Edge { .. } | RollModifier::Trouble { .. }
            ) {
                return Err(Error::InvalidMarvelRoll(format!(
                    "{modifier:?} is not supported on Marvel rolls; only Edge and Trouble"
                )));
            }
        }
        if matches!(scoring, ScoringMode::CountSuccesses(_)) {
            return Err(Error::InvalidMarvelRoll(
                "success counting is not supported on Marvel rolls".to_string(),
            ));
        }
        if !annotation_rules.is_empty() {
            return Err(Error::InvalidMarvelRoll(
                "critical annotations are not supported on Marvel rolls".to_string(),
            ));
        }

        Ok(())
    }

    /// Parse modifiers (keep, drop, explode, reroll) and success-counting scoring.
    fn modifiers(&mut self) -> Result<(Vec<RollModifier>, ScoringMode)> {
        let mut modifiers = Vec::new();
        let mut scoring = ScoringMode::Sum;

        loop {
            match self.current {
                Token::K => {
                    self.advance()?;
                    modifiers.push(self.keep_modifier()?);
                }
                Token::Explode => {
                    self.advance()?;
                    modifiers.push(self.explode_modifier()?);
                }
                Token::R => {
                    self.advance()?;
                    modifiers.push(self.reroll_modifier()?);
                }
                Token::Edge => {
                    self.advance()?;
                    modifiers.push(self.edge_modifier()?);
                }
                Token::Trouble => {
                    self.advance()?;
                    modifiers.push(self.trouble_modifier()?);
                }
                Token::D => {
                    // In modifier context, 'd' followed by 'h' or 'l' is a drop modifier
                    let next = self.lexer.peek()?;
                    if matches!(next, Token::H | Token::L) {
                        self.advance()?;
                        modifiers.push(self.drop_modifier()?);
                    } else {
                        break;
                    }
                }
                // Comparison operators directly after dice = success counting
                Token::Gt | Token::Lt | Token::Eq => {
                    let condition = self.required_condition()?;
                    scoring = ScoringMode::CountSuccesses(condition);
                }
                _ => break,
            }
        }

        Ok((modifiers, scoring))
    }

    /// Parse a keep modifier (kh3, kl1, k3).
    fn keep_modifier(&mut self) -> Result<RollModifier> {
        let high = match self.current {
            Token::H => {
                self.advance()?;
                true
            }
            Token::L => {
                self.advance()?;
                false
            }
            _ => true, // Default to keep highest
        };

        let count = self.optional_number(1)?;

        if high {
            Ok(RollModifier::KeepHighest(count))
        } else {
            Ok(RollModifier::KeepLowest(count))
        }
    }

    /// Parse a drop modifier (dh3, dl1).
    fn drop_modifier(&mut self) -> Result<RollModifier> {
        let high = match self.current {
            Token::H => {
                self.advance()?;
                true
            }
            Token::L => {
                self.advance()?;
                false
            }
            _ => {
                return Err(Error::Expected {
                    expected: "'h' or 'l' after 'd'".to_string(),
                    found: format!("{:?}", self.current),
                });
            }
        };

        let count = self.optional_number(1)?;

        if high {
            Ok(RollModifier::DropHighest(count))
        } else {
            Ok(RollModifier::DropLowest(count))
        }
    }

    /// Parse an explode modifier (!, !!, !p, !!p, !>5, !!p>5).
    fn explode_modifier(&mut self) -> Result<RollModifier> {
        let compounding = if self.current == Token::Explode {
            self.advance()?;
            true
        } else {
            false
        };

        let penetrating = if self.current == Token::P {
            self.advance()?;
            true
        } else {
            false
        };

        let condition = self.optional_condition()?;

        Ok(RollModifier::Explode {
            compounding,
            penetrating,
            limit: None,
            condition,
        })
    }

    /// Parse a reroll modifier (r, ro, r<3).
    fn reroll_modifier(&mut self) -> Result<RollModifier> {
        let once = if self.current == Token::O {
            self.advance()?;
            true
        } else {
            false
        };

        let condition = self.optional_condition()?;

        Ok(RollModifier::Reroll { once, condition })
    }

    /// Parse a Marvel Edge modifier (e, e2). Notation always produces RerollLowest.
    fn edge_modifier(&mut self) -> Result<RollModifier> {
        let count = self.marvel_count("edge")?;
        Ok(RollModifier::Edge {
            count,
            policy: EdgePolicy::RerollLowest,
        })
    }

    /// Parse a Marvel Trouble modifier (t, t2).
    fn trouble_modifier(&mut self) -> Result<RollModifier> {
        let count = self.marvel_count("trouble")?;
        Ok(RollModifier::Trouble { count })
    }

    /// Parse a Marvel Edge/Trouble count, defaulting to 1 when omitted.
    ///
    /// An explicit 0 is rejected (the user wrote a no-op), and counts beyond the
    /// reroll limit the roller can execute are rejected as well.
    fn marvel_count(&mut self, label: &'static str) -> Result<u32> {
        let count = self.optional_number(1)?;
        if count == 0 {
            return Err(Error::ZeroMarvelCount(label));
        }
        if count > MAX_REROLLS {
            return Err(Error::RerollLimit(MAX_REROLLS));
        }
        Ok(count)
    }

    /// Parse an optional number, returning default if not present.
    fn optional_number(&mut self, default: u32) -> Result<u32> {
        if let Token::Number(n) = self.current {
            self.advance()?;
            Ok(n)
        } else {
            Ok(default)
        }
    }

    /// Parse a required condition (>=8, <3, =5, etc.) for success counting.
    fn required_condition(&mut self) -> Result<Condition> {
        self.optional_condition()?.ok_or_else(|| Error::Expected {
            expected: "comparison operator (>, <, =, >=, <=)".to_string(),
            found: format!("{:?}", self.current),
        })
    }

    /// Parse an optional condition (=5, <3, >2, etc.).
    fn optional_condition(&mut self) -> Result<Option<Condition>> {
        let compare = match self.current {
            Token::Eq => Compare::Equal,
            Token::Lt => {
                self.advance()?;
                if self.current == Token::Eq {
                    self.advance()?;
                    Compare::LessOrEqual
                } else if self.current == Token::Gt {
                    self.advance()?;
                    Compare::NotEqual
                } else {
                    return self.finish_condition(Compare::LessThan).map(Some);
                }
            }
            Token::Gt => {
                self.advance()?;
                if self.current == Token::Eq {
                    self.advance()?;
                    Compare::GreaterOrEqual
                } else {
                    return self.finish_condition(Compare::GreaterThan).map(Some);
                }
            }
            _ => return Ok(None),
        };

        if compare == Compare::Equal {
            self.advance()?;
        }

        self.finish_condition(compare).map(Some)
    }

    fn finish_condition(&mut self, compare: Compare) -> Result<Condition> {
        if let Token::Number(n) = self.current {
            self.advance()?;
            Ok(Condition {
                compare,
                value: n as i64,
            })
        } else {
            Err(Error::Expected {
                expected: "number after comparison".to_string(),
                found: format!("{:?}", self.current),
            })
        }
    }

    /// Parse critical success/failure markers (cs, cf) into annotation rules.
    fn annotation_rules(&mut self) -> Result<Vec<AnnotationRule>> {
        let mut rules = Vec::new();
        let mut seen_success = false;
        let mut seen_failure = false;

        loop {
            match self.current {
                Token::CritSuccess => {
                    if seen_success {
                        return Err(Error::DuplicateCritMarker("cs".to_string()));
                    }
                    self.advance()?;
                    let cond = self.crit_condition()?;
                    rules.push(AnnotationRule::CriticalSuccess(cond));
                    seen_success = true;
                }
                Token::CritFail => {
                    if seen_failure {
                        return Err(Error::DuplicateCritMarker("cf".to_string()));
                    }
                    self.advance()?;
                    let cond = self.crit_condition()?;
                    rules.push(AnnotationRule::CriticalFailure(cond));
                    seen_failure = true;
                }
                _ => break,
            }
        }

        Ok(rules)
    }

    /// Parse a condition for crit markers. Defaults to Equal if just a number.
    fn crit_condition(&mut self) -> Result<Condition> {
        // If we see a comparison operator, use optional_condition
        if matches!(self.current, Token::Gt | Token::Lt | Token::Eq) {
            self.optional_condition()?.ok_or_else(|| Error::Expected {
                expected: "condition after critical marker".to_string(),
                found: format!("{:?}", self.current),
            })
        } else if let Token::Number(n) = self.current {
            // Just a number means Equal
            self.advance()?;
            Ok(Condition {
                compare: Compare::Equal,
                value: n as i64,
            })
        } else {
            Err(Error::Expected {
                expected: "number or comparison after critical marker".to_string(),
                found: format!("{:?}", self.current),
            })
        }
    }
}

/// Parse a dice notation string into an expression.
pub fn parse(input: &str) -> Result<Expr> {
    Parser::new(input)?.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the lowered `RollPlan` for a digit-dice expression (Dnn).
    fn digit_plan(count: u32, sides: u32) -> Expr {
        Expr::Roll(RollPlan::new_unchecked(
            DicePool {
                count,
                kind: DieKind::Number(sides),
            },
            vec![],
            ScoringMode::DigitConcatenate,
            vec![],
        ))
    }

    #[test]
    fn test_parse_number() {
        let expr = parse("42").unwrap();
        assert_eq!(expr, Expr::Number(42));
    }

    #[test]
    fn test_parse_basic_roll() {
        let expr = parse("2d6").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 2,
                    kind: DieKind::Number(6),
                },
                vec![],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_roll_with_modifier() {
        let expr = parse("4d6kh3").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 4,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::KeepHighest(3)],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_expression() {
        let expr = parse("2d6 + 5").unwrap();
        match expr {
            Expr::BinOp { op, left, right } => {
                assert_eq!(op, Op::Add);
                assert!(matches!(*left, Expr::Roll(_)));
                assert_eq!(*right, Expr::Number(5));
            }
            _ => panic!("Expected BinOp"),
        }
    }

    #[test]
    fn test_parse_explode() {
        let expr = parse("1d6!").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: false,
                    penetrating: false,
                    limit: None,
                    condition: None,
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_explode_condition() {
        let expr = parse("1d6!>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: false,
                    penetrating: false,
                    limit: None,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_penetrating_explode() {
        let expr = parse("1d6!p").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: false,
                    penetrating: true,
                    limit: None,
                    condition: None,
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_penetrating_explode_condition() {
        let expr = parse("1d6!p>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: false,
                    penetrating: true,
                    limit: None,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_compounding_explode() {
        let expr = parse("1d6!!").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: true,
                    penetrating: false,
                    limit: None,
                    condition: None,
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_compounding_penetrating() {
        let expr = parse("1d6!!p").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: true,
                    penetrating: true,
                    limit: None,
                    condition: None,
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_compounding_condition() {
        let expr = parse("1d6!!>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: true,
                    penetrating: false,
                    limit: None,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_compounding_penetrating_condition() {
        let expr = parse("1d6!!p>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::Explode {
                    compounding: true,
                    penetrating: true,
                    limit: None,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_percent() {
        let expr = parse("d%").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Percent,
                },
                vec![],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_fudge() {
        let expr = parse("4dF").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 4,
                    kind: DieKind::Fudge,
                },
                vec![],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_complex() {
        let expr = parse("(2d6 + 3) * 2").unwrap();
        match expr {
            Expr::BinOp { op, left, .. } => {
                assert_eq!(op, Op::Mul);
                assert!(matches!(*left, Expr::Group(_)));
            }
            _ => panic!("Expected BinOp"),
        }
    }

    #[test]
    fn test_parse_drop_lowest() {
        let expr = parse("4d6dl1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 4,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::DropLowest(1)],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_drop_highest() {
        let expr = parse("2d20dh1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 2,
                    kind: DieKind::Number(20),
                },
                vec![RollModifier::DropHighest(1)],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_success_count_gte() {
        let expr = parse("5d10>=8").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 5,
                    kind: DieKind::Number(10),
                },
                vec![],
                ScoringMode::CountSuccesses(Condition {
                    compare: Compare::GreaterOrEqual,
                    value: 8,
                }),
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_success_count_gt() {
        let expr = parse("6d6>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 6,
                    kind: DieKind::Number(6),
                },
                vec![],
                ScoringMode::CountSuccesses(Condition {
                    compare: Compare::GreaterThan,
                    value: 4,
                }),
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_success_count_eq() {
        let expr = parse("8d6=6").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 8,
                    kind: DieKind::Number(6),
                },
                vec![],
                ScoringMode::CountSuccesses(Condition {
                    compare: Compare::Equal,
                    value: 6,
                }),
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_marvel_roll() {
        let expr = parse("3dMarvel").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 3,
                    kind: DieKind::MarvelD6,
                },
                vec![],
                ScoringMode::MarvelMultiverse,
                vec![AnnotationRule::MarvelFantastic],
            ))
        );
    }

    #[test]
    fn test_parse_marvel_roll_case_insensitive() {
        for input in ["3dMarvel", "3dmarvel", "3dMARVEL"] {
            let expr = parse(input).unwrap();
            assert!(matches!(
                &expr,
                Expr::Roll(plan) if plan.pools()[0].count == 3
                    && plan.pools()[0].kind == DieKind::MarvelD6
                    && plan.modifiers().is_empty()
                    && plan.scoring() == &ScoringMode::MarvelMultiverse
                    && plan.annotation_rules() == vec![AnnotationRule::MarvelFantastic]
            ));
        }
    }

    #[test]
    fn test_parse_marvel_rejects_invalid_forms() {
        for input in [
            "2dMarvel",
            "4dMarvel",
            "3dMarvel>=8",
            "3dMarvelkh2",
            "3dMarvelk2",
            "3dMarvelr",
            "3dMarvelro",
            "3dMarvel!",
            "3dMarvel!!",
            "3dMarvel!p",
            "3dMarvelcs6",
            "3dMarve",
            "3dMarvelous",
        ] {
            assert!(parse(input).is_err(), "{input} should be invalid");
        }
    }

    #[test]
    fn test_parse_marvel_edge() {
        let expr = parse("3dMarvele2").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 3,
                    kind: DieKind::MarvelD6,
                },
                vec![RollModifier::Edge {
                    count: 2,
                    policy: EdgePolicy::RerollLowest,
                }],
                ScoringMode::MarvelMultiverse,
                vec![AnnotationRule::MarvelFantastic],
            ))
        );
    }

    #[test]
    fn test_parse_marvel_edge_default_count() {
        let expr = parse("3dMarvele").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 3,
                    kind: DieKind::MarvelD6,
                },
                vec![RollModifier::Edge {
                    count: 1,
                    policy: EdgePolicy::RerollLowest,
                }],
                ScoringMode::MarvelMultiverse,
                vec![AnnotationRule::MarvelFantastic],
            ))
        );
    }

    #[test]
    fn test_parse_marvel_trouble() {
        let expr = parse("3dMarvelt1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 3,
                    kind: DieKind::MarvelD6,
                },
                vec![RollModifier::Trouble { count: 1 }],
                ScoringMode::MarvelMultiverse,
                vec![AnnotationRule::MarvelFantastic],
            ))
        );
    }

    #[test]
    fn test_parse_marvel_edge_trouble_combinations() {
        for input in [
            "3dMarvele1t1",
            "3dMarvelt1e1",
            "3dMarvele2t1",
            "3dMarvelt1e2",
        ] {
            let expr = parse(input).unwrap();
            assert!(
                matches!(
                    &expr,
                    Expr::Roll(plan) if plan.pools()[0].count == 3
                        && plan.pools()[0].kind == DieKind::MarvelD6
                        && plan.scoring() == &ScoringMode::MarvelMultiverse
                        && plan.annotation_rules() == vec![AnnotationRule::MarvelFantastic]
                ),
                "{input} should parse as a Marvel roll with Edge/Trouble modifiers"
            );
        }
    }

    #[test]
    fn test_parse_edge_trouble_rejected_on_non_marvel() {
        for input in ["2d6e1", "4d6t1", "1d20e1"] {
            let err = parse(input).unwrap_err();
            assert!(
                matches!(err, Error::InvalidMarvelRoll(_)),
                "{input} should be rejected with InvalidMarvelRoll, got {err:?}"
            );
        }
    }

    #[test]
    fn test_parse_marvel_edge_zero_rejected() {
        let err = parse("3dMarvele0").unwrap_err();
        assert!(
            matches!(err, Error::ZeroMarvelCount(_)),
            "3dMarvele0 should be rejected, got {err:?}"
        );
    }

    #[test]
    fn test_parse_marvel_trouble_zero_rejected() {
        let err = parse("3dMarvelt0").unwrap_err();
        assert!(
            matches!(err, Error::ZeroMarvelCount(_)),
            "3dMarvelt0 should be rejected, got {err:?}"
        );
    }

    #[test]
    fn test_parse_marvel_count_over_limit_rejected() {
        for input in ["3dMarvele101", "3dMarvelt101"] {
            let err = parse(input).unwrap_err();
            assert!(
                matches!(err, Error::RerollLimit(_)),
                "{input} should be rejected with RerollLimit, got {err:?}"
            );
        }
    }

    #[test]
    fn test_parse_marvel_count_at_limit_accepted() {
        assert!(parse("3dMarvele100").is_ok());
        assert!(parse("3dMarvelt100").is_ok());
    }

    #[test]
    fn test_parse_crit_success() {
        let expr = parse("1d20cs20").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(20),
                },
                vec![],
                ScoringMode::Sum,
                vec![AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 20,
                })],
            ))
        );
    }

    #[test]
    fn test_parse_crit_failure() {
        let expr = parse("1d20cf1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(20),
                },
                vec![],
                ScoringMode::Sum,
                vec![AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                })],
            ))
        );
    }

    #[test]
    fn test_parse_crit_both() {
        let expr = parse("1d20cs20cf1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(20),
                },
                vec![],
                ScoringMode::Sum,
                vec![
                    AnnotationRule::CriticalSuccess(Condition {
                        compare: Compare::Equal,
                        value: 20,
                    }),
                    AnnotationRule::CriticalFailure(Condition {
                        compare: Compare::Equal,
                        value: 1,
                    }),
                ],
            ))
        );
    }

    #[test]
    fn test_parse_crit_with_comparison() {
        let expr = parse("1d20cs>=19cf1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(20),
                },
                vec![],
                ScoringMode::Sum,
                vec![
                    AnnotationRule::CriticalSuccess(Condition {
                        compare: Compare::GreaterOrEqual,
                        value: 19,
                    }),
                    AnnotationRule::CriticalFailure(Condition {
                        compare: Compare::Equal,
                        value: 1,
                    }),
                ],
            ))
        );
    }

    #[test]
    fn test_parse_duplicate_crit_success_error() {
        let err = parse("1d20cs20cs19").unwrap_err();
        assert!(matches!(err, Error::DuplicateCritMarker(_)));
    }

    #[test]
    fn test_parse_duplicate_crit_failure_error() {
        let err = parse("1d20cf1cf2").unwrap_err();
        assert!(matches!(err, Error::DuplicateCritMarker(_)));
    }

    #[test]
    fn test_parse_crit_success_missing_value() {
        let err = parse("1d20cs").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_crit_failure_missing_value() {
        let err = parse("1d20cf").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_crit_with_modifier() {
        let expr = parse("4d6kh3cs6").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 4,
                    kind: DieKind::Number(6),
                },
                vec![RollModifier::KeepHighest(3)],
                ScoringMode::Sum,
                vec![AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 6,
                })],
            ))
        );
    }

    #[test]
    fn test_parse_crit_reversed_order() {
        let expr = parse("1d20cf1cs20").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 1,
                    kind: DieKind::Number(20),
                },
                vec![],
                ScoringMode::Sum,
                vec![
                    AnnotationRule::CriticalFailure(Condition {
                        compare: Compare::Equal,
                        value: 1,
                    }),
                    AnnotationRule::CriticalSuccess(Condition {
                        compare: Compare::Equal,
                        value: 20,
                    }),
                ],
            ))
        );
    }

    #[test]
    fn test_crit_with_success_counting_error() {
        let err = parse("5d10>=8cs10").unwrap_err();
        assert!(matches!(err, Error::CritWithSuccessCounting));
    }

    #[test]
    fn test_crit_failure_with_success_counting_error() {
        let err = parse("5d10>=8cf1").unwrap_err();
        assert!(matches!(err, Error::CritWithSuccessCounting));
    }

    #[test]
    fn test_parse_digit_dice_d66() {
        assert_eq!(parse("D66").unwrap(), digit_plan(2, 6));
    }

    #[test]
    fn test_parse_digit_dice_d666() {
        assert_eq!(parse("D666").unwrap(), digit_plan(3, 6));
    }

    #[test]
    fn test_parse_digit_dice_d44() {
        assert_eq!(parse("D44").unwrap(), digit_plan(2, 4));
    }

    #[test]
    fn test_parse_digit_dice_d88() {
        assert_eq!(parse("D88").unwrap(), digit_plan(2, 8));
    }

    #[test]
    fn test_parse_digit_dice_single_d6() {
        assert_eq!(parse("D6").unwrap(), digit_plan(1, 6));
    }

    #[test]
    fn test_parse_digit_dice_invalid_mixed_digits() {
        let err = parse("D46").unwrap_err();
        assert!(matches!(err, Error::InvalidDigitDice(46)));
    }

    #[test]
    fn test_parse_digit_dice_invalid_d123() {
        let err = parse("D123").unwrap_err();
        assert!(matches!(err, Error::InvalidDigitDice(123)));
    }

    #[test]
    fn test_both_crit_with_success_counting_error() {
        let err = parse("5d10>=8cs10cf1").unwrap_err();
        assert!(matches!(err, Error::CritWithSuccessCounting));
    }

    #[test]
    fn test_parse_empty_input() {
        let err = parse("").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_missing_die_kind() {
        let err = parse("2d").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_unclosed_parenthesis() {
        let err = parse("(2d6 + 3").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_missing_right_operand() {
        let err = parse("2d6 +").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_trailing_garbage_invalid_chars() {
        let err = parse("2d6 abc").unwrap_err();
        assert!(matches!(err, Error::UnexpectedChar(..)));
    }

    #[test]
    fn test_parse_trailing_garbage_valid_token() {
        let err = parse("2d6 3").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    #[test]
    fn test_parse_invalid_character() {
        let err = parse("2d6 @ 3").unwrap_err();
        assert!(matches!(err, Error::UnexpectedChar(..)));
    }

    #[test]
    fn test_parse_bare_d_no_kind() {
        let err = parse("d").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }

    // --- Narrative notation (spec §2.8 disambiguation matrix) ---

    fn narrative_pool(count: u32, die: NarrativeDie) -> DicePool {
        DicePool {
            count,
            kind: DieKind::Narrative(die),
        }
    }

    fn assert_narrative_plan(expr: &Expr, expected_pools: &[DicePool]) {
        match expr {
            Expr::Roll(plan) => {
                assert_eq!(plan.pools(), expected_pools);
                assert!(plan.modifiers().is_empty());
                assert_eq!(plan.scoring(), &ScoringMode::SymbolCancel);
                assert_eq!(
                    plan.annotation_rules(),
                    vec![AnnotationRule::Triumph, AnnotationRule::Despair]
                );
            }
            _ => panic!("expected Expr::Roll, got {expr:?}"),
        }
    }

    #[test]
    fn test_parse_narrative_single_groups() {
        for (input, die) in [
            ("2dAbility", NarrativeDie::Ability),
            ("1dBoost", NarrativeDie::Boost),
            ("1dSetback", NarrativeDie::Setback),
            ("2dDifficulty", NarrativeDie::Difficulty),
            ("1dProficiency", NarrativeDie::Proficiency),
            ("1dChallenge", NarrativeDie::Challenge),
            ("1dForce", NarrativeDie::Force),
        ] {
            let count = input
                .chars()
                .next()
                .unwrap()
                .to_digit(10)
                .expect("test input starts with a digit");
            let expr = parse(input).unwrap_or_else(|e| panic!("{input} should parse: {e}"));
            assert_narrative_plan(&expr, &[narrative_pool(count, die)]);
        }
    }

    #[test]
    fn test_parse_narrative_union() {
        let expr = parse("2dAbility&1dProficiency&2dDifficulty").unwrap();
        assert_narrative_plan(
            &expr,
            &[
                narrative_pool(2, NarrativeDie::Ability),
                narrative_pool(1, NarrativeDie::Proficiency),
                narrative_pool(2, NarrativeDie::Difficulty),
            ],
        );
    }

    #[test]
    fn test_parse_narrative_duplicate_groups_allowed() {
        let expr = parse("2dAbility&1dAbility").unwrap();
        assert_narrative_plan(
            &expr,
            &[
                narrative_pool(2, NarrativeDie::Ability),
                narrative_pool(1, NarrativeDie::Ability),
            ],
        );
        let Expr::Roll(plan) = &expr else {
            panic!("expected Expr::Roll");
        };
        let total_dice: u32 = plan.pools().iter().map(|p| p.count).sum();
        assert_eq!(total_dice, 3);
    }

    #[test]
    fn test_parse_narrative_zero_count_first_group_rejected() {
        let result = parse("0dAbility");
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn test_parse_narrative_zero_count_later_group_rejected() {
        let result = parse("2dAbility&0dBoost");
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn test_parse_narrative_case_insensitive() {
        let expr = parse("2dability&1dPROFICIENCY").unwrap();
        assert_narrative_plan(
            &expr,
            &[
                narrative_pool(2, NarrativeDie::Ability),
                narrative_pool(1, NarrativeDie::Proficiency),
            ],
        );
    }

    #[test]
    fn test_parse_narrative_second_group_non_narrative_errors() {
        let err = parse("2dAbility&3d6").unwrap_err();
        assert!(
            matches!(err, Error::InvalidNarrativeRoll(_)),
            "expected InvalidNarrativeRoll, got {err:?}"
        );
    }

    #[test]
    fn test_parse_narrative_first_group_non_narrative_leaves_ampersand_trailing() {
        let err = parse("3d6&1dAbility").unwrap_err();
        assert!(
            matches!(err, Error::Expected { .. }),
            "expected trailing-garbage Error::Expected, got {err:?}"
        );
    }

    #[test]
    fn test_parse_narrative_keep_modifier_rejected() {
        let err = parse("2dAbilitykh1").unwrap_err();
        assert!(
            matches!(err, Error::InvalidNarrativeRoll(_)),
            "expected InvalidNarrativeRoll, got {err:?}"
        );
    }

    #[test]
    fn test_parse_narrative_success_counting_rejected() {
        let err = parse("2dAbility>=5").unwrap_err();
        assert!(
            matches!(err, Error::InvalidNarrativeRoll(_)),
            "expected InvalidNarrativeRoll, got {err:?}"
        );
    }

    #[test]
    fn test_parse_narrative_crit_marker_rejected() {
        let err = parse("2dAbilitycs5").unwrap_err();
        assert!(
            matches!(err, Error::InvalidNarrativeRoll(_)),
            "expected InvalidNarrativeRoll, got {err:?}"
        );
    }

    #[test]
    fn test_parse_digit_dice_unaffected_by_narrative_notation() {
        assert_eq!(parse("D66").unwrap(), digit_plan(2, 6));
    }

    #[test]
    fn test_parse_marvel_unaffected_by_narrative_notation() {
        let expr = parse("3dmarvel").unwrap();
        assert!(matches!(
            &expr,
            Expr::Roll(plan) if plan.pools()[0].kind == DieKind::MarvelD6
        ));
    }

    // --- End-to-end: roll() over narrative notation ---

    #[test]
    fn test_roll_narrative_union_returns_symbols_outcome() {
        let result = crate::roll("2dAbility&1dDifficulty").unwrap();
        assert!(matches!(result.outcome, crate::RollOutcome::Symbols(_)));
    }

    #[test]
    fn test_roll_narrative_plus_number_errors_non_numeric() {
        let err = crate::roll("2dAbility + 2").unwrap_err();
        assert!(matches!(err, Error::NonNumericOutcome));
    }

    #[test]
    fn test_roll_grouped_narrative_times_number_errors_non_numeric() {
        let err = crate::roll("(1dBoost) * 2").unwrap_err();
        assert!(matches!(err, Error::NonNumericOutcome));
    }
}
