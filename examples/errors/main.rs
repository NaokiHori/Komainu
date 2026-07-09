//! Shows the diagnostics for a template and definitions that contain mistakes.
//! Run from the repository root: `cargo run --example errors` (it exits with a non-zero code).

use komainu::{Document, Expr};

const TEMPLATE: &str = "examples/errors/errors.md";

fn build(template: String) -> Document {
    let mut doc = Document::new(template);
    doc.symbol("density", r"\rho");
    doc.symbol("pressure", "p");
    doc.symbol("velocity", r"\vec{u}");
    doc.symbol("time", "t");
    doc.operator("partial", 2, |args| {
        Expr::partial_derivative(&args[0], &args[1])
    });
    // A name registered twice.
    doc.symbol("density", r"\varrho");
    // A name that does not match `[a-z_][a-z0-9_]*`.
    doc.symbol("Bad Name", "x");
    // Defined, but never used by the template.
    doc.symbol("unused", "u");
    doc.operator("unused_operator", 1, |args| args[0].clone());
    doc
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let template = std::fs::read_to_string(TEMPLATE)?;
    let markdown = build(template).render()?;
    std::fs::create_dir_all("out")?;
    std::fs::write("out/errors.md", markdown)?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
