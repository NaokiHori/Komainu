//! How an expression is written as LaTeX: the precedence table, where parentheses go, and how
//! negations are written (`a + (-b)` as `a - b`, `(-a) b` as `- a b`).

use super::{BigOperatorKind, Delimiter, DerivativeForm, DerivativeKind, Expr, ExprKind};

/// Binding strength of each node; a larger value binds tighter.
/// Operands are parenthesized when their precedence is below what the position requires.
mod precedence {
    pub const RELATION: u8 = 0;
    pub const ADDITIVE: u8 = 1;
    /// Unary minus.
    pub const PREFIX: u8 = 2;
    /// Integrals, sums and products: they extend to the right (an integral up to `d<var>`,
    /// a sum or a product over the factors that follow). Fine as a term of a sum
    /// (`a - \int f dx`, `a + \sum_k b_k`), but as a factor they are parenthesized.
    pub const BIG_OPERATOR: u8 = 3;
    pub const MULTIPLICATIVE: u8 = 4;
    pub const POWER: u8 = 5;
    pub const ATOM: u8 = 6;
}

impl Expr {
    fn precedence(&self) -> u8 {
        match self.kind() {
            // Both are written at the top of what they sit in (an index, a whole equation);
            // as an operand of anything they are parenthesized.
            ExprKind::Relation(_, _) | ExprKind::List(_) => precedence::RELATION,
            ExprKind::Add(_, _) | ExprKind::Sub(_, _) => precedence::ADDITIVE,
            ExprKind::Named { expr, .. } => expr.precedence(),
            ExprKind::Neg(_) => precedence::PREFIX,
            ExprKind::Integral { .. } | ExprKind::BigOperator { .. } => precedence::BIG_OPERATOR,
            // `(-x) r` is written as `-(x r)`, so it is a negation as far as parentheses go.
            ExprKind::Mul(_, _) if self.negated().is_some() => precedence::PREFIX,
            ExprKind::Mul(_, _)
            | ExprKind::Derivative {
                form: DerivativeForm::Operator,
                ..
            } => precedence::MULTIPLICATIVE,
            // A label on a base is written like a power: it is not an atom as the base of one.
            ExprKind::Power { .. } | ExprKind::Superscript { .. } => precedence::POWER,
            ExprKind::Fraction { .. }
            | ExprKind::Function { .. }
            | ExprKind::Number(_)
            | ExprKind::Derivative {
                form: DerivativeForm::Fraction,
                ..
            }
            | ExprKind::Sqrt(_)
            | ExprKind::Delimited { .. }
            | ExprKind::Subscript { .. }
            | ExprKind::Symbol(_) => precedence::ATOM,
        }
    }

    /// Renders `self`, wrapping it in parentheses if it binds weaker than `min`.
    fn render(&self, min: u8) -> String {
        if self.precedence() < min {
            format!(r"\left( {self} \right)")
        } else {
            self.to_string()
        }
    }

    /// Whether what is written first is a number: the number itself, or something that starts
    /// with one (`2^3`, `2 x`).
    fn starts_with_number(&self) -> bool {
        match self.kind() {
            ExprKind::Number(_) => true,
            ExprKind::Named { expr, .. } => expr.starts_with_number(),
            ExprKind::Mul(left, _) => left.starts_with_number(),
            ExprKind::Power { base, .. }
            | ExprKind::Subscript { base, .. }
            | ExprKind::Superscript { base, .. } => base.starts_with_number(),
            _ => false,
        }
    }

    /// If `self` is a negation `-x` (looking through names), or a product whose first factor
    /// is one (`(-x) r`, which is `-(x r)`), returns `x` (or `x r`).
    ///
    /// Such expressions are written with the sign in front (`- x r`), so every rule about
    /// negations (parentheses as an operand, `a + (-b)` as `a - b`) applies to them as well.
    fn negated(&self) -> Option<Expr> {
        match self.kind() {
            ExprKind::Named { expr, .. } => expr.negated(),
            ExprKind::Neg(operand) => Some(operand.clone()),
            ExprKind::Mul(left, right) => left
                .negated()
                .map(|left| Self(std::rc::Rc::new(ExprKind::Mul(left, right.clone())))),
            _ => None,
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        use precedence::*;
        match self.kind() {
            ExprKind::Relation(l, r) => {
                write!(f, "{} = {}", l.render(ADDITIVE), r.render(ADDITIVE))
            }
            // `a + (-b)` is written as the subtraction `a - b`, never as `a + - b`.
            ExprKind::Add(l, r) => match r.negated() {
                Some(operand) => {
                    write!(
                        f,
                        "{} - {}",
                        l.render(ADDITIVE),
                        operand.render(BIG_OPERATOR)
                    )
                }
                None => write!(f, "{} + {}", l.render(ADDITIVE), r.render(ADDITIVE)),
            },
            // `a - (b + c)` and `a - (-b)` need parentheses; `a - b c` and `a - \int f dx` do not.
            ExprKind::Sub(l, r) => {
                write!(f, "{} - {}", l.render(ADDITIVE), r.render(BIG_OPERATOR))
            }
            ExprKind::Mul(l, r) => match self.negated() {
                Some(negated) => write!(f, "- {}", negated.render(BIG_OPERATOR)),
                None => {
                    // Juxtaposition would merge a number into what precedes it: `2 3` reads
                    // as 23, and `x 2` as `x2`. The product is then written out.
                    let separator = if r.starts_with_number() {
                        r" \cdot "
                    } else {
                        " "
                    };
                    write!(
                        f,
                        "{}{separator}{}",
                        l.render(MULTIPLICATIVE),
                        r.render(MULTIPLICATIVE)
                    )
                }
            },
            ExprKind::List(items) => {
                let items: Vec<String> = items.iter().map(Expr::to_string).collect();
                write!(f, "{}", items.join(", "))
            }
            ExprKind::Named { expr, .. } => write!(f, "{expr}"),
            ExprKind::Neg(e) => write!(f, "- {}", e.render(BIG_OPERATOR)),
            ExprKind::Fraction {
                numerator,
                denominator,
            } => write!(f, r"\frac{{{numerator}}}{{{denominator}}}"),
            ExprKind::Function {
                function,
                arguments,
            } => {
                if arguments.is_empty() {
                    return write!(f, "{function}");
                }
                let arguments: Vec<String> = arguments.iter().map(Expr::to_string).collect();
                write!(f, r"{function} \left( {} \right)", arguments.join(", "))
            }
            ExprKind::Integral {
                lower,
                upper,
                integrand,
                variable_of_integration,
            } => {
                // Omit an absent bound instead of emitting an empty `_{}` / `^{}`.
                let bound = |mark: char, e: &Option<Expr>| {
                    e.as_ref()
                        .map(|e| format!("{mark}{{{e}}}"))
                        .unwrap_or_default()
                };
                let (lower, upper) = (bound('_', lower), bound('^', upper));
                let integrand = integrand.render(PREFIX);
                write!(
                    f,
                    r"{{\int{lower}{upper} {integrand} d{variable_of_integration}}}"
                )
            }
            ExprKind::BigOperator {
                kind,
                lower,
                upper,
                body,
            } => {
                let symbol = match kind {
                    BigOperatorKind::Sum => r"\sum",
                    BigOperatorKind::Product => r"\prod",
                };
                let upper = upper
                    .as_ref()
                    .map(|upper| format!("^{{{upper}}}"))
                    .unwrap_or_default();
                // As for an integrand, a sum or a difference needs parentheses.
                let body = body.render(PREFIX);
                write!(f, "{{{symbol}_{{{lower}}}{upper} {body}}}")
            }
            ExprKind::Number(n) => write!(f, "{n}"),
            ExprKind::Subscript { base, index } => {
                write!(f, "{{{}_{{{index}}}}}", base.render(ATOM))
            }
            ExprKind::Superscript { base, label } => {
                write!(f, "{{{}^{{{label}}}}}", base.render(ATOM))
            }
            ExprKind::Power { base, exponent } => {
                write!(f, "{{{{{}}}^{{{exponent}}}}}", base.render(ATOM))
            }
            ExprKind::Delimited { delimiter, inner } => match delimiter {
                Delimiter::Paren => write!(f, r"\left( {inner} \right)"),
                Delimiter::Angle => write!(f, r"\left\langle {inner} \right\rangle"),
            },
            ExprKind::Sqrt(e) => write!(f, r"{{\sqrt{{{e}}}}}"),
            ExprKind::Symbol(s) => write!(f, "{{{}}}", s.latex),
            ExprKind::Derivative {
                kind,
                form,
                target,
                variable,
            } => {
                let d = match kind {
                    DerivativeKind::Partial => r"\partial",
                    DerivativeKind::Total => "d",
                };
                // `\partial x y` and `\partial a + b` would be misread, like the target.
                let variable = variable.render(POWER);
                match form {
                    DerivativeForm::Fraction => {
                        // `\partial \rho u` would read as `(\partial \rho) u`, so a product (or
                        // anything weaker) is parenthesized. A power or a label binds tighter:
                        // `\partial u^2` and `\partial u_i^n` are written as they are.
                        let target = target.render(POWER);
                        write!(f, r"\frac{{{d} {target}}}{{{d} {variable}}}")
                    }
                    DerivativeForm::Operator => {
                        write!(f, r"\frac{{{d}}}{{{d} {variable}}} \left( {target} \right)")
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Symbol;

    fn s(name: &str) -> Expr {
        Expr::from_symbol(Symbol::new(name, name))
    }

    #[test]
    fn sub_with_sum_on_right_is_parenthesized() {
        let e = s("a") - (s("b") + s("c"));
        assert_eq!(e.to_string(), r"{a} - \left( {b} + {c} \right)");
    }

    #[test]
    fn sub_chain_needs_no_parentheses_on_left() {
        let e = (s("a") - s("b")) - s("c");
        assert_eq!(e.to_string(), "{a} - {b} - {c}");
    }

    #[test]
    fn mul_with_neg_is_parenthesized() {
        assert_eq!((s("a") * -s("b")).to_string(), r"{a} \left( - {b} \right)");
        // A negated first factor: the sign goes in front, `(-a) b` is written `- a b`.
        assert_eq!((-s("a") * s("b")).to_string(), "- {a} {b}");
    }

    #[test]
    fn add_of_a_negation_is_written_as_a_subtraction() {
        assert_eq!((s("a") + -s("b")).to_string(), "{a} - {b}");
        assert_eq!((s("a") + -(s("b") * s("c"))).to_string(), "{a} - {b} {c}");
        assert_eq!(
            (s("a") + -(s("b") + s("c"))).to_string(),
            r"{a} - \left( {b} + {c} \right)"
        );
        // Chains stay flat, and the negation may come first.
        assert_eq!(
            (-s("a") + -s("b") + -s("c")).to_string(),
            "- {a} - {b} - {c}"
        );
        // Looking through a name: the notation of a definition does not matter.
        let mut doc = crate::Document::new("");
        let named = doc.def("minus_b", -s("b"));
        assert_eq!((s("a") + named).to_string(), "{a} - {b}");
    }

    #[test]
    fn negated_first_factor_is_written_as_a_negation() {
        let (a, b, c) = (s("a"), s("b"), s("c"));
        assert_eq!((-&a * &b * &c).to_string(), "- {a} {b} {c}");
        assert_eq!(
            ((-&a + &b) * &c).to_string(),
            r"\left( - {a} + {b} \right) {c}"
        );
        assert_eq!(
            (-(&a + &b) * &c).to_string(),
            r"- \left( {a} + {b} \right) {c}"
        );
        // Both factors negated.
        assert_eq!((-&a * -&b).to_string(), r"- {a} \left( - {b} \right)");
        // It behaves like any negation as an operand: parenthesized, or a subtraction.
        assert_eq!(
            (&a * (-&b * &c)).to_string(),
            r"{a} \left( - {b} {c} \right)"
        );
        assert_eq!((-(-&a * &b)).to_string(), r"- \left( - {a} {b} \right)");
        assert_eq!(
            (&a - (-&b * &c)).to_string(),
            r"{a} - \left( - {b} {c} \right)"
        );
        assert_eq!((&a + (-&b * &c)).to_string(), "{a} - {b} {c}");
        // The sign of the second factor stays parenthesized.
        assert_eq!((&a * -&b).to_string(), r"{a} \left( - {b} \right)");
    }

    #[test]
    fn sub_of_a_negation_is_parenthesized() {
        assert_eq!(
            (s("a") - -s("b")).to_string(),
            r"{a} - \left( - {b} \right)"
        );
        assert_eq!(
            (s("a") + -(-s("b"))).to_string(),
            r"{a} - \left( - {b} \right)"
        );
    }

    #[test]
    fn neg_of_sum_and_neg_of_neg() {
        assert_eq!(
            (-(s("a") + s("b"))).to_string(),
            r"- \left( {a} + {b} \right)"
        );
        assert_eq!((-(-s("a"))).to_string(), r"- \left( - {a} \right)");
    }

    #[test]
    fn mul_of_sums_is_parenthesized() {
        let e = (s("a") + s("b")) * (s("c") - s("d"));
        assert_eq!(
            e.to_string(),
            r"\left( {a} + {b} \right) \left( {c} - {d} \right)"
        );
    }

    #[test]
    fn integral_with_sum_integrand_is_parenthesized() {
        let e = Expr::indefinite_integral(s("a") + s("b"), s("x"));
        assert_eq!(e.to_string(), r"{\int \left( {a} + {b} \right) d{x}}");
        let e = Expr::indefinite_integral(s("a") * s("b"), s("x"));
        assert_eq!(e.to_string(), r"{\int {a} {b} d{x}}");
    }

    #[test]
    fn integral_as_mul_operand_is_parenthesized() {
        let i = Expr::indefinite_integral(s("a"), s("x"));
        assert_eq!(
            (&i * s("b")).to_string(),
            r"\left( {\int {a} d{x}} \right) {b}"
        );
    }

    #[test]
    fn power_base_parenthesization() {
        assert_eq!(s("a").pow(s("n")).to_string(), "{{{a}}^{{n}}}");
        assert_eq!(
            (s("a") * s("b")).pow(s("n")).to_string(),
            r"{{\left( {a} {b} \right)}^{{n}}}"
        );
        assert_eq!(
            s("a").pow(s("m")).pow(s("n")).to_string(),
            r"{{\left( {{{a}}^{{m}}} \right)}^{{n}}}"
        );
    }

    #[test]
    fn relation() {
        let e = Expr::relation(s("a") + s("b"), Expr::integer(0));
        assert_eq!(e.to_string(), "{a} + {b} = 0");
        let nested = Expr::relation(Expr::relation(s("a"), s("b")), s("c"));
        assert_eq!(nested.to_string(), r"\left( {a} = {b} \right) = {c}");
    }

    #[test]
    fn the_three_forms_of_integral() {
        // No empty `_{}` / `^{}` is written for a bound that is absent.
        assert_eq!(
            Expr::indefinite_integral(s("f"), s("x")).to_string(),
            r"{\int {f} d{x}}"
        );
        assert_eq!(
            Expr::integral_over(s("V"), s("f"), s("x")).to_string(),
            r"{\int_{{V}} {f} d{x}}"
        );
        assert_eq!(
            Expr::definite_integral(s("a"), s("b"), s("f"), s("x")).to_string(),
            r"{\int_{{a}}^{{b}} {f} d{x}}"
        );
    }

    #[test]
    fn the_forms_of_sums_and_products() {
        let (k, n, a) = (s("k"), s("N"), s("a"));
        let to = &n - Expr::integer(1);
        assert_eq!(Expr::sum_over(&k, &a).to_string(), r"{\sum_{{k}} {a}}");
        assert_eq!(
            Expr::sum_between(&k, Expr::integer(0), &to, &a).to_string(),
            r"{\sum_{{k} = 0}^{{N} - 1} {a}}"
        );
        assert_eq!(Expr::product_over(&k, &a).to_string(), r"{\prod_{{k}} {a}}");
        assert_eq!(
            Expr::product_between(&k, Expr::integer(1), &n, &a).to_string(),
            r"{\prod_{{k} = 1}^{{N}} {a}}"
        );
    }

    #[test]
    fn summand_extends_over_products_but_not_over_sums() {
        let (k, l, a, b) = (s("k"), s("l"), s("a"), s("b"));
        assert_eq!(
            Expr::sum_over(&k, &a * &b).to_string(),
            r"{\sum_{{k}} {a} {b}}"
        );
        assert_eq!(
            Expr::sum_over(&k, &a + &b).to_string(),
            r"{\sum_{{k}} \left( {a} + {b} \right)}"
        );
        assert_eq!(
            Expr::sum_over(&k, Expr::relation(&a, &b)).to_string(),
            r"{\sum_{{k}} \left( {a} = {b} \right)}"
        );
        // Nested sums stay flat.
        assert_eq!(
            Expr::sum_over(&l, Expr::sum_over(&k, &a)).to_string(),
            r"{\sum_{{l}} {\sum_{{k}} {a}}}"
        );
    }

    #[test]
    fn sums_and_products_are_terms_but_parenthesized_factors() {
        let (k, a, b, c) = (s("k"), s("a"), s("b"), s("c"));
        let sum = Expr::sum_over(&k, &b);
        // A term of a sum or a difference, or either side of `=`: no parentheses.
        assert_eq!((&a + &sum).to_string(), r"{a} + {\sum_{{k}} {b}}");
        assert_eq!((&sum + &a).to_string(), r"{\sum_{{k}} {b}} + {a}");
        assert_eq!((&a - &sum).to_string(), r"{a} - {\sum_{{k}} {b}}");
        assert_eq!(
            Expr::relation(&sum, &c).to_string(),
            r"{\sum_{{k}} {b}} = {c}"
        );
        assert_eq!((-&sum).to_string(), r"- {\sum_{{k}} {b}}");
        // A factor, a power base: parenthesized (nothing says where the sum ends).
        assert_eq!(
            (&c * &sum).to_string(),
            r"{c} \left( {\sum_{{k}} {b}} \right)"
        );
        assert_eq!(
            (&sum * &c).to_string(),
            r"\left( {\sum_{{k}} {b}} \right) {c}"
        );
        assert_eq!(
            sum.pow(Expr::integer(2)).to_string(),
            r"{{\left( {\sum_{{k}} {b}} \right)}^{2}}"
        );
        let product = Expr::product_over(&k, &b);
        assert_eq!(
            (&c * &product).to_string(),
            r"{c} \left( {\prod_{{k}} {b}} \right)"
        );
    }

    #[test]
    fn angle_brackets() {
        let (q, k, l, w) = (s("Q"), s("k"), s("l"), s("w"));
        let (qk, ql) = (Expr::subscript(&q, &k), Expr::subscript(&q, &l));
        let inner = Expr::angle(Expr::list([&qk, &ql]));
        assert_eq!(
            inner.to_string(),
            r"\left\langle {{Q}_{{k}}}, {{Q}_{{l}}} \right\rangle"
        );
        // an atom: a weight as a subscript, a factor and a power base need no parentheses
        assert_eq!(
            Expr::subscript(&inner, &w).to_string(),
            r"{\left\langle {{Q}_{{k}}}, {{Q}_{{l}}} \right\rangle_{{w}}}"
        );
        assert_eq!(
            (&w * &inner).to_string(),
            r"{w} \left\langle {{Q}_{{k}}}, {{Q}_{{l}}} \right\rangle"
        );
        // the inner expression is as written: no automatic parentheses
        assert_eq!(
            Expr::angle(&k + &l).to_string(),
            r"\left\langle {k} + {l} \right\rangle"
        );
    }

    #[test]
    fn lists() {
        let (p, i, k) = (s("P"), s("i"), s("k"));
        let two = Expr::integer(2);
        let list = Expr::list([&i, &(&k - &two)]);
        assert_eq!(list.to_string(), "{i}, {k} - 2");
        assert_eq!(
            Expr::subscript(&p, &list).to_string(),
            "{{P}_{{i}, {k} - 2}}"
        );
        assert_eq!(
            Expr::function(&p, [Expr::list([&i, &k]), two.clone()]).to_string(),
            r"{P} \left( {i}, {k}, 2 \right)"
        );
        // anywhere else it is an operand, and parenthesized
        assert_eq!((&p * &list).to_string(), r"{P} \left( {i}, {k} - 2 \right)");
        // half-integer grid points
        let half = Expr::fraction(Expr::integer(1), &two);
        assert_eq!(
            Expr::subscript(&p, Expr::list([&i + &half, k.clone()])).to_string(),
            r"{{P}_{{i} + \frac{1}{2}, {k}}}"
        );
    }

    #[test]
    fn functions() {
        let (h, x, t) = (s("H"), s("x"), s("t"));
        assert_eq!(
            Expr::function(&h, [&x]).to_string(),
            r"{H} \left( {x} \right)"
        );
        assert_eq!(
            Expr::function(&h, [&x, &t]).to_string(),
            r"{H} \left( {x}, {t} \right)"
        );
        assert_eq!(Expr::function(&h, [] as [&Expr; 0]).to_string(), "{H}");
        // an atom: a factor, a power base and a subscript base need no parentheses
        let hx = Expr::function(&h, [&x]);
        assert_eq!((&x * &hx).to_string(), r"{x} {H} \left( {x} \right)");
        assert_eq!(
            hx.pow(Expr::integer(2)).to_string(),
            r"{{{H} \left( {x} \right)}^{2}}"
        );
        assert_eq!(
            Expr::paren(&hx).pow(Expr::integer(2)).to_string(),
            r"{{\left( {H} \left( {x} \right) \right)}^{2}}"
        );
    }

    #[test]
    fn subscripts_and_superscripts() {
        let (u, i, n) = (s("u"), s("i"), s("n"));
        assert_eq!(Expr::subscript(&u, &i).to_string(), r"{{u}_{{i}}}");
        assert_eq!(Expr::superscript(&u, &n).to_string(), r"{{u}^{{n}}}");
        // The index and the label are any expression, written as they are.
        assert_eq!(
            Expr::subscript(&u, &i + Expr::integer(1)).to_string(),
            r"{{u}_{{i} + 1}}"
        );
        assert_eq!(
            Expr::superscript(&u, &n + Expr::integer(1)).to_string(),
            r"{{u}^{{n} + 1}}"
        );
        // Both on one base: `u_i^{n + 1}`.
        assert_eq!(
            Expr::superscript(Expr::subscript(&u, &i), &n + Expr::integer(1)).to_string(),
            r"{{{u}_{{i}}}^{{n} + 1}}"
        );
    }

    #[test]
    fn a_subscripted_base_is_an_atom_but_a_labelled_one_is_not() {
        let (u, x, i, j, n) = (s("u"), s("x"), s("i"), s("j"), s("n"));
        let u_i = Expr::subscript(&u, &i);
        // `u_i` needs no parentheses as a derivative target, a power base or a factor.
        assert_eq!(
            Expr::partial_derivative(&u_i, Expr::subscript(&x, &j)).to_string(),
            r"\frac{\partial {{u}_{{i}}}}{\partial {{x}_{{j}}}}"
        );
        assert_eq!(
            u_i.pow(Expr::integer(2)).to_string(),
            r"{{{{u}_{{i}}}}^{2}}"
        );
        assert_eq!(
            (&u_i * Expr::subscript(&x, &j)).to_string(),
            r"{{u}_{{i}}} {{x}_{{j}}}"
        );
        // `u^n` is not: it is parenthesized as the base of another label, of a power, or of
        // a subscript.
        let u_n = Expr::superscript(&u, &n);
        assert_eq!(
            Expr::superscript(&u_n, &i).to_string(),
            r"{\left( {{u}^{{n}}} \right)^{{i}}}"
        );
        assert_eq!(
            u_n.pow(Expr::integer(2)).to_string(),
            r"{{\left( {{u}^{{n}}} \right)}^{2}}"
        );
        assert_eq!(
            Expr::subscript(&u_n, &i).to_string(),
            r"{\left( {{u}^{{n}}} \right)_{{i}}}"
        );
        // A base that is not an atom is parenthesized.
        assert_eq!(
            Expr::subscript(&u * &x, &i).to_string(),
            r"{\left( {u} {x} \right)_{{i}}}"
        );
    }

    #[test]
    fn a_number_after_a_factor_is_separated_by_a_dot() {
        let (x, y) = (s("x"), s("y"));
        let (two, three) = (Expr::integer(2), Expr::integer(3));
        // Not merged into one number, nor into the symbol before it.
        assert_eq!((&two * &three).to_string(), r"2 \cdot 3");
        assert_eq!(
            (&two * Expr::number("0.5").unwrap()).to_string(),
            r"2 \cdot 0.5"
        );
        assert_eq!((&x * &two).to_string(), r"{x} \cdot 2");
        assert_eq!((&x * two.pow(&three)).to_string(), r"{x} \cdot {{2}^{3}}");
        assert_eq!((&x * &y * &two).to_string(), r"{x} {y} \cdot 2");
        assert_eq!((&x * (&two * &y)).to_string(), r"{x} \cdot 2 {y}");
        assert_eq!(
            (Expr::fraction(Expr::integer(1), &two) * &three).to_string(),
            r"\frac{1}{2} \cdot 3"
        );
        // A number first, or a factor that is not a number, is written as before.
        assert_eq!((&two * &x).to_string(), "2 {x}");
        assert_eq!((&x * &y).to_string(), "{x} {y}");
        // Also through the rewriting of negations.
        assert_eq!((-&x * &two).to_string(), r"- {x} \cdot 2");
        assert_eq!((Expr::integer(-2) * &x).to_string(), "- 2 {x}");
    }

    #[test]
    fn derivative_variable_is_parenthesized_like_the_target() {
        let (f, x, y) = (s("f"), s("x"), s("y"));
        assert_eq!(
            Expr::partial_derivative(&f, &x * &y).to_string(),
            r"\frac{\partial {f}}{\partial \left( {x} {y} \right)}"
        );
        assert_eq!(
            Expr::total_derivative(&f, &x + &y).to_string(),
            r"\frac{d {f}}{d \left( {x} + {y} \right)}"
        );
        assert_eq!(
            Expr::partial_derivative_operator(&f, &x * &y).to_string(),
            r"\frac{\partial}{\partial \left( {x} {y} \right)} \left( {f} \right)"
        );
        // An atom, a subscripted symbol and a label are written as they are.
        assert_eq!(
            Expr::partial_derivative(&f, Expr::subscript(&x, &y)).to_string(),
            r"\frac{\partial {f}}{\partial {{x}_{{y}}}}"
        );
    }

    #[test]
    fn explicit_parentheses_are_always_written() {
        let (a, b, c) = (s("a"), s("b"), s("c"));
        assert_eq!(Expr::paren(&a).to_string(), r"\left( {a} \right)");
        // Even where the meaning does not need them: the author's grouping is kept.
        assert_eq!(
            (&a + Expr::paren(&b + &c)).to_string(),
            r"{a} + \left( {b} + {c} \right)"
        );
        assert_eq!(
            (Expr::paren(&a * &b) + &c).to_string(),
            r"\left( {a} {b} \right) + {c}"
        );
        // Where the meaning needs them, they are not doubled.
        assert_eq!(
            (Expr::paren(&a + &b) * &c).to_string(),
            r"\left( {a} + {b} \right) {c}"
        );
        assert_eq!(
            Expr::paren(&a * &b).pow(Expr::integer(2)).to_string(),
            r"{{\left( {a} {b} \right)}^{2}}"
        );
    }

    #[test]
    fn explicit_parentheses_stop_the_rewriting_of_negations() {
        let (a, b) = (s("a"), s("b"));
        assert_eq!(
            (&a + Expr::paren(-&b)).to_string(),
            r"{a} + \left( - {b} \right)"
        );
        assert_eq!(
            (Expr::paren(-&a) * &b).to_string(),
            r"\left( - {a} \right) {b}"
        );
        // Without them the rewriting applies, also through a name.
        assert_eq!((&a + -&b).to_string(), "{a} - {b}");
        let mut doc = crate::Document::new("");
        let named = doc.def("minus_b", -&b);
        assert_eq!((&a + named).to_string(), "{a} - {b}");
    }

    #[test]
    fn integral_as_term_needs_no_parentheses() {
        let i = Expr::indefinite_integral(s("f"), s("x"));
        assert_eq!((s("a") - &i).to_string(), r"{a} - {\int {f} d{x}}");
        assert_eq!((-&i).to_string(), r"- {\int {f} d{x}}");
    }

    #[test]
    fn derivatives_differ_only_in_the_symbol() {
        let target = s("a") * s("b");
        assert_eq!(
            Expr::partial_derivative(s("a"), s("t")).to_string(),
            r"\frac{\partial {a}}{\partial {t}}"
        );
        assert_eq!(
            Expr::total_derivative(s("a"), s("t")).to_string(),
            r"\frac{d {a}}{d {t}}"
        );
        assert_eq!(
            Expr::partial_derivative_operator(s("a"), s("t")).to_string(),
            r"\frac{\partial}{\partial {t}} \left( {a} \right)"
        );
        assert_eq!(
            Expr::total_derivative_operator(&target, s("t")).to_string(),
            r"\frac{d}{d {t}} \left( {a} {b} \right)"
        );
    }

    #[test]
    fn derivative_target_is_parenthesized_unless_it_binds_tighter_than_a_product() {
        assert_eq!(
            Expr::partial_derivative(s("a") * s("b"), s("t")).to_string(),
            r"\frac{\partial \left( {a} {b} \right)}{\partial {t}}"
        );
        assert_eq!(
            Expr::total_derivative(s("a") + s("b"), s("t")).to_string(),
            r"\frac{d \left( {a} + {b} \right)}{d {t}}"
        );
    }

    #[test]
    fn derivative_target_may_be_a_power_or_a_label() {
        let (u, x, i, j, n) = (s("u"), s("x"), s("i"), s("j"), s("n"));
        let u_i_n = Expr::superscript(Expr::subscript(&u, &i), &n);
        assert_eq!(
            Expr::partial_derivative(&u_i_n, Expr::subscript(&x, &j)).to_string(),
            r"\frac{\partial {{{u}_{{i}}}^{{n}}}}{\partial {{x}_{{j}}}}"
        );
        assert_eq!(
            Expr::partial_derivative(u.pow(Expr::integer(2)), &x).to_string(),
            r"\frac{\partial {{{u}}^{2}}}{\partial {x}}"
        );
        assert_eq!(
            Expr::total_derivative(Expr::subscript(&u, &i), &x).to_string(),
            r"\frac{d {{u}_{{i}}}}{d {x}}"
        );
        // Products and anything weaker are still parenthesized.
        assert_eq!(
            Expr::partial_derivative(&u * &x, &i).to_string(),
            r"\frac{\partial \left( {u} {x} \right)}{\partial {i}}"
        );
    }

    #[test]
    fn derivative_precedence() {
        let (a, b) = (s("a"), s("b"));
        // A fraction is an atom: a power base needs no more than its own parentheses rule.
        assert_eq!(
            Expr::partial_derivative(&a, &b).pow(s("n")).to_string(),
            r"{{\frac{\partial {a}}{\partial {b}}}^{{n}}}"
        );
        // The operator form acts like a factor: it is parenthesized in a power base ...
        assert_eq!(
            Expr::total_derivative_operator(&a, &b)
                .pow(s("n"))
                .to_string(),
            r"{{\left( \frac{d}{d {b}} \left( {a} \right) \right)}^{{n}}}"
        );
        // ... and in a product it needs none.
        assert_eq!(
            (&a * Expr::partial_derivative_operator(&a, &b)).to_string(),
            r"{a} \frac{\partial}{\partial {b}} \left( {a} \right)"
        );
    }
}
