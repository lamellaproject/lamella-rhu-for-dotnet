//! Hands the selected board's linker script to the linker by its exact,
//! distinct name. GNU-style linkers resolve `-T <name>` against the
//! working directory before any search path, so script names must never
//! collide with another file -- no file named `link.x` exists in this
//! tree. Exactly one board feature selects exactly one script
//! (src/board/mod.rs enforces the feature side).

fn main() {
    let script = if cfg!(feature = "board-stm32f769i-disco") {
        "link-stm32f769i-disco.x"
    } else if cfg!(feature = "board-mps2-an505") {
        "link-mps2-an505.x"
    } else {
        "link-mps2-an500.x"
    };
    println!("cargo:rustc-link-arg=-T{script}");
    println!("cargo:rerun-if-changed={script}");
}
