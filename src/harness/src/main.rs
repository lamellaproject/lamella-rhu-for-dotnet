//! rhu-gate -- the RHU verification-tier gate runner.

mod chain;
mod gate;
mod gates;
mod image;
mod results;
mod selfcheck;
mod sha256;
mod text;
mod tools;

use std::process::exit;

fn usage() -> ! {
    println!("usage: rhu-gate <base | sched | payload | api | gc | shim | rt | chain | selfcheck> [selfcheck | rederive]");
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
        (Some("api"), None, 1) => gates::api::run(&repo),
        (Some("api"), Some("selfcheck"), 2) => gates::api::selfcheck(&repo),
        (Some("gc"), None, 1) => gates::gc::run(&repo),
        (Some("gc"), Some("selfcheck"), 2) => gates::gc::selfcheck(&repo),
        (Some("gc"), Some("rederive"), 2) => gates::gc::rederive(&repo),
        (Some("shim"), None, 1) => gates::shim::run(&repo),
        (Some("shim"), Some("selfcheck"), 2) => gates::shim::selfcheck(&repo),
        (Some("shim"), Some("rederive"), 2) => gates::shim::rederive(&repo),
        (Some("rt"), None, 1) => gates::rt::run(&repo),
        (Some("rt"), Some("selfcheck"), 2) => gates::rt::selfcheck(&repo),
        (Some("rt"), Some("rederive"), 2) => gates::rt::rederive(&repo),
        (Some("selfcheck"), None, 1) => selfcheck::run(&repo),
        (Some("chain"), None, 1) => chain::run(&repo),
        _ => usage(),
    }
}
