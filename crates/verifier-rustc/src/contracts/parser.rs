use rustc_span::Span;
use std::collections::HashMap;

use verifier_core::{Context, Intern, Op, Sort, SortDef, Term};

use super::{Binding, Clause, SpecError};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Int(i128),
    True,
    False,
    LParen,
    RParen,
    Plus,
    Minus,
    Star,
    Bang,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Implies,
    End,
}

struct TermParser<'a> {
    context: &'a mut Context,
    bindings: &'a HashMap<String, Binding>,
    tokens: Vec<Token>,
    position: usize,
    span: Span,
}

impl TermParser<'_> {
    fn parse(mut self) -> Result<Term, SpecError> {
        let (term, sort) = self.parse_implies()?;
        if self.peek() != &Token::End {
            return self.error("unexpected token after specification expression");
        }
        if self.context.get(sort) != SortDef::Bool {
            return self.error("specification clause must have boolean type");
        }
        Ok(term)
    }

    fn parse_implies(&mut self) -> Result<(Term, Sort), SpecError> {
        let (lhs, lhs_sort) = self.parse_or()?;
        if self.take(&Token::Implies) {
            self.require_bool(lhs_sort, "left operand of `==>`")?;
            let (rhs, rhs_sort) = self.parse_implies()?;
            self.require_bool(rhs_sort, "right operand of `==>`")?;
            let sort = self.context.bool_sort();
            Ok((self.context.implies(lhs, rhs), sort))
        } else {
            Ok((lhs, lhs_sort))
        }
    }

    fn parse_or(&mut self) -> Result<(Term, Sort), SpecError> {
        let (mut lhs, sort) = self.parse_and()?;
        while self.take(&Token::Or) {
            self.require_bool(sort, "left operand of `||`")?;
            let (rhs, rhs_sort) = self.parse_and()?;
            self.require_bool(rhs_sort, "right operand of `||`")?;
            lhs = self.context.or(lhs, rhs);
        }
        Ok((lhs, sort))
    }

    fn parse_and(&mut self) -> Result<(Term, Sort), SpecError> {
        let (mut lhs, sort) = self.parse_comparison()?;
        while self.take(&Token::And) {
            self.require_bool(sort, "left operand of `&&`")?;
            let (rhs, rhs_sort) = self.parse_comparison()?;
            self.require_bool(rhs_sort, "right operand of `&&`")?;
            lhs = self.context.and(lhs, rhs);
        }
        Ok((lhs, sort))
    }

    fn parse_comparison(&mut self) -> Result<(Term, Sort), SpecError> {
        let (lhs, lhs_sort) = self.parse_additive()?;
        let operation = match self.peek() {
            Token::Eq => Some(Op::Eq),
            Token::Ne => Some(Op::Ne),
            Token::Lt => Some(Op::Lt),
            Token::Le => Some(Op::Le),
            Token::Gt => Some(Op::Gt),
            Token::Ge => Some(Op::Ge),
            _ => None,
        };
        let Some(operation) = operation else { return Ok((lhs, lhs_sort)) };
        self.position += 1;
        let (rhs, rhs_sort) = self.parse_additive()?;
        if lhs_sort != rhs_sort {
            return self.error("comparison operands have different types");
        }
        if !matches!(operation, Op::Eq | Op::Ne) {
            self.require_int(lhs_sort, "ordered comparison operand")?;
        }
        let sort = self.context.bool_sort();
        Ok((self.context.binary(operation, lhs, rhs), sort))
    }

    fn parse_additive(&mut self) -> Result<(Term, Sort), SpecError> {
        let (mut lhs, sort) = self.parse_multiplicative()?;
        loop {
            let operation = match self.peek() {
                Token::Plus => Op::Add,
                Token::Minus => Op::Sub,
                _ => break,
            };
            self.position += 1;
            self.require_int(sort, "arithmetic operand")?;
            let (rhs, rhs_sort) = self.parse_multiplicative()?;
            self.require_int(rhs_sort, "arithmetic operand")?;
            lhs = self.context.binary(operation, lhs, rhs);
        }
        Ok((lhs, sort))
    }

    fn parse_multiplicative(&mut self) -> Result<(Term, Sort), SpecError> {
        let (mut lhs, sort) = self.parse_unary()?;
        while self.take(&Token::Star) {
            self.require_int(sort, "multiplication operand")?;
            let (rhs, rhs_sort) = self.parse_unary()?;
            self.require_int(rhs_sort, "multiplication operand")?;
            lhs = self.context.mul(lhs, rhs);
        }
        Ok((lhs, sort))
    }

    fn parse_unary(&mut self) -> Result<(Term, Sort), SpecError> {
        if self.take(&Token::Bang) {
            let (operand, sort) = self.parse_unary()?;
            self.require_bool(sort, "operand of `!`")?;
            return Ok((self.context.not(operand), sort));
        }
        if self.take(&Token::Minus) {
            let (operand, sort) = self.parse_unary()?;
            self.require_int(sort, "operand of unary `-`")?;
            return Ok((self.context.neg(operand), sort));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<(Term, Sort), SpecError> {
        match self.next().clone() {
            Token::Int(value) => {
                let sort = self.context.int_sort();
                Ok((self.context.int_lit(value), sort))
            }
            Token::True => {
                let sort = self.context.bool_sort();
                Ok((self.context.bool_lit(true), sort))
            }
            Token::False => {
                let sort = self.context.bool_sort();
                Ok((self.context.bool_lit(false), sort))
            }
            Token::Ident(name) => {
                let binding = self.bindings.get(&name).copied().ok_or_else(|| SpecError {
                    span: self.span,
                    message: format!("unknown specification variable `{name}`"),
                })?;
                if binding.ambiguous {
                    return self.error(format!(
                        "specification variable `{name}` is shadowed or ambiguous"
                    ));
                }
                let symbol = self.context.symbol(&name, binding.sort);
                Ok((self.context.sym(symbol), binding.sort))
            }
            Token::LParen => {
                let expression = self.parse_implies()?;
                if !self.take(&Token::RParen) {
                    return self.error("expected `)`");
                }
                Ok(expression)
            }
            token => self.error(format!("expected specification expression, found {token:?}")),
        }
    }

    fn require_bool(&self, sort: Sort, what: &str) -> Result<(), SpecError> {
        if self.context.get(sort) == SortDef::Bool {
            Ok(())
        } else {
            self.error(format!("{what} must have boolean type"))
        }
    }

    fn require_int(&self, sort: Sort, what: &str) -> Result<(), SpecError> {
        if self.context.get(sort) == SortDef::Int {
            Ok(())
        } else {
            self.error(format!("{what} must have integer type"))
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn next(&mut self) -> &Token {
        let token = &self.tokens[self.position];
        self.position += 1;
        token
    }

    fn take(&mut self, expected: &Token) -> bool {
        if self.peek() == expected {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, SpecError> {
        Err(SpecError { span: self.span, message: message.into() })
    }
}

pub(super) fn parse_clause(
    context: &mut Context,
    bindings: &HashMap<String, Binding>,
    expression: &str,
    span: Span,
) -> Result<Clause, SpecError> {
    let tokens = lex(expression, span)?;
    let term = TermParser { context, bindings, tokens, position: 0, span }.parse()?;
    Ok(Clause { term, span })
}

fn lex(input: &str, span: Span) -> Result<Vec<Token>, SpecError> {
    let bytes = input.as_bytes();
    let mut position = 0;
    let mut tokens = Vec::new();
    while position < bytes.len() {
        if bytes[position].is_ascii_whitespace() {
            position += 1;
            continue;
        }
        let remaining = &input[position..];
        let (token, length) = if remaining.starts_with("==>") {
            (Token::Implies, 3)
        } else if remaining.starts_with("&&") {
            (Token::And, 2)
        } else if remaining.starts_with("||") {
            (Token::Or, 2)
        } else if remaining.starts_with("==") {
            (Token::Eq, 2)
        } else if remaining.starts_with("!=") {
            (Token::Ne, 2)
        } else if remaining.starts_with("<=") {
            (Token::Le, 2)
        } else if remaining.starts_with(">=") {
            (Token::Ge, 2)
        } else {
            match bytes[position] {
                b'(' => (Token::LParen, 1),
                b')' => (Token::RParen, 1),
                b'+' => (Token::Plus, 1),
                b'-' => (Token::Minus, 1),
                b'*' => (Token::Star, 1),
                b'!' => (Token::Bang, 1),
                b'<' => (Token::Lt, 1),
                b'>' => (Token::Gt, 1),
                byte if byte.is_ascii_digit() => {
                    let start = position;
                    while position < bytes.len() && bytes[position].is_ascii_digit() {
                        position += 1;
                    }
                    let value = input[start..position].parse().map_err(|_| SpecError {
                        span,
                        message: "integer literal is outside the supported i128 range".to_owned(),
                    })?;
                    tokens.push(Token::Int(value));
                    continue;
                }
                byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                    let start = position;
                    while position < bytes.len()
                        && (bytes[position].is_ascii_alphanumeric() || bytes[position] == b'_')
                    {
                        position += 1;
                    }
                    let word = &input[start..position];
                    tokens.push(match word {
                        "true" => Token::True,
                        "false" => Token::False,
                        _ => Token::Ident(word.to_owned()),
                    });
                    continue;
                }
                _ => {
                    let unsupported = input[position..].chars().next().unwrap();
                    return Err(SpecError {
                        span,
                        message: format!("unsupported character in specification: `{unsupported}`"),
                    });
                }
            }
        };
        tokens.push(token);
        position += length;
    }
    tokens.push(Token::End);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{TermParser, lex};
    use crate::contracts::Binding;
    use rustc_span::DUMMY_SP;
    use std::collections::HashMap;
    use verifier_core::{Context, Intern, Op, SortDef, TermDef};

    #[test]
    fn parses_directly_into_terms() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bindings = HashMap::from([
            ("x".to_owned(), Binding { sort: int, local: None, ambiguous: false }),
            ("result".to_owned(), Binding { sort: int, local: None, ambiguous: false }),
        ]);
        let tokens = lex("x >= 0 ==> result >= x", DUMMY_SP).unwrap();
        let term = TermParser {
            context: &mut context,
            bindings: &bindings,
            tokens,
            position: 0,
            span: DUMMY_SP,
        }
        .parse()
        .unwrap();

        assert!(matches!(context.get(term), TermDef::Binary { op: Op::Implies, .. }));
        assert_eq!(context.get(int), SortDef::Int);
    }
}
