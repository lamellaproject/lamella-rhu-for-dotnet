//! Hands the selected board's linker script to the linker by its exact,
//! distinct name. GNU-style linkers resolve `-T <name>` against the
//! working directory before any search path, so script names must never
//! collide with another file -- no file named `link.x` exists in this
//! tree. Exactly one board feature selects exactly one script
//! (src/board/mod.rs enforces the feature side).

/// A linker-script number: decimal or `0x` hexadecimal, with an optional
/// `K` or `M` suffix (the MEMORY lines' forms in this tree).
fn parse_ld_number(s: &str) -> Option<u64> {
    let (digits, scale) = match s.chars().last() {
        Some('K') | Some('k') => (&s[..s.len() - 1], 1024u64),
        Some('M') | Some('m') => (&s[..s.len() - 1], 1024u64 * 1024),
        _ => (s, 1u64),
    };
    let v = if let Some(h) = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()?
    } else {
        digits.parse::<u64>().ok()?
    };
    Some(v * scale)
}

fn main() {
    let script = if cfg!(feature = "board-mps2-an505") {
        "link-mps2-an505.x"
    } else if cfg!(feature = "board-adafruit-feather-rp2350-hstx") {
        "link-adafruit-feather-rp2350-hstx.x"
    } else if cfg!(feature = "board-adafruit-feather-rp2350-hstx-psram") {
        "link-adafruit-feather-rp2350-hstx-psram.x"
    } else if cfg!(feature = "board-pimoroni-pico-lipo-2-xl-w") {
        "link-pimoroni-pico-lipo-2-xl-w.x"
    } else if cfg!(feature = "board-pimoroni-pico-lipo-2") {
        "link-pimoroni-pico-lipo-2.x"
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
    } else if cfg!(feature = "board-arduino-giga-r1-wifi") {
        "link-arduino-giga-r1-wifi.x"
    } else if cfg!(feature = "board-arduino-portenta-h7") {
        "link-arduino-portenta-h7.x"
    } else if cfg!(feature = "board-arduino-portenta-h7-lite") {
        "link-arduino-portenta-h7-lite.x"
    } else if cfg!(feature = "board-arduino-portenta-h7-lite-connected") {
        "link-arduino-portenta-h7-lite-connected.x"
    } else if cfg!(feature = "board-arduino-portenta-machine-control") {
        "link-arduino-portenta-machine-control.x"
    } else {
        "link-mps2-an500.x"
    };
    println!("cargo:rustc-link-arg=-T{script}");
    println!("cargo:rerun-if-changed={script}");
    // The console's rows. The ST rows' wire is the probe's
    // port driven the ring way (the wire ring); the emulator rows' default
    // wire is the polled CMSDK UART0; a UART wire of the Pico and emulator
    // rows is a port of the UART doors' module.
    println!("cargo:rustc-check-cfg=cfg(rhu_console_probe)");
    println!("cargo:rustc-check-cfg=cfg(rhu_uart_rows)");
    // A row with a network medium under the `net` feature -- the
    // emulator rows' virtual medium.
    println!("cargo:rustc-check-cfg=cfg(rhu_net_rows)");
    if cfg!(all(feature = "net", any(feature = "board-mps2-an500", feature = "board-mps2-an505"))) {
        println!("cargo:rustc-cfg=rhu_net_rows");
    }
    println!("cargo:rustc-check-cfg=cfg(rhu_uart_pl011)");
    println!("cargo:rustc-check-cfg=cfg(rhu_uart_cmsdk)");
    // The rows with a UART table: the Pico rows over the PL011
    // backend, the emulator rows over the CMSDK backend; the ST rows carry
    // none, so the UART doors answer RHU_ENODEV there.
    if cfg!(any(
        feature = "board-adafruit-feather-rp2350-hstx",
        feature = "board-adafruit-feather-rp2350-hstx-psram",
        feature = "board-pimoroni-pico-lipo-2",
        feature = "board-pimoroni-pico-lipo-2-xl-w",
        feature = "board-pimoroni-pico-plus-2",
        feature = "board-pimoroni-pico-plus-2-w",
        feature = "board-rpi-pico2",
        feature = "board-rpi-pico2-w"
    )) {
        println!("cargo:rustc-cfg=rhu_uart_rows");
        println!("cargo:rustc-cfg=rhu_uart_pl011");
    }
    if cfg!(any(feature = "board-mps2-an500", feature = "board-mps2-an505")) {
        println!("cargo:rustc-cfg=rhu_uart_rows");
        println!("cargo:rustc-cfg=rhu_uart_cmsdk");
    }
    // The STM32H7 rows' probe channel is USART1 on the maker's console pins
    // driven the ring way too (the probe's own serial port reaches it by two
    // wires); their default is the memory console. The Machine Control row
    // compiles the same wire, and its legal channel list omits the probe
    // code (the pins are its carrier's transceiver mode lines), so a word
    // naming the wire takes the row's default there.
    if cfg!(any(
        feature = "board-st-nucleo-l4r5zi",
        feature = "board-st-nucleo-u5a5zj-q",
        feature = "board-st-stm32f746g-disco",
        feature = "board-st-stm32f769i-disco",
        feature = "board-arduino-giga-r1-wifi",
        feature = "board-arduino-portenta-h7",
        feature = "board-arduino-portenta-h7-lite",
        feature = "board-arduino-portenta-h7-lite-connected",
        feature = "board-arduino-portenta-machine-control"
    )) {
        println!("cargo:rustc-cfg=rhu_console_probe");
    }
    // The three link-time words of the console:
    // the kernel's own images take them from the environment at the cargo
    // build, as the SDK's targets pass them by --defsym from the project's
    // properties. Unset = the script's PROVIDE default (0 = the row's
    // default; the level's 0 = verbose). A word is given as its code or as
    // its name: the level silent | quiet | verbose; a channel off | memory |
    // probe | uart<n>.
    for (env, sym) in [
        ("RHU_LOG_LEVEL", "__rhu_log_level"),
        ("RHU_LOG_CHANNEL", "__rhu_log_channel"),
        ("RHU_CONSOLE_CHANNEL", "__rhu_console_channel"),
    ] {
        println!("cargo:rerun-if-env-changed={env}");
        let Ok(v) = std::env::var(env) else { continue };
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        let code: u32 = match v {
            "silent" => 1,
            "quiet" => 2,
            "verbose" => 3,
            "off" => 1,
            "memory" => 2,
            "probe" => 3,
            w if w.starts_with("uart") => 0x10 + w[4..].parse::<u32>().unwrap_or_else(|_| panic!("{env}: `{w}` is not uart<n>")),
            w => parse_ld_number(w).unwrap_or_else(|| panic!("{env}: `{w}` is neither a code nor a name")) as u32,
        };
        println!("cargo:rustc-link-arg=--defsym={sym}={code}");
    }
    // The handler stack's watermark word (stack_watermark.rs):
    // RHU_STACK_WATERMARK=1 or on in the environment turns the measurement on
    // for the kernel's own images through the same --defsym route; unset, 0
    // or off = the script's PROVIDE 0.
    println!("cargo:rerun-if-env-changed=RHU_STACK_WATERMARK");
    if let Ok(v) = std::env::var("RHU_STACK_WATERMARK") {
        let v = v.trim();
        if !v.is_empty() {
            let code: u32 = match v {
                "on" => 1,
                "off" => 0,
                w => parse_ld_number(w).unwrap_or_else(|| panic!("RHU_STACK_WATERMARK: `{w}` is not 0, 1, on or off")) as u32,
            };
            println!("cargo:rustc-link-arg=--defsym=__rhu_stack_watermark={code}");
        }
    }
    // The pin words of the UART rows (the hosting layer's encoding):
    // a GPIO number named in the environment becomes 0x100 | gpio (bit 8 the
    // valid flag); unset = the facts' pair. The kernel validates the pair
    // against the chip's legal pairs at the claim.
    for (env, sym) in [
        ("RHU_UART0_TX", "__rhu_uart0_tx"),
        ("RHU_UART0_RX", "__rhu_uart0_rx"),
        ("RHU_UART1_TX", "__rhu_uart1_tx"),
        ("RHU_UART1_RX", "__rhu_uart1_rx"),
    ] {
        println!("cargo:rerun-if-env-changed={env}");
        let Ok(v) = std::env::var(env) else { continue };
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        let gpio = parse_ld_number(v).unwrap_or_else(|| panic!("{env}: `{v}` is not a GPIO number"));
        assert!(gpio < 0x100, "{env}: `{v}` is not a GPIO number");
        println!("cargo:rustc-link-arg=--defsym={sym}={}", 0x100 | gpio);
    }
    let payload = script.replacen("link-", "payload-", 1);
    println!("cargo:rerun-if-changed={payload}");
    let mut ranges: Vec<String> = Vec::new();
    for path in [script, payload.as_str()] {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        for line in text.lines() {
            let Some((head, tail)) = line.split_once("ORIGIN =") else { continue };
            let Some((origin, length)) = tail.split_once("LENGTH =") else { continue };
            let name = head.trim().split_whitespace().next().unwrap_or("");
            let origin = origin.trim().trim_end_matches(',').trim();
            let length = length.trim().trim_end_matches(',').trim();
            let (Some(lo), Some(len)) = (parse_ld_number(origin), parse_ld_number(length)) else {
                panic!("{path}: cannot read the MEMORY line `{line}`");
            };
            ranges.push(format!("{name} {lo} {len}"));
        }
    }
    println!("cargo:rustc-env=RHU_LINK_RANGES={}", ranges.join(";"));
    // The architecture cfg the stack-limit code compiles under: the v8-M
    // mainline targets carry the limit registers; the v7-M rows compile
    // none of it.
    println!("cargo:rustc-check-cfg=cfg(armv8m)");
    if std::env::var("TARGET").map(|t| t.starts_with("thumbv8m")).unwrap_or(false) {
        println!("cargo:rustc-cfg=armv8m");
    }
    // The chip and family modules select their clock posture by one cfg
    // the board's posture feature implies, so a module serves every board
    // of its chip or family without naming them. The RP2350 rows' default
    // is the 150 MHz PLL posture; the cfg names the crystal posture a
    // board's `<board>-xtal12` feature selects.
    println!("cargo:rustc-check-cfg=cfg(rp2350_xtal)");
    if cfg!(any(
        feature = "adafruit-feather-rp2350-hstx-xtal12",
        feature = "adafruit-feather-rp2350-hstx-psram-xtal12",
        feature = "pimoroni-pico-lipo-2-xtal12",
        feature = "pimoroni-pico-lipo-2-xl-w-xtal12",
        feature = "pimoroni-pico-plus-2-xtal12",
        feature = "pimoroni-pico-plus-2-w-xtal12",
        feature = "rpi-pico2-xtal12",
        feature = "rpi-pico2-w-xtal12"
    )) {
        println!("cargo:rustc-cfg=rp2350_xtal");
    }
    // The rows with a PSRAM part (the two Pimoroni Pico Plus 2 rows, the Pico
    // LiPo 2 and the Feather RP2350 with PSRAM) carry 8 MB of PSRAM behind the flash
    // interface's second chip select. The window is brought up at the PLL
    // posture alone: at the 12 MHz crystal posture the part's chip-select
    // limit cannot be bounded with a cache line in flight (the PSRAM
    // crate's timing function refuses that posture), so the crystal images
    // of those rows build WITHOUT the window and report one RAM range.
    let psram_window = cfg!(all(
        any(
            feature = "board-pimoroni-pico-plus-2",
            feature = "board-pimoroni-pico-plus-2-w",
            feature = "board-pimoroni-pico-lipo-2",
            feature = "board-pimoroni-pico-lipo-2-xl-w",
            feature = "board-adafruit-feather-rp2350-hstx-psram"
        ),
        not(any(
            feature = "pimoroni-pico-plus-2-xtal12",
            feature = "pimoroni-pico-plus-2-w-xtal12",
            feature = "pimoroni-pico-lipo-2-xtal12",
            feature = "pimoroni-pico-lipo-2-xl-w-xtal12",
            feature = "adafruit-feather-rp2350-hstx-psram-xtal12"
        ))
    ));
    println!("cargo:rustc-check-cfg=cfg(rhu_psram_window)");
    if psram_window {
        println!("cargo:rustc-cfg=rhu_psram_window");
    }
    // The STM32F769 row carries 16 MB of SDRAM behind the chip's flexible
    // memory controller, brought up at both of the family's postures (the
    // controller's words derived per posture at compile time in
    // board/stm32f7_sdram.rs), so both image sets serve two RAM ranges.
    let sdram_window = cfg!(feature = "board-st-stm32f769i-disco");
    println!("cargo:rustc-check-cfg=cfg(rhu_sdram_window)");
    if sdram_window {
        println!("cargo:rustc-cfg=rhu_sdram_window");
    }
    // The STM32F769 row carries the board's 64 MB quad-SPI flash on the
    // chip's QUADSPI block, memory-mapped at 0x90000000 when the image's map
    // declares the window: the family's driver, the protection unit's
    // region, the instruction cache's boot step and the api flavor's cases
    // compile where the block is (both postures; one kernel), and the
    // bring-up runs by the map's word at boot.
    let qspi_window = cfg!(feature = "board-st-stm32f769i-disco");
    println!("cargo:rustc-check-cfg=cfg(rhu_qspi_window)");
    if qspi_window {
        println!("cargo:rustc-cfg=rhu_qspi_window");
    }
    // The two emulator rows carry the machines' 16 MB PSRAM bank (nothing
    // to program; proved at boot by the read-back walk), so both serve two
    // RAM ranges.
    let mps2_window = cfg!(any(feature = "board-mps2-an500", feature = "board-mps2-an505"));
    println!("cargo:rustc-check-cfg=cfg(rhu_mps2_window)");
    if mps2_window {
        println!("cargo:rustc-cfg=rhu_mps2_window");
    }
    // One cfg for every row with an external RAM range, whatever brings it
    // up: the arena's second window, the RAM-range doors' count, the
    // rehydrated-data region's zeroing and the api flavor's cases read it;
    // the bring-up sites keep their rows' cfgs.
    println!("cargo:rustc-check-cfg=cfg(rhu_extram_window)");
    if psram_window || sdram_window || mps2_window {
        println!("cargo:rustc-cfg=rhu_extram_window");
    }
    // The STM32F769 row's microSD socket on the chip's SDMMC2 controller --
    // the host, the card layer and the boot banner's sd line compile where
    // the socket is (both postures); every other row serves no block device
    // and the emulator images carry none of it. The doctored control (the
    // host capacity bit cleared in ACMD41's argument) exists only there.
    println!("cargo:rustc-check-cfg=cfg(rhu_sd_socket)");
    if cfg!(feature = "board-st-stm32f769i-disco") {
        println!("cargo:rustc-cfg=rhu_sd_socket");
    }
    if cfg!(feature = "test-sd-doctored") && !cfg!(feature = "board-st-stm32f769i-disco") {
        panic!("test-sd-doctored is the control of the STM32F769 row's card socket (the host capacity bit cleared in ACMD41's argument): select board-st-stm32f769i-disco beside it");
    }
    // The external RAM's init mode. A lazy feature compiles to its cfg
    // only where the image has an external window to be lazy about (the
    // Pimoroni rows at the PLL posture, the STM32F769 row at both postures,
    // the two emulator rows) or
    // where the api gate's test feature makes window 0 follow the mode; a
    // lazy feature anywhere else, both at once, or the doctored control
    // without a lazy mode refuses the build with its words -- never a
    // silent no-op.
    println!("cargo:rustc-check-cfg=cfg(rhu_ram_init_first_use)");
    println!("cargo:rustc-check-cfg=cfg(rhu_ram_init_background)");
    let external_window = psram_window || sdram_window || mps2_window;
    let lazy_window = external_window || cfg!(feature = "test-ram-init-window0");
    let first_use = cfg!(feature = "ram-init-first-use");
    let background = cfg!(feature = "ram-init-background");
    if first_use && background {
        panic!("ram-init-first-use and ram-init-background are two modes of one choice: select one (neither = init at boot)");
    }
    if (first_use || background) && !lazy_window {
        panic!(
            "{} asked on a row without an external RAM range (the init mode governs external RAM alone: the Pimoroni rows at the PLL posture, the STM32F769 row and the two emulator rows carry one; this build does not)",
            if first_use { "ram-init-first-use" } else { "ram-init-background" }
        );
    }
    if cfg!(feature = "test-ram-init-doctored") && !(first_use || background) {
        panic!("test-ram-init-doctored is the control of a lazy init mode: select ram-init-first-use or ram-init-background beside it");
    }
    // The background mode's control -- the idle step's memset put back
    // under the critical section -- exists only where the mode does.
    if cfg!(feature = "test-ram-init-doctored-section") && !background {
        panic!("test-ram-init-doctored-section is the control of the background init mode (the idle step's zeroing held under the critical section): select ram-init-background beside it");
    }
    if first_use {
        println!("cargo:rustc-cfg=rhu_ram_init_first_use");
    }
    if background {
        println!("cargo:rustc-cfg=rhu_ram_init_background");
    }
    // The STM32F7 family's PLL posture (216 MHz): the STM32F746 row's opt-in
    // (its `-pll216` feature; the 16 MHz internal oscillator otherwise) and
    // the STM32F769 row's default, cleared by its `-hsi16` feature alone -- the
    // STM32F769's `-pll216` feature names the default and stays accepted; the
    // two STM32F769 features together are refused.
    println!("cargo:rustc-check-cfg=cfg(stm32f7_pll216)");
    if cfg!(all(feature = "st-stm32f769i-disco-hsi16", feature = "st-stm32f769i-disco-pll216")) {
        panic!("st-stm32f769i-disco-hsi16 and st-stm32f769i-disco-pll216 name the row's two clock postures: select one (neither = the PLL posture, the row's default)");
    }
    if cfg!(feature = "st-stm32f746g-disco-pll216") || cfg!(all(feature = "board-st-stm32f769i-disco", not(feature = "st-stm32f769i-disco-hsi16"))) {
        println!("cargo:rustc-cfg=stm32f7_pll216");
    }
    // The STM32U5 row's PLL posture (PLL1 at 160 MHz off the 16 MHz internal
    // oscillator): the NUCLEO-U5A5ZJ-Q row's default, cleared by its `-hsi16`
    // feature alone -- the row's `-pll160` feature names the default and
    // stays accepted; the two features together are refused.
    println!("cargo:rustc-check-cfg=cfg(stm32u5_pll160)");
    if cfg!(all(feature = "st-nucleo-u5a5zj-q-hsi16", feature = "st-nucleo-u5a5zj-q-pll160")) {
        panic!("st-nucleo-u5a5zj-q-hsi16 and st-nucleo-u5a5zj-q-pll160 name the row's two clock postures: select one (neither = the PLL posture, the row's default)");
    }
    if cfg!(all(feature = "board-st-nucleo-u5a5zj-q", not(feature = "st-nucleo-u5a5zj-q-hsi16"))) {
        println!("cargo:rustc-cfg=stm32u5_pll160");
    }
    // The STM32H7 rows: the default image set is the 400 MHz PLL from the
    // board's external source; the cfg names the internal-oscillator
    // bring-up posture a board's `<board>-hsi64` feature selects. Their
    // reset vector names the family's register-only supply entry (the
    // chip's supply configuration must be written before any RAM write),
    // which the boot module selects through the second cfg.
    println!("cargo:rustc-check-cfg=cfg(stm32h7_hsi64)");
    if cfg!(any(
        feature = "arduino-giga-r1-wifi-hsi64",
        feature = "arduino-portenta-h7-hsi64",
        feature = "arduino-portenta-h7-lite-hsi64",
        feature = "arduino-portenta-h7-lite-connected-hsi64",
        feature = "arduino-portenta-machine-control-hsi64"
    )) {
        println!("cargo:rustc-cfg=stm32h7_hsi64");
    }
    println!("cargo:rustc-check-cfg=cfg(rhu_supply_stub)");
    if cfg!(any(
        feature = "board-arduino-giga-r1-wifi",
        feature = "board-arduino-portenta-h7",
        feature = "board-arduino-portenta-h7-lite",
        feature = "board-arduino-portenta-h7-lite-connected",
        feature = "board-arduino-portenta-machine-control"
    )) {
        println!("cargo:rustc-cfg=rhu_supply_stub");
    }
    println!("cargo:rustc-check-cfg=cfg(stm32l4plus_pll120)");
    if cfg!(feature = "st-nucleo-l4r5zi-pll120") {
        println!("cargo:rustc-cfg=stm32l4plus_pll120");
    }
    // The rows whose facts carry a bus compile the bus drivers; the rest
    // compile the kernel API's bus calls alone (the count 0). The emulator
    // rows among them carry register-level models of the blocks, and the api
    // flavor's model-backed bus checks run there alone.
    println!("cargo:rustc-check-cfg=cfg(rhu_bus_rows)");
    println!("cargo:rustc-check-cfg=cfg(rhu_bus_models)");
    if cfg!(any(feature = "board-mps2-an500", feature = "board-mps2-an505")) {
        println!("cargo:rustc-cfg=rhu_bus_rows");
        println!("cargo:rustc-cfg=rhu_bus_models");
    }
    if cfg!(any(feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f769i-disco")) {
        println!("cargo:rustc-cfg=rhu_bus_rows");
    }
    // The power-management chip's reading flavor of the four rows carrying
    // the Portenta H7 module: under it alone the row's table carries
    // the chip's I2C1, so the family bus drivers compile into that image and
    // into no shipped one.
    if cfg!(all(
        feature = "test-h7-pmic-read",
        any(feature = "board-arduino-portenta-h7", feature = "board-arduino-portenta-h7-lite", feature = "board-arduino-portenta-h7-lite-connected", feature = "board-arduino-portenta-machine-control")
    )) {
        println!("cargo:rustc-cfg=rhu_bus_rows");
    }
    // The four RP2350 rows serve the chip's I2C and SPI blocks their tables
    // name (one bus of each family a row) through their own backend and
    // drivers. The Cortex-M33 emulator row carries register-level models of
    // those blocks in place of the ST blocks' under the test feature
    // `test-bus-rp2350`, so the api gate proves the chip's drivers before a
    // board.
    println!("cargo:rustc-check-cfg=cfg(rhu_bus_rp2350)");
    if cfg!(any(
        feature = "board-adafruit-feather-rp2350-hstx",
        feature = "board-adafruit-feather-rp2350-hstx-psram",
        feature = "board-pimoroni-pico-lipo-2",
        feature = "board-pimoroni-pico-lipo-2-xl-w",
        feature = "board-pimoroni-pico-plus-2",
        feature = "board-pimoroni-pico-plus-2-w",
        feature = "board-rpi-pico2",
        feature = "board-rpi-pico2-w"
    )) {
        println!("cargo:rustc-cfg=rhu_bus_rows");
        println!("cargo:rustc-cfg=rhu_bus_rp2350");
    }
    if cfg!(all(feature = "board-mps2-an505", feature = "test-bus-rp2350")) {
        println!("cargo:rustc-cfg=rhu_bus_rp2350");
    }
    // The rows whose facts carry a PWM or an ADC table compile the doors'
    // served halves; the rest compile the count and enumeration doors alone
    // and refuse the others. The emulator rows carry the virtual models, and
    // the api flavor's model-backed checks run there alone; the RP2350 rows
    // serve the chip's slices and converter.
    println!("cargo:rustc-check-cfg=cfg(rhu_pwm_adc_rows)");
    println!("cargo:rustc-check-cfg=cfg(rhu_pwm_adc_models)");
    if cfg!(any(feature = "board-mps2-an500", feature = "board-mps2-an505")) {
        println!("cargo:rustc-cfg=rhu_pwm_adc_rows");
        println!("cargo:rustc-cfg=rhu_pwm_adc_models");
    }
    if cfg!(any(
        feature = "board-adafruit-feather-rp2350-hstx",
        feature = "board-adafruit-feather-rp2350-hstx-psram",
        feature = "board-pimoroni-pico-lipo-2",
        feature = "board-pimoroni-pico-lipo-2-xl-w",
        feature = "board-pimoroni-pico-plus-2",
        feature = "board-pimoroni-pico-plus-2-w",
        feature = "board-rpi-pico2",
        feature = "board-rpi-pico2-w"
    )) {
        println!("cargo:rustc-cfg=rhu_pwm_adc_rows");
    }
}
