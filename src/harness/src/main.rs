//! rhu-gate -- the RHU verification-tier gate runner.

mod chain;
mod gate;
mod gates;
mod image;
mod objfile;
mod results;
mod selfcheck;
mod sha256;
mod text;
mod tools;

use std::process::exit;

fn usage() -> ! {
    println!("usage: rhu-gate <base | sched | payload | api | gc | shim | rt-m7 | rt-m33 | host | chain | selfcheck> [selfcheck | rederive | sdk-parity | attrs <pre> <donor> <out>]");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = match tools::find_repo_root() {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    match (args.first().map(String::as_str), args.get(1).map(String::as_str), args.len()) {
        (Some("base"), None, 1) => gates::base::run(&repo),
        (Some("base"), Some("selfcheck"), 2) => gates::base::selfcheck(&repo),
        (Some("sched"), None, 1) => gates::sched::run(&repo),
        (Some("sched"), Some("selfcheck"), 2) => gates::sched::selfcheck(&repo),
        (Some("payload"), None, 1) => gates::payload::run(&repo),
        (Some("payload"), Some("selfcheck"), 2) => gates::payload::selfcheck(&repo),
        (Some("payload"), Some("rederive"), 2) => gates::payload::rederive(&repo),
        (Some("api"), None, 1) => gates::api::run(&repo),
        (Some("api"), Some("selfcheck"), 2) => gates::api::selfcheck(&repo),
        (Some("gc"), None, 1) => gates::gc::run(&repo),
        (Some("gc"), Some("selfcheck"), 2) => gates::gc::selfcheck(&repo),
        (Some("gc"), Some("rederive"), 2) => gates::gc::rederive(&repo),
        (Some("shim"), None, 1) => gates::shim::run(&repo),
        (Some("shim"), Some("selfcheck"), 2) => gates::shim::selfcheck(&repo),
        (Some("shim"), Some("rederive"), 2) => gates::shim::rederive(&repo),
        (Some("rt-m7"), None, 1) => gates::rt::run(&repo, &gates::rt::WORLD_M7),
        (Some("rt-m7"), Some("selfcheck"), 2) => gates::rt::selfcheck(&repo, &gates::rt::WORLD_M7),
        (Some("rt-m7"), Some("rederive"), 2) => gates::rt::rederive(&repo, &gates::rt::WORLD_M7),
        (Some("rt-m7"), Some("sdk-parity"), 2) => gates::rt::sdk_parity(&repo, &gates::rt::WORLD_M7),
        (Some("rt-m33"), None, 1) => gates::rt::run(&repo, &gates::rt::WORLD_M33),
        (Some("rt-m33"), Some("selfcheck"), 2) => gates::rt::selfcheck(&repo, &gates::rt::WORLD_M33),
        (Some("rt-m33"), Some("rederive"), 2) => gates::rt::rederive(&repo, &gates::rt::WORLD_M33),
        (Some("rt-m33"), Some("sdk-parity"), 2) => gates::rt::sdk_parity(&repo, &gates::rt::WORLD_M33),
        (Some("host"), None, 1) => gates::host::run(&repo),
        (Some("host"), Some("selfcheck"), 2) => gates::host::selfcheck(&repo),
        (Some("rt"), Some("attrs"), _) => gates::rt::attrs(&repo, &args[2..]),
        (Some("selfcheck"), None, 1) => selfcheck::run(&repo),
        (Some("chain"), None, 1) => chain::run(&repo),
        _ => usage(),
    }
}
