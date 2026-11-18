//! Parser for in-place expressions (`{{ a + b = c }}`).
//!
//! ```text
//! relation := sum ( '=' sum )?
//! sum      := product ( ('+' | '-') product )*
//! product  := unary ( ('*' | '/') unary )*
//! unary    := '-' unary | power
//! power    := atom ( '^' unary )?
//! atom     := IDENT '(' ( sum ( ',' sum )* )? ')' | IDENT | NUMBER | '(' relation ')'
//! ```
//!
//! Whitespace between tokens is optional and carries no meaning. Parentheses that are written
//! are kept (`Expr::paren`), whether or not the meaning needs them; other parentheses are
//! added by the precedence table of `Expr` only where the meaning needs them.

use crate::diagnostic::{DiagnosticKind, Problem};
use crate::expr::Expr;
use crate::name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ident,
    Number,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Comma,
    Equals,
    LParen,
    RParen,
}

#[derive(Debug, Clone, Copy)]
struct Token<'a> {
    kind: Kind,
    text: &'a str,
    offset: usize,
}

fn undefined_error(token: Token<'_>) -> Problem {
    Problem {
        offset: token.offset,
        kind: DiagnosticKind::UndefinedIdentifier {
            name: token.text.to_string(),
        },
    }
}

fn syntax_error(offset: usize, message: String) -> Problem {
    Problem::syntax(offset, message)
}

/// Splits `input` into tokens. Characters that cannot start a token are skipped; the first
/// one is returned as an error (the remaining tokens are still needed to resolve identifiers).
fn tokenize(input: &str) -> (Vec<Token<'_>>, Option<Problem>) {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut error = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let start = i;
        let kind = match b {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
                continue;
            }
            b'+' => Kind::Plus,
            b'-' => Kind::Minus,
            b'*' => Kind::Star,
            b'/' => Kind::Slash,
            b'^' => Kind::Caret,
            b',' => Kind::Comma,
            b'=' => Kind::Equals,
            b'(' => Kind::LParen,
            b')' => Kind::RParen,
            b if name::is_start(b) => {
                while i + 1 < bytes.len() && name::is_continue(bytes[i + 1]) {
                    i += 1;
                }
                Kind::Ident
            }
            b'0'..=b'9' => {
                while i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                    i += 1;
                }
                if i + 2 < bytes.len() && bytes[i + 1] == b'.' && bytes[i + 2].is_ascii_digit() {
                    i += 2;
                    while i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                        i += 1;
                    }
                }
                Kind::Number
            }
            _ => {
                let c = input[i..].chars().next().unwrap_or(' ');
                let hint = if c == '\\' {
                    " (raw LaTeX is not allowed in expressions; define a symbol instead)"
                } else {
                    ""
                };
                error.get_or_insert_with(|| {
                    syntax_error(i, format!("unexpected character `{c}`{hint}"))
                });
                i += c.len_utf8();
                continue;
            }
        };
        i += 1;
        tokens.push(Token {
            kind,
            text: &input[start..i],
            offset: start,
        });
    }
    (tokens, error)
}

/// An operator: called with its arguments, once their number has been checked.
pub(crate) type OperatorFn = std::rc::Rc<dyn Fn(&[Expr]) -> Expr>;

/// What a name refers to.
pub(crate) enum Resolved {
    Value(Expr),
    Operator { arity: usize, apply: OperatorFn },
    Undefined,
}

/// The names a template may use. `resolve` is also what marks a name as used.
pub(crate) trait Env {
    fn resolve(&mut self, name: &str) -> Resolved;
}

fn dummy() -> Expr {
    // Placeholder so that parsing continues and finds further problems.
    Expr::integer(0)
}

struct Parser<'a, 'l> {
    tokens: Vec<Token<'a>>,
    pos: usize,
    end_offset: usize,
    env: &'l mut dyn Env,
    /// Problems that do not stop the parser (undefined names, wrong use of names).
    reported: Vec<Problem>,
}

type Parsed = Result<Expr, Problem>;

impl<'a> Parser<'a, '_> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).copied()
    }

    fn peek_kind(&self) -> Option<Kind> {
        self.peek().map(|t| t.kind)
    }

    fn describe_next(&self) -> String {
        self.peek()
            .map_or("end of expression".to_string(), |t| format!("`{}`", t.text))
    }

    fn offset_of_next(&self) -> usize {
        self.peek().map_or(self.end_offset, |t| t.offset)
    }

    fn relation(&mut self) -> Parsed {
        let left = self.sum()?;
        if self.peek_kind() == Some(Kind::Equals) {
            self.pos += 1;
            let right = self.sum()?;
            return Ok(Expr::relation(left, right));
        }
        Ok(left)
    }

    fn sum(&mut self) -> Parsed {
        let mut left = self.product()?;
        while let Some(kind @ (Kind::Plus | Kind::Minus)) = self.peek_kind() {
            self.pos += 1;
            let right = self.product()?;
            left = if kind == Kind::Plus {
                left + right
            } else {
                left - right
            };
        }
        Ok(left)
    }

    fn product(&mut self) -> Parsed {
        let mut left = self.unary()?;
        while let Some(kind @ (Kind::Star | Kind::Slash)) = self.peek_kind() {
            self.pos += 1;
            let right = self.unary()?;
            left = if kind == Kind::Star {
                left * right
            } else {
                Expr::fraction(left, right)
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Parsed {
        if self.peek_kind() == Some(Kind::Minus) {
            self.pos += 1;
            return Ok(-self.unary()?);
        }
        self.power()
    }

    fn power(&mut self) -> Parsed {
        let base = self.atom()?;
        if self.peek_kind() == Some(Kind::Caret) {
            self.pos += 1;
            let exponent = self.unary()?;
            return Ok(base.pow(exponent));
        }
        Ok(base)
    }

    /// `name` has been consumed; the next token is `(`.
    fn call(&mut self, name: Token<'a>) -> Parsed {
        // Resolve first, so that the name counts as used even if the arguments are malformed.
        let resolved = self.env.resolve(name.text);
        self.pos += 1;
        let mut args = Vec::new();
        if self.peek_kind() == Some(Kind::RParen) {
            self.pos += 1;
        } else {
            loop {
                args.push(self.sum()?);
                match self.peek_kind() {
                    Some(Kind::Comma) => self.pos += 1,
                    Some(Kind::RParen) => {
                        self.pos += 1;
                        break;
                    }
                    _ => {
                        return Err(syntax_error(
                            self.offset_of_next(),
                            format!("expected `,` or `)`, found {}", self.describe_next()),
                        ));
                    }
                }
            }
        }
        Ok(match resolved {
            Resolved::Operator { arity, apply } if arity == args.len() => apply(&args),
            Resolved::Operator { arity, .. } => {
                self.reported.push(Problem {
                    offset: name.offset,
                    kind: DiagnosticKind::ArityMismatch {
                        name: name.text.to_string(),
                        expected: arity,
                        found: args.len(),
                    },
                });
                dummy()
            }
            Resolved::Value(_) => {
                self.reported.push(Problem::syntax(
                    name.offset,
                    format!("`{}` is not an operator", name.text),
                ));
                dummy()
            }
            Resolved::Undefined => {
                self.reported.push(undefined_error(name));
                dummy()
            }
        })
    }

    fn atom(&mut self) -> Parsed {
        let Some(token) = self.peek() else {
            return Err(syntax_error(
                self.end_offset,
                "expected an identifier, a number or `(`, found end of expression".to_string(),
            ));
        };
        match token.kind {
            Kind::Ident => {
                self.pos += 1;
                if self.peek_kind() == Some(Kind::LParen) {
                    return self.call(token);
                }
                Ok(match self.env.resolve(token.text) {
                    Resolved::Value(expr) => expr,
                    Resolved::Operator { .. } => {
                        self.reported.push(Problem::syntax(
                            token.offset,
                            format!("`{0}` is an operator; write `{0}(...)`", token.text),
                        ));
                        dummy()
                    }
                    Resolved::Undefined => {
                        self.reported.push(undefined_error(token));
                        dummy()
                    }
                })
            }
            Kind::Number => {
                self.pos += 1;
                Expr::number(token.text).map_err(|message| syntax_error(token.offset, message))
            }
            Kind::LParen => {
                self.pos += 1;
                let body = self.relation()?;
                if self.peek_kind() == Some(Kind::RParen) {
                    self.pos += 1;
                    Ok(Expr::paren(body))
                } else {
                    Err(syntax_error(
                        self.offset_of_next(),
                        format!("expected `)`, found {}", self.describe_next()),
                    ))
                }
            }
            _ => Err(syntax_error(
                token.offset,
                format!(
                    "expected an identifier, a number or `(`, found `{}`",
                    token.text
                ),
            )),
        }
    }
}

/// Parses `input`. Names are resolved with `env`. Every undefined identifier is
/// reported; parsing stops at the first syntax error, but the identifiers after it are still
/// looked up (so that they count as used and are checked).
pub(crate) fn parse(input: &str, env: &mut dyn Env) -> Result<Expr, Vec<Problem>> {
    let (tokens, lexical_error) = tokenize(input);
    let mut parser = Parser {
        tokens,
        pos: 0,
        end_offset: input.len(),
        env,
        reported: Vec::new(),
    };
    let lexical = lexical_error.is_some();
    let (result, first_unresolved) = match lexical_error {
        // Nothing was parsed: every identifier is still unresolved.
        Some(error) => (Err(error), 0),
        None => {
            let result = parser.relation().and_then(|expr| match parser.peek() {
                None => Ok(expr),
                Some(token) => Err(syntax_error(
                    token.offset,
                    match token.kind {
                        Kind::RParen => "unmatched `)`".to_string(),
                        Kind::Equals => "only one `=` is allowed in an expression".to_string(),
                        _ => format!("expected an operator, found `{}`", token.text),
                    },
                )),
            });
            (result, parser.pos)
        }
    };
    let expr = match result {
        Ok(expr) => Some(expr),
        Err(error) => {
            for i in first_unresolved..parser.tokens.len() {
                let token = parser.tokens[i];
                // After a lexical error the text is probably not meant as names at all
                // (e.g. raw LaTeX), so unknown names are not reported then.
                if token.kind == Kind::Ident
                    && matches!(parser.env.resolve(token.text), Resolved::Undefined)
                    && !lexical
                {
                    parser.reported.push(undefined_error(token));
                }
            }
            parser.reported.push(error);
            None
        }
    };
    match expr {
        Some(expr) if parser.reported.is_empty() => Ok(expr),
        _ => {
            let mut errors = parser.reported;
            errors.sort_by_key(|e| e.offset);
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Symbol;

    fn undefined(name: &str) -> DiagnosticKind {
        DiagnosticKind::UndefinedIdentifier { name: name.into() }
    }

    fn arity(name: &str, expected: usize, found: usize) -> DiagnosticKind {
        DiagnosticKind::ArityMismatch {
            name: name.into(),
            expected,
            found,
        }
    }

    fn syntax(message: &str) -> DiagnosticKind {
        DiagnosticKind::Syntax {
            message: message.into(),
        }
    }

    fn sym(name: &str) -> Expr {
        Expr::from_symbol(Symbol::new(name, name))
    }

    /// Names: values `a`, `b`, `c`; operators `neg` (1 argument) and `pair` (2 arguments).
    #[derive(Default)]
    struct TestEnv {
        resolved: Vec<String>,
    }

    impl Env for TestEnv {
        fn resolve(&mut self, name: &str) -> Resolved {
            self.resolved.push(name.to_string());
            match name {
                "a" | "b" | "c" => Resolved::Value(sym(name)),
                "neg" => Resolved::Operator {
                    arity: 1,
                    apply: std::rc::Rc::new(|args| -&args[0]),
                },
                "pair" => Resolved::Operator {
                    arity: 2,
                    apply: std::rc::Rc::new(|args| Expr::fraction(&args[0], &args[1])),
                },
                _ => Resolved::Undefined,
            }
        }
    }

    fn ok(input: &str) -> String {
        parse(input, &mut TestEnv::default()).unwrap().to_string()
    }

    /// (byte offset, kind) of every problem.
    fn errors(input: &str) -> Vec<(usize, DiagnosticKind)> {
        errors_with(input, &mut TestEnv::default())
    }

    fn errors_with(input: &str, env: &mut TestEnv) -> Vec<(usize, DiagnosticKind)> {
        parse(input, env)
            .unwrap_err()
            .into_iter()
            .map(|p| (p.offset, p.kind))
            .collect()
    }

    fn first_error(input: &str) -> (usize, DiagnosticKind) {
        errors(input).remove(0)
    }

    #[test]
    fn precedence_and_associativity() {
        let (a, b, c) = (sym("a"), sym("b"), sym("c"));
        assert_eq!(ok("a - (b + c)"), (&a - (&b + &c)).to_string());
        assert_eq!(ok("a - b - c"), ((&a - &b) - &c).to_string());
        assert_eq!(ok("a + b * c"), (&a + (&b * &c)).to_string());
        assert_eq!(ok("(a + b) * c"), ((&a + &b) * &c).to_string());
        assert_eq!(ok("a * b / c"), Expr::fraction(&a * &b, &c).to_string());
        assert_eq!(
            ok("a / b / c"),
            Expr::fraction(Expr::fraction(&a, &b), &c).to_string()
        );
    }

    #[test]
    fn unary_minus_binds_looser_than_power() {
        let (a, b) = (sym("a"), sym("b"));
        let two = Expr::integer(2);
        assert_eq!(ok("-a^2"), (-a.pow(&two)).to_string());
        assert_eq!(ok("a^b^2"), a.pow(b.pow(&two)).to_string());
        assert_eq!(ok("a^-b"), a.pow(-&b).to_string());
    }

    #[test]
    fn a_negation_after_an_operator_is_a_negated_operand() {
        let (a, b) = (sym("a"), sym("b"));
        assert_eq!(ok("a + -b"), (&a + -&b).to_string());
        assert_eq!(ok("a - -b"), (&a - -&b).to_string());
        assert_eq!(ok("a * -b"), (&a * -&b).to_string());
        assert_eq!(ok("-a + b"), (-&a + &b).to_string());
    }

    #[test]
    fn parentheses_that_are_written_are_kept() {
        // Even if the meaning would not need them.
        assert_eq!(ok("a + (-b)"), r"{a} + \left( - {b} \right)");
        assert_eq!(ok("a + (b + c)"), r"{a} + \left( {b} + {c} \right)");
        assert_eq!(ok("(a * b) + c"), r"\left( {a} {b} \right) + {c}");
        assert_eq!(ok("(a)"), r"\left( {a} \right)");
        assert_eq!(ok("((a))"), r"\left( \left( {a} \right) \right)");
        // Without parentheses nothing is added that the meaning does not need.
        assert_eq!(ok("a + b * c"), r"{a} + {b} {c}");
        assert_eq!(ok("a + -b"), "{a} - {b}");
    }

    #[test]
    fn a_number_after_a_factor_keeps_the_product_visible() {
        assert_eq!(ok("a * 2"), r"{a} \cdot 2");
        assert_eq!(ok("2 * a"), "2 {a}");
        assert_eq!(ok("2 * 3"), r"2 \cdot 3");
    }

    #[test]
    fn relation_and_numbers() {
        let (a, b) = (sym("a"), sym("b"));
        let half = Expr::number("0.5").unwrap();
        assert_eq!(
            ok("a + b = 0.5"),
            Expr::relation(&a + &b, &half).to_string()
        );
    }

    #[test]
    fn whitespace_is_optional_and_may_span_lines() {
        assert_eq!(ok("a-b*c"), ok("a - b * c"));
        assert_eq!(ok("a\n  +\n  b"), ok("a + b"));
    }

    #[test]
    fn undefined_identifiers_are_all_reported() {
        assert_eq!(
            errors("a + yy * (zz)"),
            vec![(4, undefined("yy")), (10, undefined("zz"))]
        );
    }

    #[test]
    fn syntax_errors() {
        let found = |what: &str| {
            syntax(&format!(
                "expected an identifier, a number or `(`, found {what}"
            ))
        };
        assert_eq!(first_error("a +"), (3, found("end of expression")));
        assert_eq!(first_error("a * * b"), (4, found("`*`")));
        assert_eq!(
            first_error("a b"),
            (2, syntax("expected an operator, found `b`"))
        );
        assert_eq!(
            first_error("2a"),
            (1, syntax("expected an operator, found `a`"))
        );
        assert_eq!(
            first_error("(a + b"),
            (6, syntax("expected `)`, found end of expression"))
        );
        assert_eq!(first_error("a)"), (1, syntax("unmatched `)`")));
        assert_eq!(
            first_error("a = b = c"),
            (6, syntax("only one `=` is allowed in an expression"))
        );
    }

    #[test]
    fn invalid_characters() {
        assert!(matches!(
            first_error(r"\alpha").1,
            DiagnosticKind::Syntax { message } if message.contains("raw LaTeX")
        ));
        assert_eq!(first_error("A"), (0, syntax("unexpected character `A`")));
        assert_eq!(first_error("1."), (1, syntax("unexpected character `.`")));
    }

    #[test]
    fn names_after_an_error_are_still_resolved() {
        let mut env = TestEnv::default();
        // Syntax error at `b`; later names are resolved (used), unknown ones are reported.
        let e = errors_with("a b c zz", &mut env);
        assert_eq!(
            e,
            vec![
                (2, syntax("expected an operator, found `b`")),
                (6, undefined("zz"))
            ]
        );
        assert!(env.resolved.contains(&"c".to_string()));
        // After a lexical error they are resolved too, but unknown ones are not reported.
        let mut env = TestEnv::default();
        let e = errors_with(r"a \ c zz", &mut env);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(env.resolved.contains(&"c".to_string()));
    }

    #[test]
    fn operator_calls() {
        let (a, b) = (sym("a"), sym("b"));
        assert_eq!(ok("neg(a)"), (-&a).to_string());
        assert_eq!(ok("neg( a )"), ok("neg(a)"));
        assert_eq!(ok("pair(a, b)"), Expr::fraction(&a, &b).to_string());
        // Arguments are expressions, and a call is an atom of larger expressions.
        assert_eq!(
            ok("pair(a + b, neg(a) * b)"),
            Expr::fraction(&a + &b, -&a * &b).to_string()
        );
        assert_eq!(
            ok("2 * neg(a) + b"),
            (Expr::integer(2) * -&a + &b).to_string()
        );
    }

    #[test]
    fn wrong_argument_count_is_reported_at_the_operator_name() {
        assert_eq!(errors("pair(a)"), vec![(0, arity("pair", 2, 1))]);
        assert_eq!(errors("neg(a, b)"), vec![(0, arity("neg", 1, 2))]);
        assert_eq!(errors("neg()"), vec![(0, arity("neg", 1, 0))]);
        // Problems in arguments and in the call itself are all reported.
        assert_eq!(
            errors("pair(zz, neg(a, b))"),
            vec![(5, undefined("zz")), (9, arity("neg", 1, 2))]
        );
    }

    #[test]
    fn misuse_of_names() {
        assert_eq!(errors("a(b)"), vec![(0, syntax("`a` is not an operator"))]);
        assert_eq!(
            errors("neg"),
            vec![(0, syntax("`neg` is an operator; write `neg(...)`"))]
        );
        assert_eq!(errors("nope(a)"), vec![(0, undefined("nope"))]);
    }

    #[test]
    fn malformed_argument_lists() {
        assert_eq!(
            first_error("pair(a b)"),
            (7, syntax("expected `,` or `)`, found `b`"))
        );
        assert_eq!(
            first_error("pair(a, )"),
            (
                8,
                syntax("expected an identifier, a number or `(`, found `)`")
            )
        );
        assert_eq!(
            first_error("pair(a, b"),
            (9, syntax("expected `,` or `)`, found end of expression"))
        );
        // The operator is resolved before its arguments, so it still counts as used.
        let mut env = TestEnv::default();
        errors_with("pair(a b)", &mut env);
        assert!(env.resolved.contains(&"pair".to_string()));
    }
}
