//! Hands the board linker script to the linker by its exact, distinct
//! name. GNU-style linkers resolve `-T <name>` against the working
//! directory before any search path, so script names must never collide
//! with another file -- no file named `link.x` exists in this tree.

fn main() {
    let script = "link-mps2-an500.x";
    println!("cargo:rustc-link-arg=-T{script}");
    println!("cargo:rerun-if-changed={script}");
}
