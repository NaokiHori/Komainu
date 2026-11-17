#[derive(Debug)]
enum ExprKind {
    Add(Expr, Expr),
    /// A sum or a product over an index: `\sum_{lower}^{upper} body`.
    BigOperator {
        kind: BigOperatorKind,
        lower: Expr,
        upper: Option<Expr>,
        body: Expr,
    },
    Fraction {
        numerator: Expr,
        denominator: Expr,
    },
    /// A function applied to arguments: `f \left( a, b \right)`.
    Function {
        function: Expr,
        arguments: Vec<Expr>,
    },
    Integral {
        lower: Option<Expr>,
        upper: Option<Expr>,
        integrand: Expr,
        variable_of_integration: Expr,
    },
    /// Expressions separated by commas, without parentheses: `a, b`. Meant for a place that
    /// takes several (an index `{i, k}`); anywhere else it is parenthesized.
    List(Vec<Expr>),
    Mul(Expr, Expr),
    Named {
        name: String,
        expr: Expr,
    },
    Neg(Expr),
    Number(String),
    /// Parentheses written explicitly; always written, whether or not the meaning needs them.
    Paren(Expr),
    Derivative {
        kind: DerivativeKind,
        form: DerivativeForm,
        target: Expr,
        variable: Expr,
    },
    Power {
        base: Expr,
        exponent: Expr,
    },
    Relation(Expr, Expr),
    Sqrt(Expr),
    Sub(Expr, Expr),
    /// `base_{index}`
    Subscript {
        base: Expr,
        index: Expr,
    },
    /// `base^{label}`: a label on the base (a time level, a step), not an exponent.
    Superscript {
        base: Expr,
        label: Expr,
    },
    Symbol(Symbol),
}

/// Which big operator: `\sum` or `\prod`.
#[derive(Debug, Clone, Copy)]
enum BigOperatorKind {
    Sum,
    Product,
}

/// Which derivative: `\partial` or `d`.
#[derive(Debug, Clone, Copy)]
enum DerivativeKind {
    Partial,
    Total,
}

/// How it is written: as a fraction (`\frac{\partial f}{\partial x}`) or as an operator
/// applied to the target (`\frac{\partial}{\partial x} \left( f \right)`).
#[derive(Debug, Clone, Copy)]
enum DerivativeForm {
    Fraction,
    Operator,
}

/// A registered symbol: a name (used by templates) and its LaTeX notation (used in the output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Symbol {
    name: String,
    latex: String,
}

impl Symbol {
    pub(crate) fn new(name: &str, latex: &str) -> Self {
        Self {
            name: name.to_string(),
            latex: latex.to_string(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// A node reached while walking an expression tree.
pub(crate) enum NodeRef<'a> {
    Symbol(&'a Symbol),
    Named(&'a str),
}

#[derive(Debug)]
pub struct Expr(std::rc::Rc<ExprKind>);

impl Clone for Expr {
    fn clone(&self) -> Self {
        Self(std::rc::Rc::clone(&self.0))
    }
}

impl AsRef<Expr> for Expr {
    fn as_ref(&self) -> &Self {
        self
    }
}

mod render;

impl Expr {
    pub(crate) fn from_symbol(symbol: Symbol) -> Self {
        Self(std::rc::Rc::new(ExprKind::Symbol(symbol)))
    }

    /// Marks `expr` as the value of the definition `name` (renders exactly like `expr`).
    pub(crate) fn named(name: &str, expr: &Self) -> Self {
        Self(std::rc::Rc::new(ExprKind::Named {
            name: name.to_string(),
            expr: expr.clone(),
        }))
    }

    /// Calls `f` once for every distinct node reachable from `self` (shared nodes are visited once).
    pub(crate) fn walk(&self, f: &mut impl FnMut(NodeRef<'_>)) {
        let mut seen = std::collections::HashSet::new();
        self.walk_inner(f, &mut seen);
    }

    fn walk_inner(
        &self,
        f: &mut impl FnMut(NodeRef<'_>),
        seen: &mut std::collections::HashSet<*const ExprKind>,
    ) {
        if !seen.insert(std::rc::Rc::as_ptr(&self.0)) {
            return;
        }
        let mut children: Vec<&Expr> = Vec::new();
        match self.kind() {
            ExprKind::Symbol(symbol) => f(NodeRef::Symbol(symbol)),
            ExprKind::Named { name, expr } => {
                f(NodeRef::Named(name));
                children.push(expr);
            }
            ExprKind::Number(_) => {}
            ExprKind::Add(l, r)
            | ExprKind::Sub(l, r)
            | ExprKind::Mul(l, r)
            | ExprKind::Relation(l, r) => {
                children.extend([l, r]);
            }
            ExprKind::List(items) => children.extend(items),
            ExprKind::Neg(e) | ExprKind::Sqrt(e) | ExprKind::Paren(e) => children.push(e),
            ExprKind::Fraction {
                numerator,
                denominator,
            } => children.extend([numerator, denominator]),
            ExprKind::Function {
                function,
                arguments,
            } => {
                children.push(function);
                children.extend(arguments);
            }
            ExprKind::Integral {
                lower,
                upper,
                integrand,
                variable_of_integration,
            } => {
                children.extend(lower.iter().chain(upper.iter()));
                children.extend([integrand, variable_of_integration]);
            }
            ExprKind::BigOperator {
                lower, upper, body, ..
            } => {
                children.push(lower);
                children.extend(upper.iter());
                children.push(body);
            }
            ExprKind::Derivative {
                target, variable, ..
            } => children.extend([target, variable]),
            ExprKind::Power { base, exponent } => children.extend([base, exponent]),
            ExprKind::Subscript { base, index: part }
            | ExprKind::Superscript { base, label: part } => children.extend([base, part]),
        }
        for child in children {
            child.walk_inner(f, seen);
        }
    }

    /// An integer: `Expr::integer(2)` is `2`, and a negative value is the negation of its
    /// absolute value (`Expr::integer(-1)` is `- 1`, like `-1` in a template).
    pub fn integer(value: i64) -> Self {
        let number = Self(std::rc::Rc::new(ExprKind::Number(
            value.unsigned_abs().to_string(),
        )));
        if value < 0 { -number } else { number }
    }

    /// A number kept verbatim; the format is restricted to `[0-9]+(\.[0-9]+)?` (D13).
    pub fn number(literal: &str) -> Result<Self, String> {
        let is_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let valid = match literal.split_once('.') {
            Some((int, frac)) => is_digits(int) && is_digits(frac),
            None => is_digits(literal),
        };
        if valid {
            Ok(Self(std::rc::Rc::new(ExprKind::Number(
                literal.to_string(),
            ))))
        } else {
            Err(format!("invalid number literal: {literal:?}"))
        }
    }

    /// Explicit parentheses: `\left( inner \right)`, written even where the meaning would not
    /// need them (for instance to keep the grouping `a + (b + c)`). Parentheses that are
    /// needed are added automatically; these are the ones the author insists on.
    pub fn paren<T: AsRef<Self>>(inner: T) -> Self {
        Self(std::rc::Rc::new(ExprKind::Paren(inner.as_ref().clone())))
    }

    /// The relation `lhs = rhs`.
    pub fn relation<T: AsRef<Self>, U: AsRef<Self>>(lhs: T, rhs: U) -> Self {
        Self(std::rc::Rc::new(ExprKind::Relation(
            lhs.as_ref().clone(),
            rhs.as_ref().clone(),
        )))
    }

    pub fn fraction<T: AsRef<Self>, U: AsRef<Self>>(numerator: T, denominator: U) -> Self {
        Self(std::rc::Rc::new(ExprKind::Fraction {
            numerator: numerator.as_ref().clone(),
            denominator: denominator.as_ref().clone(),
        }))
    }

    fn integral(
        lower: Option<&Self>,
        upper: Option<&Self>,
        integrand: &Self,
        variable_of_integration: &Self,
    ) -> Self {
        Self(std::rc::Rc::new(ExprKind::Integral {
            lower: lower.cloned(),
            upper: upper.cloned(),
            integrand: integrand.clone(),
            variable_of_integration: variable_of_integration.clone(),
        }))
    }

    /// `\int f dx`
    pub fn indefinite_integral<T: AsRef<Self>, U: AsRef<Self>>(integrand: T, variable: U) -> Self {
        Self::integral(None, None, integrand.as_ref(), variable.as_ref())
    }

    /// `\int_{domain} f dx`: only the region is given, e.g. `\int_{V} q dV`.
    pub fn integral_over<D: AsRef<Self>, T: AsRef<Self>, U: AsRef<Self>>(
        domain: D,
        integrand: T,
        variable: U,
    ) -> Self {
        Self::integral(
            Some(domain.as_ref()),
            None,
            integrand.as_ref(),
            variable.as_ref(),
        )
    }

    /// `\int_{lower}^{upper} f dx`
    pub fn definite_integral<L: AsRef<Self>, H: AsRef<Self>, T: AsRef<Self>, U: AsRef<Self>>(
        lower: L,
        upper: H,
        integrand: T,
        variable: U,
    ) -> Self {
        Self::integral(
            Some(lower.as_ref()),
            Some(upper.as_ref()),
            integrand.as_ref(),
            variable.as_ref(),
        )
    }

    fn big_operator(
        kind: BigOperatorKind,
        lower: &Self,
        upper: Option<&Self>,
        body: &Self,
    ) -> Self {
        Self(std::rc::Rc::new(ExprKind::BigOperator {
            kind,
            lower: lower.clone(),
            upper: upper.cloned(),
            body: body.clone(),
        }))
    }

    /// `\sum_{index} f`: what is summed over is given as it is written, so it may be more
    /// than a symbol (`k, k \ne i` is a symbol whose LaTeX says so).
    pub fn sum_over<I: AsRef<Self>, T: AsRef<Self>>(index: I, summand: T) -> Self {
        Self::big_operator(BigOperatorKind::Sum, index.as_ref(), None, summand.as_ref())
    }

    /// `\sum_{index = from}^{to} f`
    pub fn sum_between<I: AsRef<Self>, L: AsRef<Self>, H: AsRef<Self>, T: AsRef<Self>>(
        index: I,
        from: L,
        to: H,
        summand: T,
    ) -> Self {
        Self::big_operator(
            BigOperatorKind::Sum,
            &Self::relation(index, from),
            Some(to.as_ref()),
            summand.as_ref(),
        )
    }

    /// `\prod_{index} f`
    pub fn product_over<I: AsRef<Self>, T: AsRef<Self>>(index: I, factor: T) -> Self {
        Self::big_operator(
            BigOperatorKind::Product,
            index.as_ref(),
            None,
            factor.as_ref(),
        )
    }

    /// `\prod_{index = from}^{to} f`
    pub fn product_between<I: AsRef<Self>, L: AsRef<Self>, H: AsRef<Self>, T: AsRef<Self>>(
        index: I,
        from: L,
        to: H,
        factor: T,
    ) -> Self {
        Self::big_operator(
            BigOperatorKind::Product,
            &Self::relation(index, from),
            Some(to.as_ref()),
            factor.as_ref(),
        )
    }

    fn kind(&self) -> &ExprKind {
        &self.0
    }

    fn derivative(
        kind: DerivativeKind,
        form: DerivativeForm,
        target: &Self,
        variable: &Self,
    ) -> Self {
        Self(std::rc::Rc::new(ExprKind::Derivative {
            kind,
            form,
            target: target.clone(),
            variable: variable.clone(),
        }))
    }

    /// `\frac{\partial target}{\partial variable}`
    pub fn partial_derivative<T: AsRef<Self>, U: AsRef<Self>>(target: T, variable: U) -> Self {
        Self::derivative(
            DerivativeKind::Partial,
            DerivativeForm::Fraction,
            target.as_ref(),
            variable.as_ref(),
        )
    }

    /// `\frac{\partial}{\partial variable} \left( target \right)`
    pub fn partial_derivative_operator<T: AsRef<Self>, U: AsRef<Self>>(
        target: T,
        variable: U,
    ) -> Self {
        Self::derivative(
            DerivativeKind::Partial,
            DerivativeForm::Operator,
            target.as_ref(),
            variable.as_ref(),
        )
    }

    /// `\frac{d target}{d variable}`
    pub fn total_derivative<T: AsRef<Self>, U: AsRef<Self>>(target: T, variable: U) -> Self {
        Self::derivative(
            DerivativeKind::Total,
            DerivativeForm::Fraction,
            target.as_ref(),
            variable.as_ref(),
        )
    }

    /// `\frac{d}{d variable} \left( target \right)`
    pub fn total_derivative_operator<T: AsRef<Self>, U: AsRef<Self>>(
        target: T,
        variable: U,
    ) -> Self {
        Self::derivative(
            DerivativeKind::Total,
            DerivativeForm::Operator,
            target.as_ref(),
            variable.as_ref(),
        )
    }

    /// `base_{index}`. The notation of the base is defined once, wherever the index is chosen.
    /// The index is any expression (`i`, `i + 1`); nothing checks it (indices get no special
    /// treatment, and a subscripted symbol is also fine as one symbol).
    pub fn subscript<B: AsRef<Self>, I: AsRef<Self>>(base: B, index: I) -> Self {
        Self(std::rc::Rc::new(ExprKind::Subscript {
            base: base.as_ref().clone(),
            index: index.as_ref().clone(),
        }))
    }

    /// `base^{label}`: a label attached to the base, such as a time level (`n`, `n + 1`, `*`).
    /// It is not an exponent; for a power, use [`Expr::pow`].
    pub fn superscript<B: AsRef<Self>, L: AsRef<Self>>(base: B, label: L) -> Self {
        Self(std::rc::Rc::new(ExprKind::Superscript {
            base: base.as_ref().clone(),
            label: label.as_ref().clone(),
        }))
    }

    /// `function` applied to `arguments`: `f \left( a, b \right)`. The notation of the function
    /// (`H`, `\sin`) is defined once, as a symbol. No arguments gives the function alone.
    /// The application is written without parentheses as the base of a power
    /// (`{f(x)}^{2}`); wrap it in [`Expr::paren`] to get them.
    pub fn function<F, I>(function: F, arguments: I) -> Self
    where
        F: AsRef<Self>,
        I: IntoIterator,
        I::Item: AsRef<Self>,
    {
        Self(std::rc::Rc::new(ExprKind::Function {
            function: function.as_ref().clone(),
            arguments: arguments.into_iter().map(|a| a.as_ref().clone()).collect(),
        }))
    }

    /// `a, b, ...`: expressions separated by commas, such as the indices of
    /// `Expr::subscript(&p, Expr::list([&i, &k]))` (`{p}_{i, k}`). It is written without
    /// parentheses at the top of an index or an argument, and parenthesized elsewhere.
    pub fn list<I>(items: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<Self>,
    {
        Self(std::rc::Rc::new(ExprKind::List(
            items.into_iter().map(|e| e.as_ref().clone()).collect(),
        )))
    }

    pub fn pow<T: AsRef<Self>>(&self, exponent: T) -> Self {
        Self(std::rc::Rc::new(ExprKind::Power {
            base: self.clone(),
            exponent: exponent.as_ref().clone(),
        }))
    }

    pub fn sqrt<T: AsRef<Self>>(expr: T) -> Self {
        Self(std::rc::Rc::new(ExprKind::Sqrt(expr.as_ref().clone())))
    }
}

impl std::ops::Neg for &Expr {
    type Output = Expr;

    fn neg(self) -> Self::Output {
        Expr(std::rc::Rc::new(ExprKind::Neg(self.clone())))
    }
}

impl std::ops::Neg for Expr {
    type Output = Expr;

    fn neg(self) -> Self::Output {
        Expr(std::rc::Rc::new(ExprKind::Neg(self)))
    }
}

macro_rules! implement_binary_operations {
    ($operator:ident, $function:ident, $kind:ident, $op:tt) => {
        impl<'a> std::ops::$operator<&'a Expr> for &Expr {
            type Output = Expr;

            fn $function(self, rhs: &'a Expr) -> Self::Output {
                Expr(std::rc::Rc::new(ExprKind::$kind(self.clone(), rhs.clone())))
            }
        }

        impl<'a> std::ops::$operator<&'a Expr> for Expr {
            type Output = Expr;

            fn $function(self, rhs: &'a Expr) -> Self::Output {
                &self $op rhs
            }
        }

        impl std::ops::$operator<Expr> for &Expr {
            type Output = Expr;

            fn $function(self, rhs: Expr) -> Self::Output {
                self $op &rhs
            }
        }

        impl std::ops::$operator for Expr {
            type Output = Expr;

            fn $function(self, rhs: Expr) -> Self::Output {
                &self $op &rhs
            }
        }
    };
}

implement_binary_operations!(Add, add, Add, +);
implement_binary_operations!(Sub, sub, Sub, -);
implement_binary_operations!(Mul, mul, Mul, *);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers() {
        assert_eq!(Expr::integer(0).to_string(), "0");
        assert_eq!(Expr::integer(2).to_string(), "2");
        assert_eq!(Expr::integer(-1).to_string(), "- 1");
        assert_eq!(Expr::integer(i64::MIN).to_string(), "- 9223372036854775808");
        // A negative integer is a negation, so it follows the rules of negations.
        let a = Expr::from_symbol(Symbol::new("a", "a"));
        assert_eq!((Expr::integer(-1) * &a).to_string(), "- 1 {a}");
        assert_eq!(
            (&a * Expr::integer(-1)).to_string(),
            r"{a} \left( - 1 \right)"
        );
        assert_eq!((&a + Expr::integer(-1)).to_string(), "{a} - 1");
    }

    #[test]
    fn numbers() {
        assert_eq!(Expr::number("0").unwrap().to_string(), "0");
        assert_eq!(Expr::number("0.5").unwrap().to_string(), "0.5");
        for bad in ["", ".5", "1.", "1e3", "-1", "1.2.3", "a"] {
            assert!(Expr::number(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
