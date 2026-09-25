//! Hands the selected board's linker script to the linker by its exact,
//! distinct name. GNU-style linkers resolve `-T <name>` against the
//! working directory before any search path, so script names must never
//! collide with another file -- no file named `link.x` exists in this
//! tree. Exactly one board feature selects exactly one script
//! (src/board/mod.rs enforces the feature side).

fn main() {
    let script = if cfg!(feature = "board-mps2-an505") {
        "link-mps2-an505.x"
    } else if cfg!(feature = "board-pimoroni-pico-plus-2") {
        "link-pimoroni-pico-plus-2.x"
    } else if cfg!(feature = "board-pimoroni-pico-plus-2-w") {
        "link-pimoroni-pico-plus-2-w.x"
    } else if cfg!(feature = "board-rpi-pico2") {
        "link-rpi-pico2.x"
    } else if cfg!(feature = "board-rpi-pico2-w") {
        "link-rpi-pico2-w.x"
    } else if cfg!(feature = "board-st-nucleo-l4r5zi") {
        "link-st-nucleo-l4r5zi.x"
    } else if cfg!(feature = "board-st-nucleo-u5a5zj-q") {
        "link-st-nucleo-u5a5zj-q.x"
    } else if cfg!(feature = "board-st-stm32f746g-disco") {
        "link-st-stm32f746g-disco.x"
    } else if cfg!(feature = "board-st-stm32f769i-disco") {
        "link-st-stm32f769i-disco.x"
    } else {
        "link-mps2-an500.x"
    };
    println!("cargo:rustc-link-arg=-T{script}");
    println!("cargo:rerun-if-changed={script}");
    println!("cargo:rustc-check-cfg=cfg(armv8m)");
    if std::env::var("TARGET").map(|t| t.starts_with("thumbv8m")).unwrap_or(false) {
        println!("cargo:rustc-cfg=armv8m");
    }
    println!("cargo:rustc-check-cfg=cfg(rp2350_pll150)");
    if cfg!(any(
        feature = "pimoroni-pico-plus-2-pll150",
        feature = "pimoroni-pico-plus-2-w-pll150",
        feature = "rpi-pico2-pll150",
        feature = "rpi-pico2-w-pll150"
    )) {
        println!("cargo:rustc-cfg=rp2350_pll150");
    }
    println!("cargo:rustc-check-cfg=cfg(stm32f7_pll216)");
    if cfg!(any(feature = "st-stm32f746g-disco-pll216", feature = "st-stm32f769i-disco-pll216")) {
        println!("cargo:rustc-cfg=stm32f7_pll216");
    }
    println!("cargo:rustc-check-cfg=cfg(stm32l4plus_pll120)");
    if cfg!(feature = "st-nucleo-l4r5zi-pll120") {
        println!("cargo:rustc-cfg=stm32l4plus_pll120");
    }
    println!("cargo:rustc-check-cfg=cfg(rhu_bus_rows)");
    println!("cargo:rustc-check-cfg=cfg(rhu_bus_models)");
    if cfg!(any(feature = "board-mps2-an500", feature = "board-mps2-an505")) {
        println!("cargo:rustc-cfg=rhu_bus_rows");
        println!("cargo:rustc-cfg=rhu_bus_models");
    }
    if cfg!(any(feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f769i-disco")) {
        println!("cargo:rustc-cfg=rhu_bus_rows");
    }
}
