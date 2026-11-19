use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::diagnostic::{Diagnostic, DiagnosticKind, Error, Location, Problem};
use crate::expr::{Expr, NodeRef, Symbol};
use crate::name;
use crate::parser::{self, Env, OperatorFn, Resolved};
use crate::template::{self, Segment};

#[derive(Clone)]
struct Entry {
    name: String,
    value: Expr,
    location: Location,
    /// Imported from a vocabulary: not checked for use by the document.
    shared: bool,
    /// Whether any document has used it; shared with the copies imported into documents.
    seen: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct Operator {
    name: String,
    arity: usize,
    apply: OperatorFn,
    location: Location,
    shared: bool,
    seen: Rc<Cell<bool>>,
}

/// What a registered name refers to.
#[derive(Clone, Copy)]
enum Slot {
    Entry(usize),
    Operator(usize),
}

/// Name resolution during rendering; records which names the template uses.
struct DocumentEnv<'a> {
    document: &'a Document,
    used_entries: HashSet<usize>,
    used_operators: HashSet<usize>,
}

impl Env for DocumentEnv<'_> {
    fn resolve(&mut self, name: &str) -> Resolved {
        match self.document.vocabulary.names.get(name) {
            Some(&Slot::Entry(i)) => {
                self.used_entries.insert(i);
                Resolved::Value(self.document.vocabulary.entries[i].value.clone())
            }
            Some(&Slot::Operator(i)) => {
                self.used_operators.insert(i);
                let operator = &self.document.vocabulary.operators[i];
                Resolved::Operator {
                    arity: operator.arity,
                    apply: operator.apply.clone(),
                }
            }
            None => Resolved::Undefined,
        }
    }
}

/// A template together with the symbols and expressions it may refer to.
///
/// Symbols, expressions and operators are registered with [`Document::symbol`],
/// [`Document::def`] and [`Document::operator`] (creation and registration are one step,
/// so everything takes part in the checks).
/// [`Document::render`] checks everything, reports all problems at once, and returns the
/// Markdown as a string; reading the template and writing the result are up to the caller.
pub struct Document {
    template: String,
    vocabulary: Vocabulary,
    header: bool,
}

/// Symbols, expressions and operators registered once and shared by several documents
/// (see [`Document::import`]). Registered the same way as in a document.
pub struct Vocabulary {
    entries: Vec<Entry>,
    operators: Vec<Operator>,
    /// Every registered name, whether it is a value or an operator.
    names: HashMap<String, Slot>,
    /// Problems found while registering (duplicate or invalid names).
    registration_errors: Vec<Diagnostic>,
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self::new()
    }
}

impl Vocabulary {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            operators: Vec::new(),
            names: HashMap::new(),
            registration_errors: Vec::new(),
        }
    }

    /// Reports every definition that no document has used. Call it after all the documents
    /// that import this vocabulary have been rendered successfully (a document that failed may
    /// not have recorded all its uses).
    pub fn check_unused(&self) -> Result<(), Error> {
        let entries = self.entries.iter().map(|e| (&e.name, &e.location, &e.seen));
        let operators = self
            .operators
            .iter()
            .map(|o| (&o.name, &o.location, &o.seen));
        let diagnostics: Vec<Diagnostic> = entries
            .chain(operators)
            .filter(|(_, _, seen)| !seen.get())
            .map(|(name, location, _)| Diagnostic {
                location: location.clone(),
                kind: DiagnosticKind::UnusedDefinition { name: name.clone() },
            })
            .collect();
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(Error::new(diagnostics))
        }
    }

    /// Registers a symbol: `name` is used in templates, `latex` in the output.
    #[track_caller]
    pub fn symbol(&mut self, name: &str, latex: &str) -> Expr {
        let value = Expr::from_symbol(Symbol::new(name, latex));
        self.register(name, &value, std::panic::Location::caller());
        value
    }

    /// Registers a named expression and returns it for further use in Rust code.
    #[track_caller]
    pub fn def(&mut self, name: &str, expr: impl AsRef<Expr>) -> Expr {
        let value = Expr::named(name, expr.as_ref());
        self.register(name, &value, std::panic::Location::caller());
        value
    }

    /// Registers an operator callable as `name(arg, ...)` in templates. The number of
    /// arguments is checked against `arity` before `apply` is called.
    #[track_caller]
    pub fn operator(
        &mut self,
        name: &str,
        arity: usize,
        apply: impl Fn(&[Expr]) -> Expr + 'static,
    ) {
        let slot = Slot::Operator(self.operators.len());
        let location = Location::from_caller(std::panic::Location::caller());
        if self.check_name(name, &location) {
            self.names.insert(name.to_string(), slot);
            self.operators.push(Operator {
                name: name.to_string(),
                arity,
                apply: std::rc::Rc::new(apply),
                location,
                shared: false,
                seen: Rc::new(Cell::new(false)),
            });
        }
    }

    fn register(&mut self, name: &str, value: &Expr, caller: &std::panic::Location<'_>) {
        let location = Location::from_caller(caller);
        if self.check_name(name, &location) {
            self.names
                .insert(name.to_string(), Slot::Entry(self.entries.len()));
            self.entries.push(Entry {
                name: name.to_string(),
                value: value.clone(),
                location,
                shared: false,
                seen: Rc::new(Cell::new(false)),
            });
        }
    }

    /// Returns whether `name` may be registered; otherwise records why not.
    fn check_name(&mut self, name: &str, location: &Location) -> bool {
        let kind = if !name::is_valid(name) {
            DiagnosticKind::InvalidName {
                name: name.to_string(),
            }
        } else if let Some(&slot) = self.names.get(name) {
            let first = match slot {
                Slot::Entry(i) => self.entries[i].location.clone(),
                Slot::Operator(i) => self.operators[i].location.clone(),
            };
            DiagnosticKind::DuplicateName {
                name: name.to_string(),
                first,
            }
        } else {
            return true;
        };
        self.registration_errors.push(Diagnostic {
            location: location.clone(),
            kind,
        });
        false
    }
}

impl Document {
    /// Creates a document from the text of a template. Positions in the template are
    /// reported as `line:col`.
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            vocabulary: Vocabulary::new(),
            header: true,
        }
    }

    /// Enables or disables the "generated by komainu" header comment (enabled by default).
    pub fn header(&mut self, enabled: bool) {
        self.header = enabled;
    }

    /// Registers a symbol: `name` is used in templates, `latex` in the output.
    #[track_caller]
    pub fn symbol(&mut self, name: &str, latex: &str) -> Expr {
        self.vocabulary.symbol(name, latex)
    }

    /// Registers a named expression and returns it for further use in Rust code.
    #[track_caller]
    pub fn def(&mut self, name: &str, expr: impl AsRef<Expr>) -> Expr {
        self.vocabulary.def(name, expr)
    }

    /// Registers an operator callable as `name(arg, ...)` in templates. The number of
    /// arguments is checked against `arity` before `apply` is called.
    #[track_caller]
    pub fn operator(
        &mut self,
        name: &str,
        arity: usize,
        apply: impl Fn(&[Expr]) -> Expr + 'static,
    ) {
        self.vocabulary.operator(name, arity, apply);
    }

    /// Makes everything in `vocabulary` available to this document. The names share the
    /// document's namespace (a clash is an error), and what is imported is not required to be
    /// used: a vocabulary serves several documents, each using a part of it.
    pub fn import(&mut self, vocabulary: &Vocabulary) {
        let own = &mut self.vocabulary;
        own.registration_errors
            .extend(vocabulary.registration_errors.iter().cloned());
        for entry in &vocabulary.entries {
            if own.check_name(&entry.name, &entry.location) {
                own.names
                    .insert(entry.name.clone(), Slot::Entry(own.entries.len()));
                own.entries.push(Entry {
                    shared: true,
                    ..entry.clone()
                });
            }
        }
        for operator in &vocabulary.operators {
            if own.check_name(&operator.name, &operator.location) {
                own.names
                    .insert(operator.name.clone(), Slot::Operator(own.operators.len()));
                own.operators.push(Operator {
                    shared: true,
                    ..operator.clone()
                });
            }
        }
    }

    /// Checks the document and returns the expanded Markdown, or every problem found.
    pub fn render(&self) -> Result<String, Error> {
        let mut env = DocumentEnv {
            document: self,
            used_entries: HashSet::new(),
            used_operators: HashSet::new(),
        };
        let (body, template_diagnostics) = self.expand(&mut env);
        let used_entries = self.reachable_entries(env.used_entries);
        self.record_shared_use(&used_entries, &env.used_operators);
        let mut diagnostics = self.vocabulary.registration_errors.clone();
        diagnostics.extend(template_diagnostics);
        diagnostics.extend(self.unused(&used_entries, &env.used_operators));
        if !diagnostics.is_empty() {
            return Err(Error::new(diagnostics));
        }
        let header = if self.header {
            "<!-- generated by komainu; do not edit -->\n".to_string()
        } else {
            String::new()
        };
        Ok(header + &body)
    }

    fn location_at(&self, offset: usize) -> Location {
        let (line, col) = template::line_col(&self.template, offset);
        Location::Template { line, col }
    }

    /// Replaces the placeholders. Returns the result (meaningful only if there is no
    /// diagnostic) and the template's problems in file order.
    fn expand(&self, env: &mut DocumentEnv<'_>) -> (String, Vec<Diagnostic>) {
        let parsed = template::parse(&self.template);
        let mut found: Vec<Problem> = parsed.errors;
        let mut output = String::new();
        for segment in &parsed.segments {
            match segment {
                Segment::Text(text) => output.push_str(text),
                Segment::Placeholder { content, offset } => match parser::parse(content, env) {
                    Ok(expr) => {
                        // Names an operator brought in (a symbol it captured) are used too.
                        self.mark_used(&expr, &mut env.used_entries);
                        output.push_str(&expr.to_string());
                    }
                    Err(problems) => found.extend(problems.into_iter().map(|p| Problem {
                        offset: offset + p.offset,
                        kind: p.kind,
                    })),
                },
            }
        }
        found.sort_by_key(|problem| problem.offset);
        let diagnostics = found
            .into_iter()
            .map(|problem| Diagnostic {
                location: self.location_at(problem.offset),
                kind: problem.kind,
            })
            .collect();
        (output, diagnostics)
    }

    /// Everything reachable from a used entry is used too.
    fn reachable_entries(&self, mut used: HashSet<usize>) -> HashSet<usize> {
        let mut pending: Vec<usize> = used.iter().copied().collect();
        while let Some(i) = pending.pop() {
            let mut found = HashSet::new();
            self.mark_used(&self.vocabulary.entries[i].value, &mut found);
            for j in found {
                if used.insert(j) {
                    pending.push(j);
                }
            }
        }
        used
    }

    /// Adds the entries that `expr` refers to (symbols and named definitions) to `used`.
    fn mark_used(&self, expr: &Expr, used: &mut HashSet<usize>) {
        expr.walk(&mut |node| {
            let name = match node {
                NodeRef::Symbol(symbol) => symbol.name(),
                NodeRef::Named(name) => name,
            };
            if let Some(&Slot::Entry(i)) = self.vocabulary.names.get(name) {
                used.insert(i);
            }
        });
    }

    /// Tells the vocabularies which of what they provided has been used.
    fn record_shared_use(&self, used_entries: &HashSet<usize>, used_operators: &HashSet<usize>) {
        let entries = used_entries
            .iter()
            .map(|&i| &self.vocabulary.entries[i].seen);
        let operators = used_operators
            .iter()
            .map(|&i| &self.vocabulary.operators[i].seen);
        for seen in entries.chain(operators) {
            seen.set(true);
        }
    }

    fn unused(
        &self,
        used_entries: &HashSet<usize>,
        used_operators: &HashSet<usize>,
    ) -> Vec<Diagnostic> {
        let entries = self
            .vocabulary
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| !e.shared && !used_entries.contains(i))
            .map(|(_, e)| (&e.name, &e.location));
        let operators = self
            .vocabulary
            .operators
            .iter()
            .enumerate()
            .filter(|(i, o)| !o.shared && !used_operators.contains(i))
            .map(|(_, o)| (&o.name, &o.location));
        entries
            .chain(operators)
            .map(|(name, location)| Diagnostic {
                location: location.clone(),
                kind: DiagnosticKind::UnusedDefinition { name: name.clone() },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(result: Result<String, Error>) -> Vec<String> {
        result
            .expect_err("expected diagnostics")
            .diagnostics()
            .iter()
            .map(|d| d.to_string())
            .collect()
    }

    #[test]
    fn expands_symbols_and_definitions() {
        let mut doc = Document::new("$ {{ u }} $ and\n```math\n{{ dudt }}\n```\n");
        let u = doc.symbol("u", r"\vec{u}");
        let t = doc.symbol("t", "t");
        doc.def("dudt", Expr::partial_derivative(&u, &t));
        assert_eq!(
            doc.render().unwrap(),
            "<!-- generated by komainu; do not edit -->\n\
             $ {\\vec{u}} $ and\n```math\n\\frac{\\partial {\\vec{u}}}{\\partial {t}}\n```\n"
        );
    }

    #[test]
    fn header_can_be_disabled() {
        let mut doc = Document::new("{{ a }}");
        doc.symbol("a", "a");
        doc.header(false);
        assert_eq!(doc.render().unwrap(), "{a}");
    }

    #[test]
    fn latex_braces_pass_through() {
        let mut doc = Document::new(r"\epsilon_{{ijk}} {{{ a }}}");
        doc.symbol("a", "a");
        doc.header(false);
        assert_eq!(doc.render().unwrap(), r"\epsilon_{{ijk}} {{a}}");
    }

    #[test]
    fn undefined_identifier_has_position() {
        let mut doc = Document::new("line1\nab {{ nope }}\n");
        doc.symbol("a", "a");
        let m = messages(doc.render());
        assert!(m.contains(&"2:7: error: undefined identifier `nope`".to_string()));
    }

    #[test]
    fn unused_definitions_are_errors_with_definition_site() {
        let mut doc = Document::new("{{ a }}");
        doc.symbol("a", "a");
        doc.symbol("b", "b");
        let line = line!() - 1;
        let error = doc.render().unwrap_err();
        let errors = error.diagnostics();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].kind,
            DiagnosticKind::UnusedDefinition { name: "b".into() }
        );
        match &errors[0].location {
            Location::Source {
                file, line: actual, ..
            } => {
                assert!(file.ends_with("document.rs"));
                assert_eq!(*actual, line);
            }
            other => panic!("expected a position in the Rust code, got {other:?}"),
        }
    }

    fn vocabulary() -> Vocabulary {
        let mut vocabulary = Vocabulary::new();
        let x = vocabulary.symbol("x", "x");
        vocabulary.def("twice", &x + &x);
        vocabulary.operator("sq", 1, |args| args[0].pow(Expr::integer(2)));
        vocabulary
    }

    #[test]
    fn an_imported_vocabulary_serves_several_documents() {
        let mut first = Document::new("{{ sq(x) }}");
        first.import(&vocabulary());
        first.header(false);
        let mut second = Document::new("{{ twice }}");
        second.import(&vocabulary());
        second.header(false);
        assert_eq!(first.render().unwrap(), "{{{x}}^{2}}");
        assert_eq!(second.render().unwrap(), "{x} + {x}");
    }

    #[test]
    fn a_vocabulary_reports_what_no_document_used() {
        let vocabulary = vocabulary();
        let mut first = Document::new("{{ sq(x) }}");
        first.import(&vocabulary);
        // `twice` is used by no document; `x` and `sq` are used by the first
        first.render().unwrap();
        let messages = messages_of(&vocabulary.check_unused().unwrap_err());
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("`twice` is defined but never used"));
        // a second document, even if the first is gone, completes the use
        drop(first);
        let mut second = Document::new("{{ twice }}");
        second.import(&vocabulary);
        second.render().unwrap();
        assert!(vocabulary.check_unused().is_ok());
    }

    fn messages_of(error: &Error) -> Vec<String> {
        error.diagnostics().iter().map(|d| d.to_string()).collect()
    }

    #[test]
    fn what_is_imported_need_not_be_used() {
        let mut doc = Document::new("");
        doc.import(&vocabulary());
        assert!(doc.render().is_ok());
    }

    #[test]
    fn what_a_document_defines_itself_still_has_to_be_used() {
        let mut doc = Document::new("{{ x }}");
        doc.import(&vocabulary());
        doc.symbol("y", "y");
        assert_eq!(
            messages(doc.render())
                .iter()
                .filter(|m| m.contains("never used"))
                .count(),
            1
        );
    }

    #[test]
    fn a_name_clash_with_an_imported_name_is_an_error() {
        let mut doc = Document::new("{{ x }}");
        doc.symbol("x", "z");
        doc.import(&vocabulary());
        let messages = messages(doc.render());
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`x` is already defined"))
        );
    }

    #[test]
    fn a_problem_in_a_vocabulary_is_reported_by_the_document() {
        let mut vocabulary = Vocabulary::new();
        vocabulary.symbol("Bad", "b");
        let mut doc = Document::new("");
        doc.import(&vocabulary);
        let messages = messages(doc.render());
        assert!(messages.iter().any(|m| m.contains("invalid name `Bad`")));
    }

    #[test]
    fn a_symbol_an_operator_brings_in_counts_as_used() {
        let mut doc = Document::new("{{ f(1) }}");
        let f = doc.symbol("func", "f");
        doc.operator("f", 1, move |args| Expr::function(&f, args));
        doc.header(false);
        assert_eq!(doc.render().unwrap(), r"{f} \left( 1 \right)");
    }

    #[test]
    fn symbols_and_definitions_reachable_from_used_entries_count_as_used() {
        let mut doc = Document::new("{{ top }}");
        let a = doc.symbol("a", "a");
        let b = doc.symbol("b", "b");
        let inner = doc.def("inner", &a + &b);
        doc.def("top", &inner * &a);
        doc.header(false);
        assert_eq!(doc.render().unwrap(), r"\left( {a} + {b} \right) {a}");
    }

    #[test]
    fn duplicate_names_report_both_locations() {
        let mut doc = Document::new("{{ a }}");
        doc.symbol("a", "a");
        doc.symbol("a", "other");
        let m = messages(doc.render());
        assert_eq!(m.len(), 1);
        assert!(
            m[0].contains("error: `a` is already defined at "),
            "{}",
            m[0]
        );
    }

    #[test]
    fn invalid_names_are_rejected() {
        let mut doc = Document::new("");
        doc.symbol("A", "A");
        doc.symbol("1a", "a");
        assert_eq!(messages(doc.render()).len(), 2);
    }

    #[test]
    fn all_problems_are_reported_together() {
        let mut doc = Document::new("{{ x }} {{ a + b }} {{ y}} {{ z }}");
        doc.symbol("z", "z");
        doc.symbol("unused", "u");
        let m = messages(doc.render());
        // undefined x, undefined a, undefined b, syntax (missing space), unused
        assert_eq!(m.len(), 5, "{m:#?}");
    }

    #[test]
    fn diagnostic_exposes_a_location_and_a_message() {
        let doc = Document::new("a\n  {{ nope }}");
        let error = doc.render().unwrap_err();
        let diagnostic = &error.diagnostics()[0];
        assert_eq!(
            diagnostic.location(),
            &Location::Template { line: 2, col: 6 }
        );
        assert_eq!(diagnostic.message(), "undefined identifier `nope`");
        assert_eq!(
            diagnostic.to_string(),
            "2:6: error: undefined identifier `nope`"
        );
    }

    #[test]
    fn error_lists_every_diagnostic_and_counts_them() {
        let mut doc = Document::new("{{ a }} {{ b }}");
        doc.symbol("c", "c");
        let error = doc.render().unwrap_err();
        assert_eq!(error.diagnostics().len(), 3);
        let text = error.to_string();
        assert!(text.contains("1:4: error: undefined identifier `a`\n"));
        assert!(text.ends_with("\n\n3 errors found"), "{text}");
        assert_eq!(format!("{error:?}"), text);
    }

    #[test]
    fn expressions_are_expanded_and_positions_are_file_positions() {
        let mut doc = Document::new("text\n$${{\n  a +\n  nope }}$$ {{ a + b }}");
        doc.symbol("a", "a");
        doc.symbol("b", "b");
        let m = messages(doc.render());
        assert_eq!(m, vec!["4:3: error: undefined identifier `nope`"]);
    }

    #[test]
    fn names_after_a_syntax_error_count_as_used() {
        // `b` and `c` are not reached by the parser, but they are not reported as unused;
        // `zz` is reported as undefined.
        let mut doc = Document::new("{{ a b c zz }}");
        for name in ["a", "b", "c"] {
            doc.symbol(name, name);
        }
        let m = messages(doc.render());
        assert_eq!(m.len(), 2, "{m:?}");
        assert!(m[0].contains("expected an operator, found `b`"));
        assert!(m[1].contains("undefined identifier `zz`"));
    }

    #[test]
    fn symbols_inside_a_sum_count_as_used() {
        let mut doc = Document::new("{{ total }}");
        let k = doc.symbol("k", "k");
        let n = doc.symbol("n", "N");
        let a = doc.symbol("a", "a_k");
        doc.def(
            "total",
            Expr::sum_between(&k, Expr::integer(0), &n - Expr::integer(1), &a),
        );
        doc.header(false);
        assert_eq!(doc.render().unwrap(), r"{\sum_{{k} = 0}^{{N} - 1} {a_k}}");
    }

    #[test]
    fn symbols_inside_explicit_parentheses_count_as_used() {
        let mut doc = Document::new("{{ x }}");
        let a = doc.symbol("a", "a");
        let b = doc.symbol("b", "b");
        doc.def("x", Expr::paren(&a + &b));
        doc.header(false);
        assert_eq!(doc.render().unwrap(), r"\left( {a} + {b} \right)");
    }

    #[test]
    fn the_base_is_defined_once_and_the_template_picks_the_index_and_the_label() {
        let mut doc = Document::new(
            "{{ sub(x, i) }} {{ sub(x, j + 1) }} {{ sup(sub(x, i), n + 1) }} {{ sup(x, star) }}",
        );
        for (name, latex) in [
            ("x", r"\vec{x}"),
            ("i", "i"),
            ("j", "j"),
            ("n", "n"),
            ("star", "*"),
        ] {
            doc.symbol(name, latex);
        }
        doc.operator("sub", 2, |args| Expr::subscript(&args[0], &args[1]));
        doc.operator("sup", 2, |args| Expr::superscript(&args[0], &args[1]));
        doc.header(false);
        assert_eq!(
            doc.render().unwrap(),
            r"{{\vec{x}}_{{i}}} {{\vec{x}}_{{j} + 1}} {{{\vec{x}}_{{i}}}^{{n} + 1}} {{\vec{x}}^{{*}}}"
        );
    }

    #[test]
    fn symbols_inside_a_subscript_count_as_used_and_the_others_do_not() {
        let mut doc = Document::new("{{ x_i }}");
        let x = doc.symbol("x", "x");
        let i = doc.symbol("i", "i");
        doc.symbol("j", "j");
        doc.def("x_i", Expr::subscript(&x, &i));
        let m = messages(doc.render());
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("`j` is defined but never used"), "{m:?}");
    }

    #[test]
    fn operators_are_called_from_the_template() {
        let mut doc = Document::new("{{ pair(a, neg(b)) }}");
        let a = doc.symbol("a", "a");
        let b = doc.symbol("b", "b");
        doc.operator("neg", 1, |args| -&args[0]);
        doc.operator("pair", 2, |args| Expr::fraction(&args[0], &args[1]));
        doc.header(false);
        assert_eq!(doc.render().unwrap(), Expr::fraction(&a, -&b).to_string());
    }

    #[test]
    fn operator_argument_count_is_checked_at_the_operator_name() {
        let mut doc = Document::new("{{ pair(a) }}");
        doc.symbol("a", "a");
        doc.operator("pair", 2, |args| &args[0] + &args[1]);
        assert_eq!(
            messages(doc.render()),
            vec!["1:4: error: operator `pair` takes 2 arguments, found 1"]
        );
    }

    #[test]
    fn a_malformed_call_still_counts_the_operator_and_its_arguments_as_used() {
        let mut doc = Document::new("{{ pair(a b) }}");
        doc.symbol("a", "a");
        doc.symbol("b", "b");
        doc.operator("pair", 2, |args| &args[0] + &args[1]);
        let m = messages(doc.render());
        assert_eq!(m.len(), 1, "{m:?}");
    }

    #[test]
    fn unused_and_duplicate_operators_are_errors() {
        let mut doc = Document::new("{{ a }}");
        doc.symbol("a", "a");
        doc.operator("unused", 1, |args| args[0].clone());
        doc.operator("a", 1, |args| args[0].clone());
        let m = messages(doc.render());
        assert_eq!(m.len(), 2, "{m:?}");
        assert!(m[0].contains("`a` is already defined at "));
        assert!(m[1].contains("`unused` is defined but never used"));
    }
}
