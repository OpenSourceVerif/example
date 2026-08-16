use std::{fmt, ops::Range};

use crate::{DefStore, Environment, INTERNERS, Op, Sort, SortDef, Term, scoped};

use super::Clause;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    Unknown,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    Term,
    RParen,
    End,
    Bool,
    Int,
    SameSort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Unsupported(char),
    IntOverflow,
    Unknown(String),
    Ambiguous(String),
    Inconsistent(String),
    Expected(Expected),
    TooManyBindings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub range: Range<usize>,
    pub kind: ParseErrorKind,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expected as Want;
        use ParseErrorKind as Kind;

        match self {
            Kind::Unsupported(ch) => write!(f, "unsupported character `{ch}`"),
            Kind::IntOverflow => f.write_str("integer literal is outside the i128 range"),
            Kind::Unknown(name) => write!(f, "unknown variable `{name}`"),
            Kind::Ambiguous(name) => write!(f, "variable `{name}` is ambiguous"),
            Kind::Inconsistent(name) => {
                write!(f, "variable `{name}` resolves to inconsistent sorts")
            }
            Kind::Expected(Want::Term) => f.write_str("expected a term"),
            Kind::Expected(Want::RParen) => f.write_str("expected `)`"),
            Kind::Expected(Want::End) => f.write_str("expected the end of the clause"),
            Kind::Expected(Want::Bool) => f.write_str("expected Bool"),
            Kind::Expected(Want::Int) => f.write_str("expected Int"),
            Kind::Expected(Want::SameSort) => {
                f.write_str("comparison operands have different sorts")
            }
            Kind::TooManyBindings => f.write_str("clause has more than u32::MAX bindings"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Ident(&'a str),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Lexed<'a> {
    token: Token<'a>,
    range: Range<usize>,
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn next(&mut self) -> Result<Lexed<'a>, ParseError> {
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }

        let start = self.pos;
        if start == bytes.len() {
            return Ok(Lexed { token: Token::End, range: start..start });
        }

        let rest = &self.text[start..];
        let token = if rest.starts_with("==>") {
            self.pos += 3;
            Token::Implies
        } else if rest.starts_with("&&") {
            self.pos += 2;
            Token::And
        } else if rest.starts_with("||") {
            self.pos += 2;
            Token::Or
        } else if rest.starts_with("==") {
            self.pos += 2;
            Token::Eq
        } else if rest.starts_with("!=") {
            self.pos += 2;
            Token::Ne
        } else if rest.starts_with("<=") {
            self.pos += 2;
            Token::Le
        } else if rest.starts_with(">=") {
            self.pos += 2;
            Token::Ge
        } else {
            match bytes[start] {
                b'(' => self.single(Token::LParen),
                b')' => self.single(Token::RParen),
                b'+' => self.single(Token::Plus),
                b'-' => self.single(Token::Minus),
                b'*' => self.single(Token::Star),
                b'!' => self.single(Token::Bang),
                b'<' => self.single(Token::Lt),
                b'>' => self.single(Token::Gt),
                byte if byte.is_ascii_digit() => {
                    while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                        self.pos += 1;
                    }
                    let value = self.text[start..self.pos].parse().map_err(|_| ParseError {
                        range: start..self.pos,
                        kind: ParseErrorKind::IntOverflow,
                    })?;
                    Token::Int(value)
                }
                byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                    while self.pos < bytes.len()
                        && (bytes[self.pos].is_ascii_alphanumeric() || bytes[self.pos] == b'_')
                    {
                        self.pos += 1;
                    }
                    match &self.text[start..self.pos] {
                        "true" => Token::True,
                        "false" => Token::False,
                        name => Token::Ident(name),
                    }
                }
                _ => {
                    let ch = rest.chars().next().unwrap();
                    return Err(ParseError {
                        range: start..start + ch.len_utf8(),
                        kind: ParseErrorKind::Unsupported(ch),
                    });
                }
            }
        };
        Ok(Lexed { token, range: start..self.pos })
    }

    fn single(&mut self, token: Token<'a>) -> Token<'a> {
        self.pos += 1;
        token
    }
}

struct Parser<'a, B, F> {
    lexer: Lexer<'a>,
    current: Lexed<'a>,
    resolve: F,
    environment: Environment<B>,
}

pub fn parse<B, F>(text: &str, resolve: F) -> Result<Clause<B>, ParseError>
where
    B: Copy + Eq,
    F: FnMut(&str) -> Result<(Sort, B), ResolveError>,
{
    let mut lexer = Lexer { text, pos: 0 };
    let current = lexer.next()?;
    Parser { lexer, current, resolve, environment: Environment::new() }.parse()
}

impl<B, F> Parser<'_, B, F>
where
    B: Copy + Eq,
    F: FnMut(&str) -> Result<(Sort, B), ResolveError>,
{
    fn parse(mut self) -> Result<Clause<B>, ParseError> {
        let term = self.implies()?;
        if self.current.token != Token::End {
            return self.error(Expected::End);
        }
        self.require(term, SortDef::Bool)?;
        Ok(Clause { term, environment: self.environment })
    }

    fn implies(&mut self) -> Result<Term, ParseError> {
        let lhs = self.or()?;
        if self.take(Token::Implies)? {
            self.require(lhs, SortDef::Bool)?;
            let rhs = self.implies()?;
            self.require(rhs, SortDef::Bool)?;
            Ok(self.environment.implies(lhs, rhs))
        } else {
            Ok(lhs)
        }
    }

    fn or(&mut self) -> Result<Term, ParseError> {
        let mut lhs = self.and()?;
        while self.take(Token::Or)? {
            self.require(lhs, SortDef::Bool)?;
            let rhs = self.and()?;
            self.require(rhs, SortDef::Bool)?;
            lhs = self.environment.or(lhs, rhs);
        }
        Ok(lhs)
    }

    fn and(&mut self) -> Result<Term, ParseError> {
        let mut lhs = self.comparison()?;
        while self.take(Token::And)? {
            self.require(lhs, SortDef::Bool)?;
            let rhs = self.comparison()?;
            self.require(rhs, SortDef::Bool)?;
            lhs = self.environment.and(lhs, rhs);
        }
        Ok(lhs)
    }

    fn comparison(&mut self) -> Result<Term, ParseError> {
        let lhs = self.additive()?;
        let op = match self.current.token {
            Token::Eq => Op::Eq,
            Token::Ne => Op::Ne,
            Token::Lt => Op::Lt,
            Token::Le => Op::Le,
            Token::Gt => Op::Gt,
            Token::Ge => Op::Ge,
            _ => return Ok(lhs),
        };
        self.bump()?;
        let rhs = self.additive()?;
        if self.term_sort(lhs) != self.term_sort(rhs) {
            return self.error(Expected::SameSort);
        }
        if !matches!(op, Op::Eq | Op::Ne) {
            self.require(lhs, SortDef::Int)?;
        }
        Ok(self.environment.binary(op, lhs, rhs))
    }

    fn additive(&mut self) -> Result<Term, ParseError> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = match self.current.token {
                Token::Plus => Op::Add,
                Token::Minus => Op::Sub,
                _ => return Ok(lhs),
            };
            self.bump()?;
            self.require(lhs, SortDef::Int)?;
            let rhs = self.multiplicative()?;
            self.require(rhs, SortDef::Int)?;
            lhs = self.environment.binary(op, lhs, rhs);
        }
    }

    fn multiplicative(&mut self) -> Result<Term, ParseError> {
        let mut lhs = self.unary()?;
        while self.take(Token::Star)? {
            self.require(lhs, SortDef::Int)?;
            let rhs = self.unary()?;
            self.require(rhs, SortDef::Int)?;
            lhs = self.environment.mul(lhs, rhs);
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Term, ParseError> {
        if self.take(Token::Bang)? {
            let term = self.unary()?;
            self.require(term, SortDef::Bool)?;
            return Ok(self.environment.not(term));
        }
        if self.take(Token::Minus)? {
            let term = self.unary()?;
            self.require(term, SortDef::Int)?;
            return Ok(self.environment.neg(term));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Term, ParseError> {
        let Lexed { token, range } = self.current.clone();
        match token {
            Token::Int(value) => {
                self.bump()?;
                Ok(self.environment.int(value))
            }
            Token::True | Token::False => {
                self.bump()?;
                Ok(self.environment.bool(token == Token::True))
            }
            Token::Ident(name) => {
                self.bump()?;
                let (sort, binding) = (self.resolve)(name).map_err(|error| ParseError {
                    range: range.clone(),
                    kind: match error {
                        ResolveError::Unknown => ParseErrorKind::Unknown(name.to_owned()),
                        ResolveError::Ambiguous => ParseErrorKind::Ambiguous(name.to_owned()),
                    },
                })?;
                let existing = self
                    .environment
                    .iter()
                    .find(|(_, _, bound)| **bound == binding)
                    .map(|(var, declaration, _)| (var, declaration.clone()));
                let var = match existing {
                    Some((var, crate::Declaration::Value(previous))) if previous == sort => var,
                    Some(_) => {
                        return Err(ParseError {
                            range,
                            kind: ParseErrorKind::Inconsistent(name.to_owned()),
                        });
                    }
                    None => {
                        if self.environment.len() == u32::MAX as usize {
                            return Err(ParseError {
                                range,
                                kind: ParseErrorKind::TooManyBindings,
                            });
                        }
                        self.environment.bind_value(sort, binding)
                    }
                };
                Ok(self.environment.var(var))
            }
            Token::LParen => {
                self.bump()?;
                let term = self.implies()?;
                if !self.take(Token::RParen)? {
                    return self.error(Expected::RParen);
                }
                Ok(term)
            }
            Token::RParen
            | Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Bang
            | Token::And
            | Token::Or
            | Token::Eq
            | Token::Ne
            | Token::Lt
            | Token::Le
            | Token::Gt
            | Token::Ge
            | Token::Implies
            | Token::End => self.error(Expected::Term),
        }
    }

    fn require(&mut self, term: Term, expected: SortDef<'_>) -> Result<(), ParseError> {
        let sort = self.term_sort(term);
        scoped!(let interners = INTERNERS);
        if interners.borrow().get(sort) == expected {
            Ok(())
        } else {
            self.error(match expected {
                SortDef::Bool => Expected::Bool,
                SortDef::Int => Expected::Int,
                SortDef::Tuple(_) => unreachable!(),
            })
        }
    }

    fn term_sort(&self, term: Term) -> Sort {
        self.environment.sort(term).expect("parser only constructs checked terms")
    }

    fn take(&mut self, token: Token<'_>) -> Result<bool, ParseError> {
        if self.current.token == token {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn bump(&mut self) -> Result<(), ParseError> {
        self.current = self.lexer.next()?;
        Ok(())
    }

    fn error<T>(&self, expected: Expected) -> Result<T, ParseError> {
        Err(ParseError {
            range: self.current.range.clone(),
            kind: ParseErrorKind::Expected(expected),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ParseErrorKind, ResolveError, parse};
    use crate::{DefStore, INTERNERS, Intern, Op, SortDef, TermDef, scope, scoped};

    #[test]
    fn parses_directly_into_terms() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let clause = parse("x >= 0 ==> result >= x", |name| match name {
                    "x" => Ok((int, 1)),
                    "result" => Ok((int, 2)),
                    _ => Err(ResolveError::Unknown),
                })
                .unwrap();

                scoped!(let interners = INTERNERS);
                assert!(matches!(
                    interners.borrow().get(clause.term),
                    TermDef::Binary { op: Op::Implies, .. }
                ));
                assert_eq!(
                    clause.environment.iter().map(|(_, _, binding)| *binding).collect::<Vec<_>>(),
                    [1, 2]
                );
            })
        }
    }

    #[test]
    fn renamed_variables_reuse_terms() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let x = parse("x >= 0", |_| Ok((int, 1))).unwrap();
                let value = parse("value >= 0", |_| Ok((int, 2))).unwrap();
                assert_eq!(value.term, x.term);
                assert_ne!(value.environment, x.environment);
            })
        }
    }

    #[test]
    fn reports_errors_by_kind_and_range() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let error =
                    parse::<u8, _>("missing >= 0", |_| Err(ResolveError::Unknown)).unwrap_err();
                assert_eq!(error.range, 0..7);
                assert_eq!(error.kind, ParseErrorKind::Unknown("missing".to_owned()));
            })
        }
    }

    #[test]
    fn rejects_one_binding_at_two_sorts() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let bool = SortDef::Bool.intern();
                let error = parse("x == y", |name| match name {
                    "x" => Ok((int, 1)),
                    "y" => Ok((bool, 1)),
                    _ => Err(ResolveError::Unknown),
                })
                .unwrap_err();
                assert_eq!(error.range, 5..6);
                assert_eq!(error.kind, ParseErrorKind::Inconsistent("y".to_owned()));
            })
        }
    }
}
