use crate::name;

/// Where a problem is (1-based line and column; the column counts characters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// A position in the template.
    Template { line: u32, col: u32 },
    /// A definition in the Rust code (recorded with `#[track_caller]`).
    Source { file: String, line: u32, col: u32 },
}

impl std::fmt::Display for Location {
    /// `line:col` in the template, `file:line:col` in the Rust code.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Template { line, col } => write!(f, "{line}:{col}"),
            Self::Source { file, line, col } => write!(f, "{file}:{line}:{col}"),
        }
    }
}

impl Location {
    /// The position of a call site (see `#[track_caller]`).
    pub(crate) fn from_caller(location: &std::panic::Location<'_>) -> Self {
        Self::Source {
            file: location.file().to_string(),
            line: location.line(),
            col: location.column(),
        }
    }
}

/// What is wrong. Not part of the public API: users get the position and the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiagnosticKind {
    /// A template refers to a name that is not registered.
    UndefinedIdentifier { name: String },
    /// A template placeholder is malformed.
    Syntax { message: String },
    /// A registered name is never used by the template (directly or through other definitions).
    UnusedDefinition { name: String },
    /// A name is registered twice.
    DuplicateName { name: String, first: Location },
    /// An operator is called with the wrong number of arguments.
    ArityMismatch {
        name: String,
        expected: usize,
        found: usize,
    },
    /// A name does not match `[a-z_][a-z0-9_]*`.
    InvalidName { name: String },
}

/// A problem found in the template, positioned by a byte offset into it. The parser and the
/// template scanner report these; `Document` turns the offset into a line and a column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Problem {
    pub offset: usize,
    pub kind: DiagnosticKind,
}

impl Problem {
    pub(crate) fn syntax(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            kind: DiagnosticKind::Syntax {
                message: message.into(),
            },
        }
    }
}

impl std::fmt::Display for DiagnosticKind {
    /// The message, without the position.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UndefinedIdentifier { name } => write!(f, "undefined identifier `{name}`"),
            Self::Syntax { message } => write!(f, "{message}"),
            Self::UnusedDefinition { name } => write!(f, "`{name}` is defined but never used"),
            Self::DuplicateName { name, first } => {
                write!(f, "`{name}` is already defined at {first}")
            }
            Self::ArityMismatch {
                name,
                expected,
                found,
            } => {
                let s = if *expected == 1 { "" } else { "s" };
                write!(
                    f,
                    "operator `{name}` takes {expected} argument{s}, found {found}"
                )
            }
            Self::InvalidName { name } => {
                write!(f, "invalid name `{name}`: expected `{}`", name::PATTERN)
            }
        }
    }
}

/// A problem found while checking a document. Every diagnostic is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub(crate) location: Location,
    pub(crate) kind: DiagnosticKind,
}

impl Diagnostic {
    /// Where the problem is: in the template, or at a definition in the Rust code.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// What is wrong, without the position.
    pub fn message(&self) -> String {
        self.kind.to_string()
    }
}

impl std::fmt::Display for Diagnostic {
    /// `file:line:col: error: message`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: error: {}", self.location, self.kind)
    }
}

/// The error of [`Document::render`](crate::Document::render): the document did not pass
/// the checks. It carries every problem found, in a stable order.
pub struct Error {
    diagnostics: Vec<Diagnostic>,
}

impl Error {
    pub(crate) fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }

    /// Every problem found.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, diagnostic) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{diagnostic}")?;
        }
        let n = self.diagnostics.len();
        write!(f, "\n\n{n} error{} found", if n == 1 { "" } else { "s" })
    }
}

/// `main` returning `Result<_, Box<dyn std::error::Error>>` prints this on failure,
/// so it matches `Display`.
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Error {}
