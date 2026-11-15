//! The wired row's flight check, the flight flavor of its api image
//! (`test-net-flight`): the kernel's network API over the Ethernet
//! controller on a real wire, against a real DHCP server, a real name
//! server and two small peer programs on a bench machine. The image runs
//! this check alone. It finds its peer by one datagram to the limited
//! broadcast address, and the peer's answer names the leg to fly: the
//! first leg (the lookups, the echoes both ways, a caller accepted, a
//! closed port, the connect budget), the cable leg (the link dropped and
//! returned by a hand at the jack, under open sockets), or the soak (a
//! fixed cadence of echoes). No address is built into the image.
//!
//! THE MASK. This check prints no address: an IPv4 address is printed as a
//! fixed word and the order in which the boot first printed it, so that two
//! prints of one address carry one number; a hardware address is never
//! printed. The transcript of a flight can be kept without naming the
//! network it flew on.
//!
//! THE VERDICTS. Every case ends on one line: PASS, REFUSED (the row did
//! what the case forbids), SKIPPED (not flown, with the reason) or OBSERVED
//! (the row's behaviour is legal and the bench differed from what was
//! expected of it). A refusal does not stop the leg; the leg ends with its
//! summary, the END line and the exit function, 0 without a refusal.
//!
//! Every wait either blocks on a futex word with a deadline or sleeps a
//! millisecond between two tries: this thread is more urgent than the
//! network thread, and a spin would starve it.

use core::fmt;
use core::ptr::{addr_of, addr_of_mut, read_volatile};
use core::sync::atomic::Ordering;

use crate::board::stm32f7_eth::WITNESS as ETH;
use crate::{api, entropy, net, sched};

/// The peer's datagram port (the discovery, the echo, the call), the
/// board's own listener and its own datagram port.
const PEER_PORT: u16 = 47135;
const LISTEN_PORT: u16 = 47138;
const OUR_PORT: u16 = 47139;
/// The discard port, for the one connect to the address nobody uses.
const UNUSED_PORT: u16 = 9;
const ASK: &[u8; 16] = b"RHU-NET-FLIGHT?1";
const ANSWER: &[u8; 16] = b"RHU-NET-FLIGHT!1";
const CALL: &[u8; 16] = b"RHU-NET-FLIGHT>1";
const LETTERS: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
/// A public resolver's own name (two address records, these) and a name
/// under the reserved top-level name that never exists.
const NAME: &str = "dns.google";
const NAME_PAIR: [[u8; 4]; 2] = [[8, 8, 8, 8], [8, 8, 4, 4]];
const NO_NAME: &str = "rhu-net-flight.invalid";
const LEG_FIRST: u8 = 1;
const LEG_CABLE: u8 = 2;
const LEG_SOAK: u8 = 3;
const DEAD: i32 = (net::WRITABLE | net::ERROR | net::HANGUP) as i32;
const RESET: i32 = (net::READABLE | net::WRITABLE | net::ERROR | net::HANGUP) as i32;

static mut OUT: [u8; 3000] = [0; 3000];
static mut BACK: [u8; 3000] = [0; 3000];

fn out_buf() -> &'static mut [u8; 3000] {
    unsafe { &mut *addr_of_mut!(OUT) }
}

fn back_buf() -> &'static mut [u8; 3000] {
    unsafe { &mut *addr_of_mut!(BACK) }
}

/// An address's place in the boot's prints: 0 for no address.
struct Tag(usize);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            0 => f.write_str("<none>"),
            usize::MAX => f.write_str("<ip #more>"),
            n => write!(f, "<ip #{}>", n),
        }
    }
}

struct Flight {
    pass: u32,
    refused: u32,
    skipped: u32,
    observed: u32,
    tags: [u32; 12],
    tagged: usize,
}

impl Flight {
    fn tag(&mut self, ip: [u8; 4]) -> Tag {
        let w = u32::from_ne_bytes(ip);
        if w == 0 {
            return Tag(0);
        }
        for i in 0..self.tagged {
            if self.tags[i] == w {
                return Tag(i + 1);
            }
        }
        if self.tagged == self.tags.len() {
            return Tag(usize::MAX);
        }
        self.tags[self.tagged] = w;
        self.tagged += 1;
        Tag(self.tagged)
    }

    fn pass(&mut self, case: &str, what: fmt::Arguments) {
        self.pass += 1;
        kprintln!("[flight] PASS {} ({})", case, what);
    }

    fn refused(&mut self, case: &str, what: fmt::Arguments) {
        self.refused += 1;
        kprintln!("[flight] REFUSED {}: {}", case, what);
    }

    fn skipped(&mut self, case: &str, why: fmt::Arguments) {
        self.skipped += 1;
        kprintln!("[flight] SKIPPED {}: not flown: {}", case, why);
    }

    fn observed(&mut self, case: &str, what: fmt::Arguments) {
        self.observed += 1;
        kprintln!("[flight] OBSERVED {}: {}", case, what);
    }
}

/// What the peer's answer hands over.
struct Peer {
    ip: [u8; 4],
    leg: u8,
    closed_word: u8,
    unused: Option<[u8; 4]>,
    tcp_port: u16,
    closed_port: u16,
    soak_s: u32,
}

fn addr(ip: [u8; 4], port: u16) -> net::Addr {
    let mut a = net::Addr { family: net::FAMILY_IPV4, port: port as u32, addr: [0; 16], scope: 0 };
    a.addr[..4].copy_from_slice(&ip);
    a
}

fn none_addr() -> net::Addr {
    net::Addr { family: net::FAMILY_NONE, port: 0, addr: [0; 16], scope: 0 }
}

fn ip_of(a: &net::Addr) -> [u8; 4] {
    [a.addr[0], a.addr[1], a.addr[2], a.addr[3]]
}

fn num(o: [u8; 4]) -> u32 {
    u32::from_be_bytes(o)
}

fn witness() -> net::NetWitness {
    unsafe { read_volatile(addr_of!(net::WITNESS)) }
}

fn eth(c: &core::sync::atomic::AtomicU32) -> u32 {
    c.load(Ordering::Relaxed)
}

fn deadline_in(ms: u32) -> u64 {
    sched::now_ns() + ms as u64 * 1_000_000
}

fn since(t0_ms: u32) -> u32 {
    sched::now_ms().wrapping_sub(t0_ms)
}

/// The recipe on a socket's word until the mask has a bit of `want` (or an
/// error bit), or the deadline: the mask read.
fn wait_mask(h: i32, want: i32, deadline_ns: u64) -> i32 {
    loop {
        let m = api::rhu_net_readiness(h, 0);
        if m < 0 || m & (want | net::ERROR as i32) != 0 {
            return m;
        }
        let w = api::rhu_net_event_word(h);
        if w.is_null() {
            return sched::ERR_INVAL;
        }
        let v = unsafe { read_volatile(w) };
        let m = api::rhu_net_readiness(h, 0);
        if m < 0 || m & (want | net::ERROR as i32) != 0 {
            return m;
        }
        if api::rhu_futex_wait(w, v, deadline_ns) == sched::ERR_TIMEDOUT {
            return api::rhu_net_readiness(h, 0);
        }
    }
}

/// The recipe on any word until `done` answers true, or the deadline.
fn wait_until(w: *const u32, deadline_ns: u64, mut done: impl FnMut() -> bool) -> bool {
    loop {
        if done() {
            return true;
        }
        let v = unsafe { read_volatile(w) };
        if done() {
            return true;
        }
        if api::rhu_futex_wait(w, v, deadline_ns) == sched::ERR_TIMEDOUT {
            return done();
        }
    }
}

fn medium_link() -> u32 {
    let mut info = net::MediumInfo { kind: 9, link: 9, hw: [0; 6], reserved: 0 };
    if api::rhu_net_medium(0, &mut info) == 0 { info.link } else { 9 }
}

fn lease_now() -> Option<net::Lease> {
    let mut lease = net::Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 };
    if api::rhu_net_lease(0, &mut lease) == 0 { Some(lease) } else { None }
}

/// The wire-truth line: the driver's counters as they stand.
fn wire_truth(when: &str) {
    kprintln!(
        "[flight] wire ({}): frames {} out, {} in, {} errored, {} fragments, {} missed, {} transmit errors; link ups {}, downs {}, restarts {}; management timeouts {}, flush timeouts {}; interrupts {}, the last status {:#010x}; the last BMSR {:#06x}, register 31 {:#06x}",
        when,
        eth(&ETH.tx_frames),
        eth(&ETH.rx_frames),
        eth(&ETH.rx_errored),
        eth(&ETH.rx_fragments),
        eth(&ETH.rx_missed),
        eth(&ETH.tx_errors),
        eth(&ETH.link_ups),
        eth(&ETH.link_downs),
        eth(&ETH.restarts),
        eth(&ETH.mdio_timeouts),
        eth(&ETH.flush_timeouts),
        eth(&ETH.irqs),
        eth(&ETH.last_status),
        eth(&ETH.last_bmsr),
        eth(&ETH.last_scsr)
    );
}

/// Every byte of `data` taken by the stream, a millisecond's sleep between
/// two tries: 0, or the refusing code.
fn send_all(h: i32, data: &[u8], deadline_ns: u64) -> i32 {
    let mut sent = 0;
    while sent < data.len() {
        let r = api::rhu_net_send(h, data[sent..].as_ptr(), (data.len() - sent) as u32, core::ptr::null());
        if r > 0 {
            sent += r as usize;
        } else if r != sched::ERR_AGAIN {
            return r;
        } else if sched::now_ns() >= deadline_ns {
            return sched::ERR_TIMEDOUT;
        } else {
            sched::sleep_ms(1);
        }
    }
    0
}

/// The stream read until `buf` is full: 0, the peer's close as 1, or the
/// refusing code.
fn read_all(h: i32, buf: &mut [u8], deadline_ns: u64) -> i32 {
    let mut got = 0;
    while got < buf.len() {
        let r = api::rhu_net_recv(h, buf[got..].as_mut_ptr(), (buf.len() - got) as u32, 0, core::ptr::null_mut());
        if r > 0 {
            got += r as usize;
        } else if r == 0 {
            return 1;
        } else if r != sched::ERR_AGAIN {
            return r;
        } else if sched::now_ns() >= deadline_ns {
            return sched::ERR_TIMEDOUT;
        } else {
            sched::sleep_ms(1);
        }
    }
    0
}

/// The next datagram on the socket: its length and its sender, or the code.
fn read_datagram(d: i32, buf: &mut [u8], deadline_ns: u64) -> (i32, net::Addr) {
    let mut from = none_addr();
    loop {
        let r = api::rhu_net_recv(d, buf.as_mut_ptr(), buf.len() as u32, 0, &mut from);
        if r != sched::ERR_AGAIN {
            return (r, from);
        }
        if sched::now_ns() >= deadline_ns {
            return (sched::ERR_TIMEDOUT, from);
        }
        sched::sleep_ms(1);
    }
}

fn pattern(buf: &mut [u8], seed: usize) {
    for (i, b) in buf.iter_mut().enumerate() {
        *b = (i * 7 + 3 + seed) as u8;
    }
}

/// One datagram of `n` patterned bytes to the peer and its echo read back
/// equal from the peer's address and port, inside `bound_ms`: the
/// milliseconds it took, or the words of what went wrong.
fn datagram_echo(d: i32, peer: &Peer, n: usize, seed: usize, bound_ms: u32) -> Result<u32, &'static str> {
    let out = &mut out_buf()[..n];
    pattern(out, seed);
    let t0 = sched::now_ms();
    if api::rhu_net_send(d, out.as_ptr(), n as u32, &addr(peer.ip, PEER_PORT)) != n as i32 {
        return Err("the send did not take the datagram");
    }
    let deadline = deadline_in(bound_ms);
    loop {
        let back = &mut back_buf()[..];
        let (r, from) = read_datagram(d, back, deadline);
        if r == sched::ERR_TIMEDOUT {
            return Err("no echo inside the bound");
        }
        if r < 0 {
            return Err("the read was refused");
        }
        // A datagram of another cycle or another sender is not this echo: the next one is read.
        if r as usize == n && back[..n] == *out && ip_of(&from) == peer.ip && from.port == PEER_PORT as u32 {
            return Ok(since(t0));
        }
    }
}

/// A stream connected to the peer's echo port: the handle, or the words.
fn stream_to_peer(peer: &Peer) -> Result<i32, &'static str> {
    let c = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    if c < 0 {
        return Err("no stream socket in the pool");
    }
    if api::rhu_net_connect(c, &addr(peer.ip, peer.tcp_port)) != net::ERR_INPROGRESS {
        api::rhu_net_close(c);
        return Err("the connect did not answer EINPROGRESS");
    }
    let m = wait_mask(c, net::WRITABLE as i32, deadline_in(3000));
    if m != net::WRITABLE as i32 || api::rhu_net_error(c) != 0 {
        api::rhu_net_close(c);
        return Err("the stream was not writable without an error inside 3000 ms");
    }
    Ok(c)
}

/// `n` patterned bytes out on the stream and their echo read back equal.
fn stream_echo(c: i32, n: usize, seed: usize, bound_ms: u32) -> Result<u32, &'static str> {
    let out = &mut out_buf()[..n];
    pattern(out, seed);
    let back = &mut back_buf()[..n];
    let t0 = sched::now_ms();
    let deadline = deadline_in(bound_ms);
    // The two directions move together: the peer echoes as it reads, and neither buffer holds the whole.
    let (mut sent, mut got) = (0usize, 0usize);
    while got < n {
        let mut moved = false;
        if sent < n {
            let r = api::rhu_net_send(c, out[sent..].as_ptr(), (n - sent) as u32, core::ptr::null());
            if r > 0 {
                sent += r as usize;
                moved = true;
            } else if r != sched::ERR_AGAIN {
                return Err("a send was refused");
            }
        }
        let r = api::rhu_net_recv(c, back[got..].as_mut_ptr(), (n - got) as u32, 0, core::ptr::null_mut());
        if r > 0 {
            got += r as usize;
            moved = true;
        } else if r == 0 {
            return Err("the peer closed inside the echo");
        } else if r != sched::ERR_AGAIN {
            return Err("a read was refused");
        }
        if !moved {
            if sched::now_ns() >= deadline {
                return Err("the echo did not complete inside the bound");
            }
            sched::sleep_ms(1);
        }
    }
    if back != out {
        return Err("the echoed bytes differ");
    }
    Ok(since(t0))
}

/// The call: the peer asked to connect to the listener, the caller
/// accepted from the peer's address, its 26 letters read and sent back, its
/// close read.
fn take_call(d: i32, l: i32, peer: &Peer) -> Result<(), &'static str> {
    let mut msg = [0u8; 18];
    msg[..16].copy_from_slice(CALL);
    msg[16..].copy_from_slice(&LISTEN_PORT.to_be_bytes());
    if api::rhu_net_send(d, msg.as_ptr(), 18, &addr(peer.ip, PEER_PORT)) != 18 {
        return Err("the call's datagram was not taken");
    }
    if wait_mask(l, net::READABLE as i32, deadline_in(5000)) != net::READABLE as i32 {
        return Err("the listener was not readable inside 5000 ms");
    }
    let mut from = none_addr();
    let s = api::rhu_net_accept(l, &mut from);
    if s < 0 {
        return Err("the accept answered no handle");
    }
    let mut got = [0u8; 26];
    let r = if ip_of(&from) != peer.ip {
        Err("the caller's address is not the peer's")
    } else if read_all(s, &mut got, deadline_in(3000)) != 0 || got != *LETTERS {
        Err("the caller's 26 letters were not read equal")
    } else if send_all(s, &got, deadline_in(3000)) != 0 {
        Err("the 26 letters were not sent back")
    } else if wait_mask(s, net::HANGUP as i32, deadline_in(3000)) & net::HANGUP as i32 == 0 {
        Err("the caller's close was not read as hang-up inside 3000 ms")
    } else {
        Ok(())
    };
    api::rhu_net_close(s);
    r
}

/// A resolution through the kernel's functions: the count or the code, the
/// first two addresses, the milliseconds it took.
fn resolve(name: &str, bound_ms: u32) -> (i32, [net::Addr; 2], u32) {
    let mut out = [none_addr(); 2];
    let t0 = sched::now_ms();
    let h = api::rhu_net_resolve(name.as_ptr(), name.len() as u32);
    if h < 0 {
        return (h, out, 0);
    }
    let w = api::rhu_net_resolve_word(h);
    let mut r = sched::ERR_AGAIN;
    wait_until(w, deadline_in(bound_ms), || {
        r = api::rhu_net_resolve_result(h, out.as_mut_ptr(), 2);
        r != sched::ERR_AGAIN
    });
    (if r == sched::ERR_AGAIN { sched::ERR_TIMEDOUT } else { r }, out, since(t0))
}

pub fn run() {
    let mut f = Flight { pass: 0, refused: 0, skipped: 0, observed: 0, tags: [0; 12], tagged: 0 };
    kprintln!("[flight] the wired medium's flight check starts {} ms after the boot; every address is masked", sched::now_ms());
    let mut leg = 0u8;
    if link(&mut f) {
        hw_address(&mut f);
        if let Some(lease) = lease(&mut f, "lease", 0) {
            let d = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
            let bound = d >= 0 && api::rhu_net_bind(d, &addr([0; 4], OUR_PORT)) == 0;
            match if bound { discovery(&mut f, d) } else { None } {
                Some(peer) => {
                    leg = peer.leg;
                    f.pass("address-order", format_args!("a frame addressed to the board's own hardware address was received: the peer's answer, sent to this board alone"));
                    match peer.leg {
                        LEG_FIRST => first_leg(&mut f, d, &peer, &lease),
                        LEG_CABLE => cable_leg(&mut f, d, &peer, &lease),
                        LEG_SOAK => soak_leg(&mut f, d, &peer),
                        other => f.refused("leg", format_args!("the peer named the leg {} where 1, 2 or 3 was expected", other)),
                    }
                }
                None => {
                    if !bound {
                        f.refused("discovery", format_args!("the datagram socket or its bind to port {} was refused", OUR_PORT));
                    }
                    f.skipped("address-order", format_args!("no answer from the peer"));
                    f.skipped("the peer's cases", format_args!("no peer"));
                }
            }
            if d >= 0 {
                api::rhu_net_close(d);
            }
        }
    }
    f.skipped("renewal", format_args!("the lease's duration is the server's, it is not read through the kernel's functions, and the server is never configured by the flight"));
    wire_truth("the end");
    kprintln!("[flight] SUMMARY leg {}: {} PASS, {} REFUSED, {} SKIPPED, {} OBSERVED", leg, f.pass, f.refused, f.skipped, f.observed);
    kprintln!("[flight] END");
    api::rhu_exit(if f.refused == 0 { 0 } else { 1 })
}

/// L1: the medium's link inside ten seconds, the driver's bring-up and the
/// transceiver's identity; the resolution and the transceiver's path printed.
fn link(f: &mut Flight) -> bool {
    let step = eth(&ETH.bring_up);
    if step != 1 {
        f.refused("link", format_args!("the driver's bring-up ended at step {} (2 the rings outside the RAM the controller reaches, 3 no reference clock, 4 the management interface silent, 5 the transceiver's identity wrong)", step));
        return false;
    }
    let mw = api::rhu_net_medium_word(0);
    if api::rhu_net_count() != 1 || mw.is_null() {
        f.refused("link", format_args!("the count reads {} or the medium has no word", api::rhu_net_count()));
        return false;
    }
    let up = wait_until(mw, deadline_in(10_000), || medium_link() == 1);
    let at = sched::now_ms();
    let mut info = net::MediumInfo { kind: 9, link: 9, hw: [0; 6], reserved: 0 };
    let rc = api::rhu_net_medium(0, &mut info);
    let (id1, id2, res) = (eth(&ETH.phy_id1), eth(&ETH.phy_id2), eth(&ETH.resolution));
    if !up || rc != 0 || info.kind != net::KIND_WIRED || info.link != 1 || eth(&ETH.link_ups) == 0 || id1 != 0x0007 || id2 & 0xFFF0 != 0xC130 {
        wire_truth("no link");
        f.refused("link", format_args!("no link inside 10000 ms: the medium reads rc {} kind {} link {}, the transceiver's identity {:04x} {:04x}", rc, info.kind, info.link, id1, id2));
        return false;
    }
    kprintln!(
        "[flight] link: up {} ms after the boot at {} Mbit {} duplex; the transceiver was {}; its identity {:04x} {:04x}, the management clock the bus clock over {}",
        at,
        if res & 1 != 0 { 100 } else { 10 },
        if res & 2 != 0 { "full" } else { "half" },
        if at < 1000 { "found negotiated and left as found" } else { "found without a link, reset and negotiated" },
        id1,
        id2,
        eth(&ETH.mdc_divisor)
    );
    f.pass("link", format_args!("one wired medium with its link up inside 10000 ms; the driver brought up; the transceiver's identity read"));
    true
}

/// L2: the medium's hardware address is the witness's, locally administered
/// and unicast; never printed.
fn hw_address(f: &mut Flight) {
    let mut info = net::MediumInfo { kind: 9, link: 9, hw: [0; 6], reserved: 0 };
    api::rhu_net_medium(0, &mut info);
    let w = witness();
    if info.hw == w.hw[..6] && info.hw[0] & 3 == 2 && info.hw != [0; 6] {
        f.pass("hw-address", format_args!("the medium's hardware address is the witness's, locally administered and unicast; its equality with the digest of the chip's identifier is the probe's verdict"));
    } else {
        f.refused("hw-address", format_args!("the medium's hardware address differs from the witness's, or its first byte's two low bits read {} where 2 was expected", info.hw[0] & 3));
    }
}

/// L3: a lease by DHCP inside ninety seconds, its record sound; the
/// wire-truth line every ten seconds while it is pending. `changes` is the
/// witness's count of lease changes the lease must be newer than.
fn lease(f: &mut Flight, case: &str, changes: u32) -> Option<net::Lease> {
    let mw = api::rhu_net_medium_word(0);
    let t0 = sched::now_ms();
    let out0 = eth(&ETH.tx_frames);
    let mut got = None;
    for _ in 0..9 {
        let fresh = || witness().lease_changes > changes && lease_now().is_some_and(|l| l.source == net::SOURCE_DHCP);
        if wait_until(mw, deadline_in(10_000), fresh) {
            got = lease_now();
            break;
        }
        wire_truth("the lease pending");
    }
    let Some(l) = got else {
        wire_truth("no lease");
        let heard = eth(&ETH.rx_frames) > 0 && eth(&ETH.tx_frames) > 0;
        f.refused(case, format_args!("no lease inside 90000 ms; {}", if heard { "frames went out and frames came in, so the suspect is the byte order of the controller's two address registers (the frames addressed to this board are dropped by its own filter)" } else { "no frame came in or none went out: the suspect is the data path, not the address filter" }));
        return None;
    };
    let (ip, mask, gw) = (l.ip4.to_ne_bytes(), num(l.mask.to_ne_bytes()), l.gateway.to_ne_bytes());
    let prefix = mask.leading_ones();
    let sound = l.ip4 != 0 && prefix + mask.trailing_zeros() == 32 && (8..=30).contains(&prefix) && l.gateway != 0 && num(gw) & mask == num(ip) & mask;
    let (ti, tg, t1, t2) = (f.tag(ip), f.tag(gw), f.tag(l.dns[0].to_ne_bytes()), f.tag(l.dns[1].to_ne_bytes()));
    kprintln!(
        "[flight] {}: the address {}, the gateway {}, the name servers {} and {}; {} ms after the wait began, {} ms after the boot; {} frames went out during the wait",
        case,
        ti,
        tg,
        t1,
        t2,
        since(t0),
        witness().lease_at_ms,
        eth(&ETH.tx_frames).wrapping_sub(out0)
    );
    if sound {
        f.pass(case, format_args!("a lease by DHCP inside 90000 ms: an address, a mask of 8 to 30 bits in one run, a gateway on the address's subnet"));
        Some(l)
    } else {
        f.refused(case, format_args!("the lease's record is not sound: the address or the gateway is zero, the mask is not one run of 8 to 30 bits, or the gateway is off the address's subnet"));
        None
    }
}

/// L4: the peer found by one datagram to the limited broadcast address,
/// sent at most five times two seconds apart; the answer well formed with
/// the nonce sent.
fn discovery(f: &mut Flight, d: i32) -> Option<Peer> {
    let mut ask = [0u8; 24];
    ask[..16].copy_from_slice(ASK);
    entropy::fill(&mut ask[16..]);
    for attempt in 1..=5u32 {
        let r = api::rhu_net_send(d, ask.as_ptr(), 24, &addr([255; 4], PEER_PORT));
        if r != 24 {
            f.refused("discovery", format_args!("the datagram to the limited broadcast address was refused with {}", r));
            return None;
        }
        let deadline = deadline_in(2000);
        loop {
            let mut buf = [0u8; 64];
            let (n, from) = read_datagram(d, &mut buf, deadline);
            if n == sched::ERR_TIMEDOUT {
                break;
            }
            if n != 40 || buf[..16] != *ANSWER || buf[16..24] != ask[16..] || from.port != PEER_PORT as u32 {
                continue;
            }
            let peer = Peer {
                ip: ip_of(&from),
                leg: buf[24],
                closed_word: buf[25],
                unused: if buf[26] == 1 { Some([buf[28], buf[29], buf[30], buf[31]]) } else { None },
                tcp_port: u16::from_be_bytes([buf[32], buf[33]]),
                closed_port: u16::from_be_bytes([buf[34], buf[35]]),
                soak_s: u16::from_be_bytes([buf[36], buf[37]]) as u32,
            };
            let tp = f.tag(peer.ip);
            kprintln!("[flight] discovery: the peer at {} answered send {} of 5; the leg {}, its word on the closed port {}, an unused address {}", tp, attempt, peer.leg, peer.closed_word, if peer.unused.is_some() { "given" } else { "not given" });
            f.pass("discovery", format_args!("the peer answered the datagram to the limited broadcast address inside 2000 ms of a send, with the nonce sent"));
            return Some(peer);
        }
    }
    wire_truth("no peer");
    f.refused("discovery", format_args!("no well-formed answer to five datagrams to the limited broadcast address, two seconds apart"));
    None
}

fn first_leg(f: &mut Flight, d: i32, peer: &Peer, lease: &net::Lease) {
    // R1 and R2: the two lookups.
    if lease.dns[0] == 0 {
        f.skipped("resolve", format_args!("the lease names no name server"));
        f.skipped("resolve-none", format_args!("the lease names no name server"));
    } else {
        let (r, a, ms) = resolve(NAME, 12_000);
        let pair = (1..=2).contains(&r) && a[..r as usize].iter().all(|x| NAME_PAIR.contains(&ip_of(x))) && (r == 1 || ip_of(&a[0]) != ip_of(&a[1]));
        if (1..=2).contains(&r) && ms <= 10_000 && a[..r as usize].iter().all(|x| x.family == net::FAMILY_IPV4) {
            f.pass("resolve", format_args!("the public name answered with {} address records in {} ms; the answered set is the name's public pair: {}", r, ms, if pair { "yes" } else { "no" }));
        } else {
            f.refused("resolve", format_args!("the public name's resolution answered {} after {} ms where 1 or 2 addresses inside 10000 ms were expected", r, ms));
        }
        let (r, _, ms) = resolve(NO_NAME, 12_000);
        if r == net::ERR_HOSTUNREACH && ms <= 10_000 {
            f.pass("resolve-none", format_args!("the name that does not exist answered RHU_EHOSTUNREACH in {} ms", ms));
        } else if r >= 1 || r == sched::ERR_TIMEDOUT {
            f.observed("resolve-none", format_args!("the name that does not exist answered {} after {} ms: the network's name server rewrites or withholds a name error", r, ms));
        } else {
            f.refused("resolve-none", format_args!("the name that does not exist answered {} after {} ms where RHU_EHOSTUNREACH was expected", r, ms));
        }
    }

    // U1: datagrams both ways, a small one and the largest one frame carries.
    match (datagram_echo(d, peer, 64, 0, 2000), datagram_echo(d, peer, 1472, 1, 2000)) {
        (Ok(a), Ok(b)) => f.pass("udp-echo", format_args!("64 bytes echoed equal by the peer in {} ms and 1472 bytes, a full frame, in {} ms, each from the peer's address and port", a, b)),
        (a, b) => f.refused("udp-echo", format_args!("the 64-byte echo: {}; the 1472-byte echo: {}", a.err().unwrap_or("equal"), b.err().unwrap_or("equal"))),
    }

    // T1: a stream to the peer.
    match stream_to_peer(peer) {
        Err(why) => f.refused("tcp-echo", format_args!("{}", why)),
        Ok(c) => {
            let small = stream_echo(c, 26, 2, 3000);
            let large = stream_echo(c, 3000, 3, 10_000);
            let shut = api::rhu_net_shutdown(c, net::SHUT_WR);
            let m = wait_mask(c, net::HANGUP as i32, deadline_in(3000));
            let mut one = [0u8; 1];
            let end = api::rhu_net_recv(c, one.as_mut_ptr(), 1, 0, core::ptr::null_mut());
            let closed = shut == 0 && m & (net::HANGUP | net::ERROR) as i32 == net::HANGUP as i32 && end == 0;
            api::rhu_net_close(c);
            match (small, large, closed) {
                (Ok(a), Ok(b), true) => f.pass("tcp-echo", format_args!("a stream to the peer connected inside 3000 ms; 26 bytes echoed equal in {} ms and 3000 bytes in {} ms; the peer's close read as hang-up and a read of 0 after the write side was shut", a, b)),
                (a, b, _) => f.refused("tcp-echo", format_args!("the 26-byte echo: {}; the 3000-byte echo: {}; the half-close: shutdown {}, the mask {}, the read {}", a.err().unwrap_or("equal"), b.err().unwrap_or("equal"), shut, m, end)),
            }
        }
    }

    // A1: the listener takes a caller.
    let l = listener();
    match if l >= 0 { take_call(d, l, peer) } else { Err("the listener's socket, bind or listen was refused") } {
        Ok(()) => f.pass("accept", format_args!("the listener on port {} was readable inside 5000 ms of the call; the caller accepted from the peer's address, its 26 letters read equal and sent back, its close read as hang-up", LISTEN_PORT)),
        Err(why) => f.refused("accept", format_args!("{}", why)),
    }
    if l >= 0 {
        api::rhu_net_close(l);
    }

    closed_port(f, peer);
    budget(f, peer, lease);
}

fn listener() -> i32 {
    let l = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    if l >= 0 && (api::rhu_net_bind(l, &addr([0; 4], LISTEN_PORT)) != 0 || api::rhu_net_listen(l, 1) != 0) {
        api::rhu_net_close(l);
        return -1;
    }
    l
}

/// A connect that must fail: the mask at its end, the code, the
/// milliseconds from the call.
fn failing_connect(to: net::Addr) -> Result<(i32, i32, u32), &'static str> {
    let c = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    if c < 0 {
        return Err("no stream socket in the pool");
    }
    let t0 = sched::now_ms();
    if api::rhu_net_connect(c, &to) != net::ERR_INPROGRESS {
        api::rhu_net_close(c);
        return Err("the connect did not answer EINPROGRESS");
    }
    let m = wait_mask(c, (net::WRITABLE | net::ERROR) as i32, deadline_in(net::CONNECT_BUDGET_MS + 3000));
    let ms = since(t0);
    let code = api::rhu_net_error(c);
    api::rhu_net_close(c);
    Ok((m, code, ms))
}

/// C1: a connect to the peer machine's closed port, judged by the peer's
/// word on what that machine does with one.
fn closed_port(f: &mut Flight, peer: &Peer) {
    let (m, code, ms) = match failing_connect(addr(peer.ip, peer.closed_port)) {
        Ok(x) => x,
        Err(why) => return f.refused("closed-port", format_args!("{}", why)),
    };
    let reset = m == DEAD && code == net::ERR_CONNREFUSED && ms <= 2000;
    let silent = m == DEAD && code == net::ERR_HOSTUNREACH && (2900..=4000).contains(&ms);
    match (peer.closed_word, reset, silent) {
        (1, true, _) | (0, true, _) => f.pass("closed-port", format_args!("the peer machine answered a reset: the connect ended RHU_ECONNREFUSED in {} ms with the mask writable, error and hang-up", ms)),
        (2, _, true) | (0, _, true) => f.pass("closed-port", format_args!("the peer machine stayed silent: the connect ended RHU_EHOSTUNREACH at the connect budget, {} ms after the call, with the mask writable, error and hang-up", ms)),
        (1, _, true) => f.observed("closed-port", format_args!("the peer's word said a reset and the machine stayed silent: the connect ended RHU_EHOSTUNREACH at the budget, {} ms after the call", ms)),
        (2, true, _) => f.observed("closed-port", format_args!("the peer's word said silence and the machine answered a reset: the connect ended RHU_ECONNREFUSED in {} ms", ms)),
        _ => f.refused("closed-port", format_args!("the connect ended with the mask {}, the code {}, {} ms after the call: neither a reset inside 2000 ms nor the budget between 2900 and 4000 ms", m, code, ms)),
    }
}

/// B1: the connect budget against the address nobody uses, and a
/// resolution timed inside it.
fn budget(f: &mut Flight, peer: &Peer, lease: &net::Lease) {
    let Some(unused) = peer.unused else {
        return f.skipped("budget", format_args!("the peer gave no unused address"));
    };
    let mask = num(lease.mask.to_ne_bytes());
    let (ours, gw) = (lease.ip4.to_ne_bytes(), lease.gateway.to_ne_bytes());
    if num(unused) & mask != num(ours) & mask || unused == ours || unused == gw || unused == peer.ip {
        return f.skipped("budget", format_args!("the unused address is not on the lease's subnet, or it is the board's, the gateway's or the peer's"));
    }
    let c = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let t0 = sched::now_ms();
    if c < 0 || api::rhu_net_connect(c, &addr(unused, UNUSED_PORT)) != net::ERR_INPROGRESS {
        if c >= 0 {
            api::rhu_net_close(c);
        }
        return f.refused("budget", format_args!("the stream socket or its connect was refused"));
    }
    sched::sleep_ms(500);
    let q0 = sched::now_ms();
    let h = if lease.dns[0] != 0 { api::rhu_net_resolve(NAME.as_ptr(), NAME.len() as u32) } else { -1 };
    let mut out = [none_addr(); 2];
    let (mut query, mut ended) = (None, None);
    while since(t0) < 13_000 && (ended.is_none() || (h >= 0 && query.is_none())) {
        if h >= 0 && query.is_none() {
            let r = api::rhu_net_resolve_result(h, out.as_mut_ptr(), 2);
            if r != sched::ERR_AGAIN {
                query = Some((r, since(q0)));
            }
        }
        if ended.is_none() {
            let m = api::rhu_net_readiness(c, 0);
            if m != 0 {
                ended = Some((m, since(t0)));
            }
        }
        sched::sleep_ms(5);
    }
    let code = api::rhu_net_error(c);
    api::rhu_net_close(c);
    match query {
        Some((r, ms)) => kprintln!("[flight] budget: a resolution issued 500 ms into the connect answered {} in {} ms", r, ms),
        None => kprintln!("[flight] budget: no resolution was timed inside the connect ({})", if h >= 0 { "it did not answer inside the bound" } else { "no name server, or no slot" }),
    }
    match ended {
        Some((m, ms)) if m == DEAD && code == net::ERR_HOSTUNREACH && (2900..=4000).contains(&ms) => f.pass("budget", format_args!("the connect to the address nobody uses ended RHU_EHOSTUNREACH at the connect budget, {} ms after the call, with the mask writable, error and hang-up", ms)),
        Some((m, ms)) => f.refused("budget", format_args!("the connect to the address nobody uses ended with the mask {}, the code {}, {} ms after the call, where RHU_EHOSTUNREACH between 2900 and 4000 ms was expected", m, code, ms)),
        None => f.refused("budget", format_args!("the connect to the address nobody uses did not end inside 13000 ms")),
    }
}

/// The wait for the link to read `want`, up to 900 s, a line every 30 s.
fn wait_link(want: u32, what: &str) -> bool {
    let mw = api::rhu_net_medium_word(0);
    for i in 1..=30u32 {
        if wait_until(mw, deadline_in(30_000), || medium_link() == want) {
            return true;
        }
        kprintln!("[flight] CABLE: still waiting for the link to {} ({} s of 900)", what, i * 30);
    }
    false
}

fn cable_leg(f: &mut Flight, d: i32, peer: &Peer, before: &net::Lease) {
    // THE SETUP: a connected stream, a listener, the datagram socket already bound.
    let c = match stream_to_peer(peer) {
        Ok(c) if stream_echo(c, 26, 4, 3000).is_ok() => c,
        _ => {
            f.refused("cable-setup", format_args!("the stream to the peer or its 26-byte echo before the pull"));
            f.skipped("cable-down", format_args!("no setup"));
            return f.skipped("cable-up", format_args!("no setup"));
        }
    };
    let l = listener();
    let mw = api::rhu_net_medium_word(0);
    let (word0, downs0, changes0) = (unsafe { read_volatile(mw) }, eth(&ETH.link_downs), witness().lease_changes);
    kprintln!("[flight] cable-setup: a stream to the peer connected with 26 bytes echoed, a listener on port {}, a datagram socket on port {}", LISTEN_PORT, OUR_PORT);
    kprintln!("[flight] CABLE: WAITING FOR THE LINK TO DROP (the wait is 900 s)");
    if !wait_link(0, "drop") {
        api::rhu_net_close(c);
        if l >= 0 {
            api::rhu_net_close(l);
        }
        f.skipped("cable-down", format_args!("no hand within the wait"));
        return f.skipped("cable-up", format_args!("no hand within the wait"));
    }
    // K1: the checks at Down.
    let moved = unsafe { read_volatile(mw) } != word0;
    let m = wait_mask(c, net::ERROR as i32, deadline_in(1000));
    let code = api::rhu_net_error(c);
    let (lm, le) = if l >= 0 { (api::rhu_net_readiness(l, 0), api::rhu_net_error(l)) } else { (-1, -1) };
    let de = api::rhu_net_error(d);
    let n = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let nc = if n >= 0 { api::rhu_net_connect(n, &addr(peer.ip, peer.tcp_port)) } else { n };
    if n >= 0 {
        api::rhu_net_close(n);
    }
    api::rhu_net_close(c);
    let downs = eth(&ETH.link_downs).wrapping_sub(downs0);
    if moved && m == RESET && code == net::ERR_CONNRESET && lm == 0 && le == 0 && de == 0 && nc == net::ERR_NETDOWN && downs == 1 {
        f.pass("cable-down", format_args!("the link dropped: the medium's word moved and its link reads 0; the connected stream readable, writable, hang-up and error with RHU_ECONNRESET inside 1000 ms; the listener's mask 0 with no error; the datagram socket with no error; a new connect RHU_ENETDOWN at the call; one link down counted"));
    } else {
        f.refused("cable-down", format_args!("at the drop: the medium's word moved {}; the stream's mask {} and code {}; the listener's mask {} and code {}; the datagram socket's code {}; a new connect {}; link downs counted {}", moved, m, code, lm, le, de, nc, downs));
    }
    kprintln!("[flight] CABLE: DOWN CHECKS DONE, WAITING FOR THE LINK TO RETURN (the wait is 900 s)");
    if !wait_link(1, "return") {
        if l >= 0 {
            api::rhu_net_close(l);
        }
        return f.skipped("cable-up", format_args!("no hand within the wait"));
    }
    // K2: the lease started over, then every kept socket used.
    let Some(after) = lease(f, "cable-lease", changes0 + 1) else {
        if l >= 0 {
            api::rhu_net_close(l);
        }
        return f.skipped("cable-up", format_args!("no lease after the link's return"));
    };
    let stream = stream_to_peer(peer).and_then(|s| {
        let r = stream_echo(s, 26, 5, 3000);
        api::rhu_net_close(s);
        r
    });
    let datagram = datagram_echo(d, peer, 64, 6, 2000);
    let call = if l >= 0 { take_call(d, l, peer) } else { Err("the listener was refused at the setup") };
    if l >= 0 {
        api::rhu_net_close(l);
    }
    match (stream, datagram, call) {
        (Ok(_), Ok(_), Ok(())) => f.pass("cable-up", format_args!("the link returned: a lease by DHCP after the client was started over, the same address as before: {}; a new stream to the peer echoed 26 bytes; the kept datagram socket echoed 64 bytes; the kept listener took a caller", if after.ip4 == before.ip4 { "yes" } else { "no" })),
        (s, g, k) => f.refused("cable-up", format_args!("after the return: the new stream: {}; the kept datagram socket: {}; the kept listener: {}", s.err().unwrap_or("echoed"), g.err().unwrap_or("echoed"), k.err().unwrap_or("took a caller"))),
    }
}

fn soak_leg(f: &mut Flight, d: i32, peer: &Peer) {
    let c = match stream_to_peer(peer) {
        Ok(c) => c,
        Err(why) => return f.refused("soak", format_args!("{}", why)),
    };
    let changes0 = witness().lease_changes;
    let (mut lost, mut late, mut cycles) = (0u32, 0u32, 0u32);
    let mut broken = None;
    let t0 = sched::now_ms();
    kprintln!("[flight] soak: {} cycles a second apart, 64 bytes by datagram and 64 on the stream each", peer.soak_s);
    for i in 0..peer.soak_s {
        // The cycle's second, then the two echoes.
        while since(t0) < i * 1000 {
            sched::sleep_ms(1);
        }
        cycles = i + 1;
        if let Err(why) = datagram_echo(d, peer, 64, i as usize, 900) {
            lost += 1;
            kprintln!("[flight] soak: LOSS at cycle {} (datagram): {}", i, why);
        }
        match stream_echo(c, 64, i as usize + 1, 5000) {
            Ok(ms) if ms > 900 => {
                late += 1;
                kprintln!("[flight] soak: LATE at cycle {} (stream): {} ms", i, ms);
            }
            Ok(_) => {}
            Err(why) => {
                kprintln!("[flight] soak: BROKEN at cycle {} (stream): {}", i, why);
                broken = Some(why);
                break;
            }
        }
        if cycles % 10 == 0 {
            kprintln!("[flight] soak: {} cycles, {} datagrams lost, {} stream cycles late; frames {} out, {} in, {} errored, {} fragments, {} missed, {} transmit errors", cycles, lost, late, eth(&ETH.tx_frames), eth(&ETH.rx_frames), eth(&ETH.rx_errored), eth(&ETH.rx_fragments), eth(&ETH.rx_missed), eth(&ETH.tx_errors));
        }
    }
    let alive = broken.is_none() && api::rhu_net_readiness(c, 0) & (net::ERROR | net::HANGUP) as i32 == 0;
    api::rhu_net_close(c);
    if witness().lease_changes != changes0 {
        kprintln!("[flight] soak: the lease changed {} times during the soak", witness().lease_changes.wrapping_sub(changes0));
    }
    if alive && cycles == peer.soak_s && lost * 20 <= cycles {
        f.pass("soak", format_args!("{} cycles in {} s: every stream cycle's bytes equal and in order, the connection alive at the end; {} datagrams lost, {} stream cycles later than 900 ms", cycles, since(t0) / 1000, lost, late));
    } else {
        f.refused("soak", format_args!("{} of {} cycles; the stream: {}; {} datagrams lost (the bound is 5 in 100)", cycles, peer.soak_s, broken.unwrap_or(if alive { "alive" } else { "ended or in error at the end" }), lost));
    }
}
