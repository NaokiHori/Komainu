//! Generates `out/basics.md` from `examples/basics/basics.md`.
//! Run from the repository root: `cargo run --example basics`.

use komainu::{Document, Expr};

const TEMPLATE: &str = "examples/basics/basics.md";

fn build(template: String) -> Document {
    let mut doc = Document::new(template);

    // A symbol: a name for the template, and the LaTeX notation for the output.
    // Creating and registering are one step; a symbol that is never used is an error.
    let velocity = doc.symbol("velocity", r"\vec{u}");
    let density = doc.symbol("density", r"\rho");
    let time = doc.symbol("time", "t");
    doc.symbol("pressure", "p");
    doc.symbol("index", "k");
    doc.symbol("count", "N");
    doc.symbol("weight", "w_k");
    // The base of a subscripted symbol is defined once; the template picks the index and the
    // label (a step, here), so that changing the notation of the base is a one-line change.
    doc.symbol("position", "X");
    doc.symbol("i", "i");
    doc.symbol("j", "j");
    doc.symbol("n", "n");

    // An expression built in Rust and given a name. `def` returns the expression,
    // so it can be reused in further Rust code.
    doc.def("momentum", &density * &velocity);
    doc.def("acceleration", Expr::partial_derivative(&velocity, &time));

    // An operator: callable from the template as `partial(x, y)`. The number of arguments
    // is checked, and an operator that is never used is an error, like everything else.
    doc.operator("partial", 2, |args| {
        Expr::partial_derivative(&args[0], &args[1])
    });
    doc.operator("sqrt", 1, |args| Expr::sqrt(&args[0]));
    // A function: its notation is a symbol, and the operator applies it to the arguments.
    // One operator per number of arguments; each argument is any expression.
    let heaviside = doc.symbol("heaviside", "H");
    doc.operator("step", 1, move |args| Expr::function(&heaviside, args));
    let field = doc.symbol("field", "f");
    doc.operator("f", 2, move |args| Expr::function(&field, args));
    doc.operator("sub", 2, |args| Expr::subscript(&args[0], &args[1]));
    doc.operator("sup", 2, |args| Expr::superscript(&args[0], &args[1]));
    // Sums have the same two forms as in the Rust API: what is summed over, or with a range.
    doc.operator("sum_over", 2, |args| Expr::sum_over(&args[0], &args[1]));
    doc.operator("sum_between", 4, |args| {
        Expr::sum_between(&args[0], &args[1], &args[2], &args[3])
    });

    doc
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let template = std::fs::read_to_string(TEMPLATE)?;
    let markdown = build(template).render()?;
    std::fs::create_dir_all("out")?;
    std::fs::write("out/basics.md", markdown)?;
    Ok(())
}
