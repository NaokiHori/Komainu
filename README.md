# Komainu

Generates standalone Markdown from a template and symbol/expression definitions, intended for scientific documents with many equations.

## Features

- Define a symbol once in Rust; change its notation in one place
- Write expressions in the template (`{{ a + b = c }}`, `{{ partial(u, t) }}`) or build them in Rust
- Output contains no placeholders, so it reads as is on GitHub etc.
- Every problem (undefined, unused, duplicate, malformed) is an error, reported all at once
- No third-party dependency

## Quick Start

Add this crate to `Cargo.toml` of your document crate:

```toml
[dependencies]
komainu = { git = "https://github.com/NaokiHori/Komainu" }
```

then:

```rust
use komainu::{Document, Expr};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let template = std::fs::read_to_string("input.md")?;
    let mut doc = Document::new(template);
    let u = doc.symbol("u", r"\vec{u}");
    let t = doc.symbol("t", "t");
    doc.def("dudt", Expr::partial_derivative(&u, &t));
    let markdown = doc.render()?; // Err carries every diagnostic
    std::fs::write("output.md", markdown)?;
    Ok(())
}
```

`input.md`:

````markdown
```math
{{ dudt }} = {{ -u / 2 }}
```
````

This crate itself does no file I/O: it takes the template text and returns the Markdown as a string. Run with `cargo run`; on failure the diagnostics are printed and the exit code is non-zero.

- `examples/basics`: what can be written (start here)
- `examples/errors`: what is rejected, and how it is reported
- `examples/sample`: a realistic document

Template rules: `{{ ... }}` is a placeholder only if a space follows `{{` and precedes `}}`; otherwise it is plain LaTeX (e.g., `\epsilon_{{ijk}}`). Expressions support `+ - * / ^ =`, numbers, parentheses and calls of registered operators; `*` is required for multiplication. Parentheses you write are kept as written (use them to fix the order of evaluation); parentheses the meaning needs are added for you.

## Caveats

- Checks run at run time, not compile time.
- Raw LaTeX in the template text is not validated, and indices (`u_i`) get no special treatment: a subscripted symbol is just one symbol.
- Not thread-safe (`Rc`); intended for sequential document generation.
