// ABOUTME: Recursive descent parser for dice notation expressions.
// ABOUTME: Converts token streams into an AST.

use crate::ast::{Compare, Condition, Expr, Modifier, Op, Roll, Sides};
use crate::error::{Error, Result};
use crate::lexer::{Lexer, Token};

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

        // Parse the sides
        let sides = self.sides()?;

        // Parse any modifiers
        let modifiers = self.modifiers()?;

        // Parse critical markers (cs and cf can appear in any order)
        let (crit_success, crit_failure) = self.crit_markers()?;

        // Validate: cs/cf cannot combine with success counting
        let has_success_counting = modifiers.iter().any(|m| matches!(m, Modifier::CountSuccesses(_)));
        if has_success_counting && (crit_success.is_some() || crit_failure.is_some()) {
            return Err(Error::CritWithSuccessCounting);
        }

        Ok(Expr::Roll(Roll {
            count,
            sides,
            modifiers,
            crit_success,
            crit_failure,
        }))
    }

    /// Parse dice sides (number, %, or F).
    fn sides(&mut self) -> Result<Sides> {
        match &self.current {
            Token::Number(n) => {
                let n = *n;
                self.advance()?;
                Ok(Sides::Number(n))
            }
            Token::Percent => {
                self.advance()?;
                Ok(Sides::Percent)
            }
            Token::Fudge => {
                self.advance()?;
                Ok(Sides::Fudge)
            }
            _ => Err(Error::Expected {
                expected: "dice sides (number, %, or F)".to_string(),
                found: format!("{:?}", self.current),
            }),
        }
    }

    /// Parse modifiers (keep, drop, explode, reroll).
    fn modifiers(&mut self) -> Result<Vec<Modifier>> {
        let mut modifiers = Vec::new();

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
                    modifiers.push(Modifier::CountSuccesses(condition));
                }
                _ => break,
            }
        }

        Ok(modifiers)
    }

    /// Parse a keep modifier (kh3, kl1, k3).
    fn keep_modifier(&mut self) -> Result<Modifier> {
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
            Ok(Modifier::KeepHighest(count))
        } else {
            Ok(Modifier::KeepLowest(count))
        }
    }

    /// Parse a drop modifier (dh3, dl1).
    fn drop_modifier(&mut self) -> Result<Modifier> {
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
            Ok(Modifier::DropHighest(count))
        } else {
            Ok(Modifier::DropLowest(count))
        }
    }

    /// Parse an explode modifier (!, !!, !p, !!p, !>5, !!p>5).
    fn explode_modifier(&mut self) -> Result<Modifier> {
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

        Ok(Modifier::Explode { compounding, penetrating, condition })
    }

    /// Parse a reroll modifier (r, ro, r<3).
    fn reroll_modifier(&mut self) -> Result<Modifier> {
        let once = if self.current == Token::O {
            self.advance()?;
            true
        } else {
            false
        };

        let condition = self.optional_condition()?;

        Ok(Modifier::Reroll { once, condition })
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

    /// Parse critical success/failure markers (cs, cf).
    fn crit_markers(&mut self) -> Result<(Option<Condition>, Option<Condition>)> {
        let mut crit_success = None;
        let mut crit_failure = None;

        loop {
            match self.current {
                Token::CritSuccess => {
                    if crit_success.is_some() {
                        return Err(Error::DuplicateCritMarker("cs".to_string()));
                    }
                    self.advance()?;
                    crit_success = Some(self.crit_condition()?);
                }
                Token::CritFail => {
                    if crit_failure.is_some() {
                        return Err(Error::DuplicateCritMarker("cf".to_string()));
                    }
                    self.advance()?;
                    crit_failure = Some(self.crit_condition()?);
                }
                _ => break,
            }
        }

        Ok((crit_success, crit_failure))
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
            Expr::Roll(Roll {
                count: 2,
                sides: Sides::Number(6),
                modifiers: vec![],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_roll_with_modifier() {
        let expr = parse("4d6kh3").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 4,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::KeepHighest(3)],
                crit_success: None,
                crit_failure: None,
            })
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
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: false,
                    penetrating: false,
                    condition: None,
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_explode_condition() {
        let expr = parse("1d6!>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: false,
                    penetrating: false,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_penetrating_explode() {
        let expr = parse("1d6!p").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: false,
                    penetrating: true,
                    condition: None,
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_penetrating_explode_condition() {
        let expr = parse("1d6!p>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: false,
                    penetrating: true,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_compounding_explode() {
        let expr = parse("1d6!!").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: true,
                    penetrating: false,
                    condition: None,
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_compounding_penetrating() {
        let expr = parse("1d6!!p").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: true,
                    penetrating: true,
                    condition: None,
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_compounding_condition() {
        let expr = parse("1d6!!>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: true,
                    penetrating: false,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_compounding_penetrating_condition() {
        let expr = parse("1d6!!p>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::Explode {
                    compounding: true,
                    penetrating: true,
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_percent() {
        let expr = parse("d%").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Percent,
                modifiers: vec![],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_fudge() {
        let expr = parse("4dF").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 4,
                sides: Sides::Fudge,
                modifiers: vec![],
                crit_success: None,
                crit_failure: None,
            })
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
            Expr::Roll(Roll {
                count: 4,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::DropLowest(1)],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_drop_highest() {
        let expr = parse("2d20dh1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 2,
                sides: Sides::Number(20),
                modifiers: vec![Modifier::DropHighest(1)],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_success_count_gte() {
        let expr = parse("5d10>=8").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 5,
                sides: Sides::Number(10),
                modifiers: vec![Modifier::CountSuccesses(Condition {
                    compare: Compare::GreaterOrEqual,
                    value: 8,
                })],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_success_count_gt() {
        let expr = parse("6d6>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 6,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::CountSuccesses(Condition {
                    compare: Compare::GreaterThan,
                    value: 4,
                })],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_success_count_eq() {
        let expr = parse("8d6=6").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 8,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::CountSuccesses(Condition {
                    compare: Compare::Equal,
                    value: 6,
                })],
                crit_success: None,
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_crit_success() {
        let expr = parse("1d20cs20").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(20),
                modifiers: vec![],
                crit_success: Some(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_crit_failure() {
        let expr = parse("1d20cf1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(20),
                modifiers: vec![],
                crit_success: None,
                crit_failure: Some(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            })
        );
    }

    #[test]
    fn test_parse_crit_both() {
        let expr = parse("1d20cs20cf1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(20),
                modifiers: vec![],
                crit_success: Some(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                crit_failure: Some(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            })
        );
    }

    #[test]
    fn test_parse_crit_with_comparison() {
        let expr = parse("1d20cs>=19cf1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(20),
                modifiers: vec![],
                crit_success: Some(Condition {
                    compare: Compare::GreaterOrEqual,
                    value: 19,
                }),
                crit_failure: Some(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            })
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
            Expr::Roll(Roll {
                count: 4,
                sides: Sides::Number(6),
                modifiers: vec![Modifier::KeepHighest(3)],
                crit_success: Some(Condition {
                    compare: Compare::Equal,
                    value: 6,
                }),
                crit_failure: None,
            })
        );
    }

    #[test]
    fn test_parse_crit_reversed_order() {
        let expr = parse("1d20cf1cs20").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(Roll {
                count: 1,
                sides: Sides::Number(20),
                modifiers: vec![],
                crit_success: Some(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                crit_failure: Some(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            })
        );
    }

    #[test]
    fn test_crit_with_success_counting_error() {
        let result = parse("5d10>=8cs10");
        assert!(result.is_err());
        // Verify the error is specifically about crit/success incompatibility
        match result {
            Err(Error::CritWithSuccessCounting) => {}
            _ => panic!("Expected CritWithSuccessCounting error"),
        }
    }

    #[test]
    fn test_crit_failure_with_success_counting_error() {
        let result = parse("5d10>=8cf1");
        assert!(result.is_err());
        match result {
            Err(Error::CritWithSuccessCounting) => {}
            _ => panic!("Expected CritWithSuccessCounting error"),
        }
    }

    #[test]
    fn test_both_crit_with_success_counting_error() {
        let result = parse("5d10>=8cs10cf1");
        assert!(result.is_err());
        match result {
            Err(Error::CritWithSuccessCounting) => {}
            _ => panic!("Expected CritWithSuccessCounting error"),
        }
    }
}
