//! The kernel's network API's nine checks of the api flavor, on the
//! emulator rows over the virtual medium and its scripted peer: the pump's
//! statistics, the medium and the lease, the retry scenario, the resolver,
//! a stream over the loopback, datagrams, the refusals, the event port's
//! properties, the bind to port 0; under `test-net-link-drop` another follows them, the link
//! dropped and raised under open sockets; under `test-net-resolve-two` a
//! name with two address records; under `test-net-listen-again` a listener
//! whose accept finds the stream pool spent; under `test-net-listen-reset` a
//! caller reset before the accept, and a bind while a connection is still
//! closing. Every marker is constant text the api gate pins; the rows'
//! own numbers (the passes, the milliseconds) are on lines the gate does
//! not pin. Every wait is the recipe the header states -- the function, the
//! snapshot, the function again, the futex wait with a deadline -- because this
//! thread is more urgent than the network thread and a spin would starve it.

use core::ptr::{addr_of, read_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::net_peer::{LEASE_DNS2 as DNS2, LEASE_IP as LEASE, LEASE_MASK as MASK, NAMES, OFF_SEGMENT_IP as OFF_SEGMENT, OUR_MAC, PEER_IP as PEER, REFUSED_PORT, UNKNOWN_NAME, UNOWNED_IP as UNOWNED};
#[cfg(feature = "test-net-resolve-two")]
use crate::net_peer::{PAIR_IPS, PAIR_NAME};
use crate::{api, net, sched};

const THIRD: [u8; 4] = NAMES[2].1;

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

fn word(o: [u8; 4]) -> u32 {
    u32::from_ne_bytes(o)
}

fn witness() -> net::NetWitness {
    unsafe { read_volatile(addr_of!(net::WITNESS)) }
}

fn deadline_in(ms: u32) -> u64 {
    sched::now_ns() + ms as u64 * 1_000_000
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
        assert!(!w.is_null(), "net: the event word of handle {} is null", h);
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

/// The recipe on any word until `done` answers Some, or the deadline.
fn wait_until<T>(w: *const u32, deadline_ns: u64, mut done: impl FnMut() -> Option<T>) -> Option<T> {
    loop {
        if let Some(t) = done() {
            return Some(t);
        }
        let v = unsafe { read_volatile(w) };
        if let Some(t) = done() {
            return Some(t);
        }
        if api::rhu_futex_wait(w, v, deadline_ns) == sched::ERR_TIMEDOUT {
            return done();
        }
    }
}

pub fn run() {
    check_net_pump();
    check_net_medium();
    check_net_retry();
    check_net_refuse();
    check_net_resolve();
    check_net_stream();
    check_net_datagram();
    check_net_port();
    check_net_bind_any();
    #[cfg(feature = "test-net-link-drop")]
    check_net_link();
    #[cfg(feature = "test-net-resolve-two")]
    check_net_resolve_two();
    #[cfg(feature = "test-net-resolve-second")]
    check_net_resolve_second();
    #[cfg(feature = "test-net-listen-again")]
    check_net_listen();
    #[cfg(feature = "test-net-listen-reset")]
    check_net_listen_reset();
}

/// The network thread's statistics after the boot's exchange: it ran, its
/// passes and its deadline wakes bounded (the slow-pump control makes every
/// pass half a second and is refused here). First among the checks so the
/// control bites before any wait could; the two bounds are judged before the
/// pass count, so a slow pump dies at its bound whatever its count.
fn check_net_pump() {
    kprintln!("[api] net-pump: the network thread's statistics after the boot's exchange -- passes counted, every pass and every deadline wake under the 400 ms bound");
    let w = witness();
    kprintln!(
        "[api] net-pump: {} passes ({} woken by the word, {} by the deadline, {} by a bump during the pass); the longest pass {} ms ({} over 10 ms, {} over 50 ms); the latest deadline wake {} ms late ({} over 10 ms, {} over 50 ms)",
        w.pump_passes,
        w.pump_wakes_word,
        w.pump_wakes_deadline,
        w.pump_wakes_again,
        w.pump_max_pass_ms,
        w.pump_passes_over_10,
        w.pump_passes_over_50,
        w.pump_max_late_ms,
        w.pump_late_over_10,
        w.pump_late_over_50
    );
    assert!(w.pump_max_pass_ms < 400, "net-pump: the longest pass took {} ms where the bound is 400", w.pump_max_pass_ms);
    assert!(w.pump_max_late_ms < 400, "net-pump: the latest deadline wake was {} ms late where the bound is 400", w.pump_max_late_ms);
    assert!(w.pump_passes >= 3, "net-pump: the network thread ran {} passes where at least 3 were expected", w.pump_passes);
    kprintln!("[api] PASS net-pump (the network thread ran through the boot's exchange with every pass and every deadline wake under the 400 ms bound; the counts above)");
}

/// The medium: one, virtual, link up, its hardware address; the lease by
/// DHCP from the scripted peer with the record's values exact; the limits;
/// the medium word moved; the refusals.
fn check_net_medium() {
    kprintln!("[api] net-medium: the count, the medium's facts, the lease by DHCP with the record exact, the limits, the medium word, the refusals");
    assert!(api::rhu_net_count() == 1, "net-medium: the count reads {}", api::rhu_net_count());
    let mut info = net::MediumInfo { kind: 9, link: 9, hw: [0; 6], reserved: 0 };
    assert!(api::rhu_net_medium(0, &mut info) == 0, "net-medium: the info door refused");
    assert!(info.kind == net::KIND_VIRTUAL && info.link == 1 && info.hw == OUR_MAC, "net-medium: the info reads kind {} link {} hw {:02x?}", info.kind, info.link, info.hw);
    let mw = api::rhu_net_medium_word(0);
    assert!(!mw.is_null() && api::rhu_net_medium_word(1).is_null(), "net-medium: the medium word pointers");
    let before = unsafe { read_volatile(mw) };
    let mut lease = net::Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 };
    let got = wait_until(mw, deadline_in(4000), || {
        let r = api::rhu_net_lease(0, &mut lease);
        if r == 0 { Some(()) } else { assert!(r == sched::ERR_AGAIN, "net-medium: the lease door answered {}", r); None }
    });
    assert!(got.is_some(), "net-medium: no lease within 4 s of the boot");
    assert!(
        lease.ip4 == word(LEASE) && lease.mask == word(MASK) && lease.gateway == word(PEER) && lease.dns == [word(PEER), word(DNS2)] && lease.source == net::SOURCE_DHCP,
        "net-medium: the lease reads {:08x} {:08x} {:08x} {:08x} {:08x} source {}",
        lease.ip4,
        lease.mask,
        lease.gateway,
        lease.dns[0],
        lease.dns[1],
        lease.source
    );
    let after = unsafe { read_volatile(mw) };
    assert!(after != before || before >= 2, "net-medium: the medium word did not move for the link and the lease ({} then {})", before, after);
    let mut lim = net::Limits { streams: 0, datagrams: 0, stream_rx: 0, stream_tx: 0, datagram_buf: 0, resolve_slots: 0 };
    assert!(api::rhu_net_limits(&mut lim) == 0, "net-medium: the limits door refused");
    assert!(
        lim.streams == 4 && lim.datagrams == 2 && lim.stream_rx == 2048 && lim.stream_tx == 2048 && lim.datagram_buf == 1536 && lim.resolve_slots == 4,
        "net-medium: the limits read {} {} {} {} {} {}",
        lim.streams,
        lim.datagrams,
        lim.stream_rx,
        lim.stream_tx,
        lim.datagram_buf,
        lim.resolve_slots
    );
    assert!(api::rhu_net_medium(1, &mut info) == sched::ERR_INVAL, "net-medium: a medium past the count not refused EINVAL");
    assert!(api::rhu_net_medium(0, core::ptr::null_mut()) == sched::ERR_INVAL, "net-medium: a null pointer not refused EINVAL");
    assert!(api::rhu_net_lease(1, &mut lease) == sched::ERR_INVAL && api::rhu_net_limits(core::ptr::null_mut()) == sched::ERR_INVAL, "net-medium: the lease and limits refusals");
    kprintln!("[api] PASS net-medium (1 virtual medium, link up, its hardware address; the lease by DHCP with the record exact: 192.0.2.2/24, the gateway 192.0.2.1, the name servers 192.0.2.1 and 192.0.2.3; the limits 4 streams, 2 datagrams, 2048 and 2048 byte stream buffers, a 1536 byte datagram buffer, 4 resolver slots; the medium word moved; a medium past the count and null pointers EINVAL)");
}

/// The retry scenario: the peer dropped the first discover, the client
/// retried after one second and the lease came on the second; the medium's
/// rings never overflowed.
fn check_net_retry() {
    kprintln!("[api] net-retry: the first discover unanswered by the peer, the lease on the retry after one second; the medium's rings never full");
    let (vw, pc) = net::probe_medium();
    let w = witness();
    kprintln!(
        "[api] net-retry: discovers {} (the first dropped; the transaction ids {:08x} then {:08x}, {}), offers {}, requests {}, acks {}; the lease at {} ms; the medium: {} frames out, {} in, {} looped, {} answered, {} dropped by the peer, {} dropped for want of a slot",
        pc.discovers,
        pc.first_xid,
        pc.last_xid,
        if pc.first_xid == pc.last_xid { "kept across the retry" } else { "fresh per discover, the stack's own rule" },
        pc.offers,
        pc.requests,
        pc.acks,
        w.lease_at_ms,
        vw.frames_tx,
        vw.frames_rx,
        vw.looped,
        vw.answered,
        vw.dropped_by_peer,
        vw.dropped_full
    );
    assert!(pc.discovers >= 2 && pc.offers >= 1 && pc.requests >= 1 && pc.acks >= 1, "net-retry: the exchange counts discovers {} offers {} requests {} acks {}", pc.discovers, pc.offers, pc.requests, pc.acks);
    assert!(pc.dropped >= 1, "net-retry: the peer dropped nothing");
    assert!(w.lease_at_ms >= 1000, "net-retry: the lease came at {} ms, before the one-second retry", w.lease_at_ms);
    assert!(vw.dropped_full == 0, "net-retry: the receive ring dropped {} frames for want of a slot", vw.dropped_full);
    kprintln!("[api] PASS net-retry (the first discover dropped by the peer, the retry answered, the lease after the one-second retransmit; no frame dropped for want of a slot)");
}

/// The resolver: a name answered with its record, a second name, a name
/// the peer has not, the slots exhausted, the refusals.
fn check_net_resolve() {
    kprintln!("[api] net-resolve: a scripted answer read back as an address record, a second name, a name the server has not, the slots exhausted, the refusals");
    let resolve = |name: &str| -> i32 { api::rhu_net_resolve(name.as_ptr(), name.len() as u32) };
    let result = |h: i32, deadline_ns: u64| -> (i32, net::Addr) {
        let mut out = [none_addr(); 2];
        let w = api::rhu_net_resolve_word(h);
        assert!(!w.is_null(), "net-resolve: the word of handle {} is null", h);
        let r = wait_until(w, deadline_ns, || {
            let r = api::rhu_net_resolve_result(h, out.as_mut_ptr(), 2);
            if r == sched::ERR_AGAIN { None } else { Some(r) }
        });
        (r.unwrap_or(sched::ERR_TIMEDOUT), out[0])
    };
    let (vw0, pc0) = net::probe_medium();
    let h = resolve(NAMES[0].0);
    assert!(h >= 0, "net-resolve: the first query refused {}", h);
    let (r, a) = result(h, deadline_in(4000));
    let (vw, pc) = net::probe_medium();
    let w0 = witness();
    let mut hex = [b'.'; 96];
    let n = (pc.dropped_len as usize).min(48);
    for (i, b) in pc.dropped_head[..n].iter().enumerate() {
        hex[2 * i] = b"0123456789abcdef"[(b >> 4) as usize];
        hex[2 * i + 1] = b"0123456789abcdef"[(b & 15) as usize];
    }
    let hex = core::str::from_utf8(&hex[..2 * n]).unwrap_or("?");
    let mut lease = net::Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 };
    let lr = api::rhu_net_lease(0, &mut lease);
    kprintln!("[api] net-resolve: the first name's query -- the resolver {} made {} failed; the peer {} answered {} refused, {} ARP replies, {} dropped; the medium {} out {} in ({} looped, {} answered, {} dropped full, {} dropped by the peer); the pump {} passes ({} by the word, {} by the deadline, {} again); the lease changes {}",
        w0.resolves_made, w0.resolves_failed, pc.dns_answered, pc.dns_refused, pc.arps_answered, pc.dropped, vw.frames_tx, vw.frames_rx, vw.looped, vw.answered, vw.dropped_full, vw.dropped_by_peer, w0.pump_passes, w0.pump_wakes_word, w0.pump_wakes_deadline, w0.pump_wakes_again, w0.lease_changes);
    kprintln!("[api] net-resolve: during the query {} out {} in ({} looped, {} answered, {} dropped by the peer); the last dropped frame of {} bytes: {}; the lease (rc {}) source {} gateway {:08x} dns {:08x} {:08x}",
        vw.frames_tx.wrapping_sub(vw0.frames_tx), vw.frames_rx.wrapping_sub(vw0.frames_rx), vw.looped.wrapping_sub(vw0.looped), vw.answered.wrapping_sub(vw0.answered), pc.dropped.wrapping_sub(pc0.dropped), pc.dropped_len, hex, lr, lease.source, lease.gateway, lease.dns[0], lease.dns[1]);
    let mut lhex = [b'.'; 96];
    let k = (vw.looped_len as usize).min(48);
    for (i, b) in vw.looped_head[..k].iter().enumerate() {
        lhex[2 * i] = b"0123456789abcdef"[(b >> 4) as usize];
        lhex[2 * i + 1] = b"0123456789abcdef"[(b & 15) as usize];
    }
    let (ia, ip, pd, staged) = net::probe_iface();
    kprintln!("[api] net-resolve: the last looped frame of {} bytes: {}; the interface {:?}/{}, the stack's poll delay {} ms, staged {}",
        vw.looped_len, core::str::from_utf8(&lhex[..2 * k]).unwrap_or("?"), ia, ip, pd, staged);
    assert!(vw.looped == vw0.looped, "net-resolve: {} frames looped back during the query -- an aborted connection still chattering on the medium", vw.looped.wrapping_sub(vw0.looped));
    assert!(r == 1 && a.family == net::FAMILY_IPV4 && ip_of(&a) == PEER && a.port == 0, "net-resolve: the first name read {} family {} {:?}", r, a.family, ip_of(&a));
    assert!(api::rhu_net_resolve_result(h, core::ptr::null_mut(), 0) == sched::ERR_INVAL, "net-resolve: the released slot not refused EINVAL");
    let h2 = resolve(NAMES[1].0);
    let (r2, a2) = result(h2, deadline_in(4000));
    assert!(r2 == 1 && ip_of(&a2) == DNS2, "net-resolve: the second name read {} {:?}", r2, ip_of(&a2));
    let h3 = resolve(UNKNOWN_NAME);
    let (r3, _) = result(h3, deadline_in(4000));
    assert!(r3 == net::ERR_HOSTUNREACH, "net-resolve: the unknown name read {} where EHOSTUNREACH was expected", r3);
    // The four slots taken, the fifth refused; every answer read back.
    let mut hs = [0i32; 4];
    for h in hs.iter_mut() {
        *h = resolve(NAMES[2].0);
        assert!(*h >= 0, "net-resolve: a slot refused {}", *h);
    }
    assert!(resolve(NAMES[2].0) == sched::ERR_AGAIN, "net-resolve: the fifth query not refused EAGAIN");
    for h in hs {
        let (r, a) = result(h, deadline_in(4000));
        assert!(r == 1 && ip_of(&a) == THIRD, "net-resolve: a slot's answer read {} {:?}", r, ip_of(&a));
    }
    assert!(resolve("bad name!") == sched::ERR_INVAL && api::rhu_net_resolve(core::ptr::null(), 3) == sched::ERR_INVAL, "net-resolve: a bad name or a null name not refused EINVAL");
    assert!(api::rhu_net_resolve_result(9, core::ptr::null_mut(), 0) == sched::ERR_INVAL && api::rhu_net_resolve_word(9).is_null() && api::rhu_net_resolve_word(0).is_null(), "net-resolve: the bad handle refusals");
    let w = witness();
    assert!(w.resolves_made == 7 && w.resolves_failed == 1, "net-resolve: the witness counts {} made, {} failed", w.resolves_made, w.resolves_failed);
    let (_, pc) = net::probe_medium();
    assert!(pc.dns_answered == 6 && pc.dns_refused == 1 && pc.arps_answered >= 1, "net-resolve: the peer counts {} answered, {} refused, {} ARP replies", pc.dns_answered, pc.dns_refused, pc.arps_answered);
    kprintln!("[api] PASS net-resolve (two names answered with their records through the resolver's word, a name the server has not EHOSTUNREACH, four slots taken and the fifth EAGAIN with every answer read, a bad name and bad handles EINVAL, the released slot EINVAL, the witness exact)");
}

/// A stream over the loopback: the connect edge, the risen bits, the
/// accept with the peer's address, a small then a multi-segment byte-exact
/// echo through the peek and the available function, the half-close, the closes.
fn check_net_stream() {
    kprintln!("[api] net-stream: a listener and a client over the loopback -- the connect edge, the risen bits, the accept, 26 then 3000 bytes echoed byte-exact, the peek, the half-close, the closes");
    let l = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let c = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 6);
    assert!(l >= 0 && c >= 0 && l != c, "net-stream: the sockets read {} and {}", l, c);
    assert!(api::rhu_net_bind(l, &addr(LEASE, 7777)) == 0, "net-stream: the bind refused");
    assert!(api::rhu_net_listen(l, 1) == 0, "net-stream: the listen refused");
    assert!(api::rhu_net_readiness(l, 0) == 0, "net-stream: the listener ready before any connection");
    let r = api::rhu_net_connect(c, &addr(LEASE, 7777));
    assert!(r == net::ERR_INPROGRESS, "net-stream: the connect answered {} where EINPROGRESS was expected", r);
    assert!(api::rhu_net_readiness(c, 0) == 0, "net-stream: a connecting stream reported readiness {}", api::rhu_net_readiness(c, 0));
    assert!(api::rhu_net_connect(c, &addr(LEASE, 7777)) == net::ERR_ALREADY, "net-stream: a second connect not refused EALREADY");
    let m = wait_mask(c, net::WRITABLE as i32, deadline_in(3000));
    assert!(m == net::WRITABLE as i32, "net-stream: the client's mask at establishment reads {}", m);
    assert!(api::rhu_net_error(c) == 0, "net-stream: an error pending after a good connect");
    assert!(api::rhu_net_connect(c, &addr(LEASE, 7777)) == net::ERR_ISCONN, "net-stream: a connect on the connected stream not refused EISCONN");
    let risen = api::rhu_net_readiness(c, 1);
    assert!(risen == net::WRITABLE as i32, "net-stream: the risen bits read {}", risen);
    assert!(api::rhu_net_readiness(c, 1) == 0, "net-stream: a second take of the risen bits was not empty");
    let m = wait_mask(l, net::READABLE as i32, deadline_in(3000));
    assert!(m == net::READABLE as i32, "net-stream: the listener's mask reads {}", m);
    let mut peer = none_addr();
    let s = api::rhu_net_accept(l, &mut peer);
    assert!(s >= 0, "net-stream: the accept answered {}", s);
    let (mut cl, mut cp) = (none_addr(), none_addr());
    assert!(api::rhu_net_name(c, &mut cl, &mut cp) == 0, "net-stream: the client's names refused");
    assert!(
        peer.family == net::FAMILY_IPV4 && ip_of(&peer) == LEASE && peer.port == cl.port && cl.port >= 49152 && ip_of(&cl) == LEASE && cp.family == net::FAMILY_IPV4 && cp.port == 7777,
        "net-stream: the accept's peer {:?}:{} against the client's local {:?}:{} and peer :{}",
        ip_of(&peer),
        peer.port,
        ip_of(&cl),
        cl.port,
        cp.port
    );
    assert!(api::rhu_net_accept(l, &mut peer) == sched::ERR_AGAIN, "net-stream: a second accept with nothing pending not EAGAIN");
    // 26 bytes, peeked then read.
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    assert!(api::rhu_net_send(c, alphabet.as_ptr(), 26, core::ptr::null()) == 26, "net-stream: the 26-byte send did not take all");
    let m = wait_mask(s, net::READABLE as i32, deadline_in(3000));
    assert!(m & net::READABLE as i32 != 0, "net-stream: the server's mask after the send reads {}", m);
    assert!(api::rhu_net_available(s) == 26, "net-stream: the available door reads {}", api::rhu_net_available(s));
    let mut buf = [0u8; 64];
    assert!(api::rhu_net_recv(s, buf.as_mut_ptr(), 64, net::RECV_PEEK, core::ptr::null_mut()) == 26 && &buf[..26] == alphabet, "net-stream: the peek");
    assert!(api::rhu_net_available(s) == 26, "net-stream: the peek consumed bytes");
    buf = [0; 64];
    assert!(api::rhu_net_recv(s, buf.as_mut_ptr(), 64, 0, core::ptr::null_mut()) == 26 && &buf[..26] == alphabet, "net-stream: the read");
    assert!(api::rhu_net_available(s) == 0 && api::rhu_net_recv(s, buf.as_mut_ptr(), 64, 0, core::ptr::null_mut()) == sched::ERR_AGAIN, "net-stream: the drained socket");
    // 3000 patterned bytes out through the client, echoed by the server, read back byte-exact.
    let mut pat = [0u8; 3000];
    for (i, b) in pat.iter_mut().enumerate() {
        *b = (i * 7 + 3) as u8;
    }
    let mut back = [0u8; 3000];
    let (mut sent, mut echoed, mut served) = (0usize, 0usize, 0usize);
    let mut writes = 0u32;
    let deadline = deadline_in(8000);
    let mut tmp = [0u8; 512];
    let (mut held, mut pos) = (0usize, 0usize); // the server's bytes read and not yet echoed, and where they sit in tmp
    while echoed < 3000 {
        assert!(sched::now_ns() < deadline, "net-stream: the echo of 3000 bytes did not complete in 8 s (sent {} served {} echoed {})", sent, served, echoed);
        let mut moved = false;
        if sent < 3000 {
            let r = api::rhu_net_send(c, pat[sent..].as_ptr(), (3000 - sent) as u32, core::ptr::null());
            assert!(r > 0 || r == sched::ERR_AGAIN, "net-stream: the client's send answered {}", r);
            if r > 0 {
                sent += r as usize;
                writes += 1;
                moved = true;
            }
        }
        if held == 0 {
            let r = api::rhu_net_recv(s, tmp.as_mut_ptr(), 512, 0, core::ptr::null_mut());
            assert!(r > 0 || r == sched::ERR_AGAIN, "net-stream: the server's read answered {}", r);
            if r > 0 {
                held = r as usize;
                pos = 0;
                moved = true;
            }
        }
        if held > 0 {
            let r = api::rhu_net_send(s, tmp[pos..pos + held].as_ptr(), held as u32, core::ptr::null());
            assert!(r > 0 || r == sched::ERR_AGAIN, "net-stream: the server's echo answered {}", r);
            if r > 0 {
                served += r as usize;
                pos += r as usize;
                held -= r as usize;
                moved = true;
            }
        }
        let r = api::rhu_net_recv(c, back[echoed..].as_mut_ptr(), (3000 - echoed) as u32, 0, core::ptr::null_mut());
        assert!(r > 0 || r == sched::ERR_AGAIN, "net-stream: the client's read answered {}", r);
        if r > 0 {
            echoed += r as usize;
            moved = true;
        }
        if !moved {
            sched::sleep_ms(1);
        }
    }
    assert!(back == pat, "net-stream: the 3000 echoed bytes differ from the pattern");
    kprintln!("[api] net-stream: 3000 bytes in {} client writes, echoed by the server in 512-byte reads, read back byte-exact", writes);
    // The half-close: the client's write side shut, the server reads the hang-up and a zero read.
    assert!(api::rhu_net_shutdown(c, net::SHUT_WR) == 0, "net-stream: the shutdown refused");
    let m = wait_mask(s, net::HANGUP as i32, deadline_in(3000));
    assert!(m & (net::READABLE | net::HANGUP) as i32 == (net::READABLE | net::HANGUP) as i32 && m & net::ERROR as i32 == 0, "net-stream: the server's mask after the peer's half-close reads {}", m);
    assert!(api::rhu_net_recv(s, buf.as_mut_ptr(), 64, 0, core::ptr::null_mut()) == 0, "net-stream: the read after the peer's close was not 0");
    assert!(api::rhu_net_send(c, alphabet.as_ptr(), 1, core::ptr::null()) == net::ERR_PIPE, "net-stream: a send on the shut side not refused EPIPE");
    assert!(api::rhu_net_close(s) == 0 && api::rhu_net_close(c) == 0 && api::rhu_net_close(l) == 0, "net-stream: a close refused");
    assert!(api::rhu_net_close(c) == sched::ERR_INVAL && api::rhu_net_event_word(c).is_null(), "net-stream: the closed handle not refused");
    let w = witness();
    assert!(w.connects == 1 && w.accepts == 1 && w.bytes_out >= 6026 && w.bytes_in >= 6026, "net-stream: the witness counts connects {} accepts {} out {} in {}", w.connects, w.accepts, w.bytes_out, w.bytes_in);
    kprintln!("[api] PASS net-stream (a listener and a client over the loopback: the connect started with no readiness and a second connect EALREADY, writable at establishment with no error pending and a third connect EISCONN, the risen bits taken once and empty after, the accept with the peer's address and the client's own port, 26 bytes then 3000 patterned bytes echoed byte-exact through the peek and the available door, the peer's half-close read as hang-up and a zero read with EPIPE on the shut side, the three closed and the closed handle refused)");
}

/// Datagrams over the loopback: two bound sockets, a datagram with its
/// address record round-tripped, the peek and the available function, an
/// oversize datagram, the default peer, an unbound sender's own port.
fn check_net_datagram() {
    kprintln!("[api] net-datagram: two bound datagram sockets over the loopback -- a datagram with its address round-tripped, the peek and the available door, an oversize datagram refused, the default peer, an unbound sender's own port");
    let a = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
    let b = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 17);
    assert!(a >= 0 && b >= 0, "net-datagram: the sockets read {} {}", a, b);
    assert!(api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0) == net::ERR_MFILE, "net-datagram: a third datagram socket not refused EMFILE (the pool of record holds two)");
    assert!(api::rhu_net_bind(a, &addr(LEASE, 9000)) == 0 && api::rhu_net_bind(b, &addr([0, 0, 0, 0], 9001)) == 0, "net-datagram: a bind refused");
    assert!(api::rhu_net_readiness(a, 0) == net::WRITABLE as i32 || wait_mask(a, net::WRITABLE as i32, deadline_in(1000)) == net::WRITABLE as i32, "net-datagram: a bound socket not writable");
    let mut msg = [0u8; 64];
    for (i, m) in msg.iter_mut().enumerate() {
        *m = (i * 3) as u8;
    }
    assert!(api::rhu_net_send(a, msg.as_ptr(), 64, &addr(LEASE, 9001)) == 64, "net-datagram: the send did not take the datagram");
    let m = wait_mask(b, net::READABLE as i32, deadline_in(3000));
    assert!(m & net::READABLE as i32 != 0, "net-datagram: the receiver's mask reads {}", m);
    assert!(api::rhu_net_available(b) == 64, "net-datagram: the available door reads {}", api::rhu_net_available(b));
    let mut from = none_addr();
    let mut buf = [0u8; 128];
    assert!(api::rhu_net_recv(b, buf.as_mut_ptr(), 128, net::RECV_PEEK, &mut from) == 64 && buf[..64] == msg && ip_of(&from) == LEASE && from.port == 9000, "net-datagram: the peek with the sender's address");
    assert!(api::rhu_net_available(b) == 64, "net-datagram: the peek consumed the datagram");
    from = none_addr();
    buf = [0; 128];
    assert!(api::rhu_net_recv(b, buf.as_mut_ptr(), 128, 0, &mut from) == 64 && buf[..64] == msg && ip_of(&from) == LEASE && from.port == 9000, "net-datagram: the read with the sender's address");
    assert!(api::rhu_net_available(b) == 0 && api::rhu_net_recv(b, buf.as_mut_ptr(), 128, 0, &mut from) == sched::ERR_AGAIN, "net-datagram: the drained socket");
    let big = [0u8; 1600];
    assert!(api::rhu_net_send(a, big.as_ptr(), 1600, &addr(LEASE, 9001)) == net::ERR_MSGSIZE, "net-datagram: an oversize datagram not refused EMSGSIZE");
    // The default peer, and a datagram received into a buffer too small.
    assert!(api::rhu_net_connect(a, &addr(LEASE, 9001)) == 0, "net-datagram: the default peer refused");
    assert!(api::rhu_net_send(a, msg.as_ptr(), 32, core::ptr::null()) == 32, "net-datagram: the send to the default peer");
    assert!(wait_mask(b, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-datagram: the second datagram did not arrive");
    assert!(api::rhu_net_recv(b, buf.as_mut_ptr(), 16, 0, core::ptr::null_mut()) == net::ERR_MSGSIZE, "net-datagram: a buffer smaller than the datagram not refused EMSGSIZE");
    assert!(api::rhu_net_available(b) == 0, "net-datagram: the truncated datagram was not discarded");
    let (mut ul, mut up) = (none_addr(), none_addr());
    assert!(api::rhu_net_name(a, &mut ul, &mut up) == 0 && ul.port == 9000 && up.port == 9001 && ip_of(&up) == LEASE, "net-datagram: the names of the connected socket");
    // An unbound sender takes a port of the row's choosing: the connected socket is closed first and the sender
    // opens in its slot (the pool of record holds two datagram sockets).
    assert!(api::rhu_net_close(a) == 0, "net-datagram: the connected socket's close refused");
    let u = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
    assert!(u >= 0, "net-datagram: the unbound sender's socket read {}", u);
    assert!(api::rhu_net_send(u, msg.as_ptr(), 8, &addr(LEASE, 9001)) == 8, "net-datagram: the unbound sender's send");
    assert!(wait_mask(b, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-datagram: the unbound sender's datagram did not arrive");
    from = none_addr();
    assert!(api::rhu_net_recv(b, buf.as_mut_ptr(), 128, 0, &mut from) == 8 && from.port >= 49152, "net-datagram: the unbound sender's port reads {}", from.port);
    assert!(api::rhu_net_name(u, &mut ul, &mut up) == 0 && ul.port as u32 == from.port && up.family == net::FAMILY_NONE, "net-datagram: the unbound sender's name reads port {} peer family {}", ul.port, up.family);
    assert!(api::rhu_net_close(b) == 0 && api::rhu_net_close(u) == 0, "net-datagram: a close refused");
    kprintln!("[api] PASS net-datagram (two bound datagram sockets over the loopback: a 64-byte datagram with the sender's address round-tripped through the peek and the available door, an oversize datagram EMSGSIZE, a buffer smaller than the datagram EMSGSIZE with the datagram discarded, the default peer set and used, an unbound sender given a port of the row's choosing and its name answered, the three closed)");
}

/// Every refusal code by its cause: the families and types, the pool spent,
/// a second bind, the unconnected stream, bad handles and words, the
/// peer's reset, the connect budget, the static address and the route.
fn check_net_refuse() {
    kprintln!("[api] net-refuse: every refusal by its cause -- IPv6, a type the row has not, the pool spent, a second bind, the unconnected stream, bad handles and words, the peer's reset, the connect budget, a static address and the route");
    assert!(api::rhu_net_socket(net::FAMILY_IPV6, net::TYPE_STREAM, 0) == net::ERR_AFNOSUPPORT, "net-refuse: IPv6 not refused EAFNOSUPPORT");
    assert!(api::rhu_net_socket(net::FAMILY_IPV4, 3, 0) == net::ERR_PROTONOSUPPORT && api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 17) == net::ERR_PROTONOSUPPORT, "net-refuse: a bad type or protocol not refused EPROTONOSUPPORT");
    // The stream pool: every closed stream's pair back once the stack removed its connection.
    let settled = wait_until(api::rhu_net_events_word(), deadline_in(3000), || if net::probe_streams_used() == 0 { Some(()) } else { None });
    assert!(settled.is_some(), "net-refuse: {} stream sockets still tied 3 s after their closes", net::probe_streams_used());
    let mut hs = [0i32; 4];
    for h in hs.iter_mut() {
        *h = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
        assert!(*h >= 0, "net-refuse: a stream of the pool refused {}", *h);
    }
    assert!(api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0) == net::ERR_MFILE, "net-refuse: the fifth stream not refused EMFILE");
    assert!(api::rhu_net_bind(hs[0], &addr(LEASE, 7000)) == 0 && api::rhu_net_bind(hs[1], &addr(LEASE, 7000)) == net::ERR_ADDRINUSE, "net-refuse: a second bind not refused EADDRINUSE");
    assert!(api::rhu_net_bind(hs[0], &addr(LEASE, 7001)) == sched::ERR_INVAL, "net-refuse: a bind on a bound stream not refused EINVAL");
    assert!(api::rhu_net_bind(hs[2], &addr(PEER, 7002)) == sched::ERR_INVAL, "net-refuse: a foreign address not refused EINVAL");
    let mut buf = [0u8; 8];
    assert!(api::rhu_net_recv(hs[2], buf.as_mut_ptr(), 8, 0, core::ptr::null_mut()) == net::ERR_NOTCONN && api::rhu_net_send(hs[2], buf.as_ptr(), 8, core::ptr::null()) == net::ERR_NOTCONN, "net-refuse: the unconnected stream not refused ENOTCONN");
    assert!(api::rhu_net_listen(hs[2], 1) == sched::ERR_INVAL && api::rhu_net_listen(hs[0], 0) == sched::ERR_INVAL, "net-refuse: a listen on an unbound stream or with a backlog of 0 not refused EINVAL");
    let d = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
    assert!(d >= 0 && api::rhu_net_listen(d, 1) == sched::ERR_INVAL && api::rhu_net_accept(d, core::ptr::null_mut()) == sched::ERR_INVAL, "net-refuse: a listen or accept on a datagram socket not refused EINVAL");
    assert!(api::rhu_net_option(d, net::OPT_NODELAY, 1, 1) == net::ERR_NOTSUP && api::rhu_net_option(hs[0], 9, 0, 0) == net::ERR_NOTSUP, "net-refuse: an unsupported option not refused ENOTSUP");
    assert!(api::rhu_net_option(hs[0], net::OPT_NODELAY, 1, 1) == 0 && api::rhu_net_option(hs[0], net::OPT_NODELAY, 0, 0) == 1 && api::rhu_net_option(hs[0], net::OPT_KEEPALIVE, 0, 0) == 0, "net-refuse: the option's read-back");
    assert!(api::rhu_net_option(hs[0], net::OPT_NODELAY, 2, 0) == sched::ERR_INVAL && api::rhu_net_shutdown(hs[0], 3) == sched::ERR_INVAL && api::rhu_net_readiness(hs[0], 2) == sched::ERR_INVAL && api::rhu_net_recv(hs[0], buf.as_mut_ptr(), 8, 2, core::ptr::null_mut()) == sched::ERR_INVAL, "net-refuse: bad words not refused EINVAL");
    assert!(api::rhu_net_shutdown(hs[0], net::SHUT_RDWR) == 0 && api::rhu_net_shutdown(d, net::SHUT_WR) == 0, "net-refuse: a shutdown on an unconnected handle refused");
    for bad in [-1i32, 7, 99] {
        assert!(
            api::rhu_net_bind(bad, &addr(LEASE, 1)) == sched::ERR_INVAL
                && api::rhu_net_listen(bad, 1) == sched::ERR_INVAL
                && api::rhu_net_accept(bad, core::ptr::null_mut()) == sched::ERR_INVAL
                && api::rhu_net_connect(bad, &addr(LEASE, 1)) == sched::ERR_INVAL
                && api::rhu_net_send(bad, buf.as_ptr(), 1, core::ptr::null()) == sched::ERR_INVAL
                && api::rhu_net_recv(bad, buf.as_mut_ptr(), 1, 0, core::ptr::null_mut()) == sched::ERR_INVAL
                && api::rhu_net_available(bad) == sched::ERR_INVAL
                && api::rhu_net_shutdown(bad, 0) == sched::ERR_INVAL
                && api::rhu_net_close(bad) == sched::ERR_INVAL
                && api::rhu_net_readiness(bad, 0) == sched::ERR_INVAL
                && api::rhu_net_error(bad) == sched::ERR_INVAL
                && api::rhu_net_event_word(bad).is_null()
                && api::rhu_net_name(bad, &mut none_addr(), core::ptr::null_mut()) == sched::ERR_INVAL
                && api::rhu_net_option(bad, 1, 0, 0) == sched::ERR_INVAL,
            "net-refuse: the bad handle {} not refused at every door",
            bad
        );
    }
    assert!(api::rhu_net_connect(hs[1], core::ptr::null()) == sched::ERR_INVAL, "net-refuse: a null address not refused EINVAL");
    let mut v6 = addr([0; 4], 80);
    v6.family = net::FAMILY_IPV6;
    assert!(api::rhu_net_connect(hs[1], &v6) == net::ERR_AFNOSUPPORT, "net-refuse: an IPv6 peer not refused EAFNOSUPPORT");
    assert!(api::rhu_net_connect(hs[1], &addr(LEASE, 0)) == net::ERR_NETUNREACH, "net-refuse: a port of 0 not refused ENETUNREACH");
    // The peer's reset: a connect to its one port ends refused on the wire.
    assert!(api::rhu_net_connect(hs[1], &addr(PEER, REFUSED_PORT)) == net::ERR_INPROGRESS, "net-refuse: the connect to the peer did not start");
    let m = wait_mask(hs[1], net::ERROR as i32, deadline_in(net::CONNECT_BUDGET_MS + 2000));
    assert!(m == (net::WRITABLE | net::ERROR | net::HANGUP) as i32, "net-refuse: the refused connect's mask reads {}", m);
    let code = api::rhu_net_error(hs[1]);
    assert!(code == net::ERR_CONNREFUSED, "net-refuse: the connect to the peer's port read {} where ECONNREFUSED was expected", code);
    assert!(api::rhu_net_error(hs[1]) == 0 && api::rhu_net_send(hs[1], buf.as_ptr(), 1, core::ptr::null()) == net::ERR_NOTCONN, "net-refuse: the code not consumed once, or the failed stream not ENOTCONN after");
    assert!(net::probe_medium().1.resets == 1, "net-refuse: the peer answered {} resets where 1 was expected", net::probe_medium().1.resets);
    // The connect budget: an address on the segment nobody owns -- the hardware address never resolves.
    let t0 = sched::now_ms();
    assert!(api::rhu_net_connect(hs[3], &addr(UNOWNED, 80)) == net::ERR_INPROGRESS, "net-refuse: the connect to the unowned address did not start");
    let m = wait_mask(hs[3], net::ERROR as i32, deadline_in(net::CONNECT_BUDGET_MS + 2000));
    let dt = sched::now_ms().wrapping_sub(t0);
    assert!(m == (net::WRITABLE | net::ERROR | net::HANGUP) as i32, "net-refuse: the unanswered connect's mask reads {} after {} ms", m, dt);
    let code = api::rhu_net_error(hs[3]);
    assert!(code == net::ERR_HOSTUNREACH, "net-refuse: the unanswered connect's code reads {} where EHOSTUNREACH was expected", code);
    assert!(dt >= net::CONNECT_BUDGET_MS && dt < net::CONNECT_BUDGET_MS + 1500, "net-refuse: the budget ended at {} ms against {}", dt, net::CONNECT_BUDGET_MS);
    kprintln!("[api] net-refuse: the peer's reset read ECONNREFUSED at once; the unowned address read EHOSTUNREACH at the budget after {} ms", dt);
    // A static address without a gateway, the route refused off the segment, the DHCP client armed again.
    let mw = api::rhu_net_medium_word(0);
    let before = unsafe { read_volatile(mw) };
    let fixed = net::Lease { ip4: word(LEASE), mask: word(MASK), gateway: 0, dns: [word(PEER), 0], source: 0 };
    assert!(api::rhu_net_configure(0, &fixed) == 0, "net-refuse: the static configuration refused");
    let mut lease = net::Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 };
    assert!(api::rhu_net_lease(0, &mut lease) == 0 && lease.source == net::SOURCE_STATIC && lease.gateway == 0 && lease.ip4 == word(LEASE), "net-refuse: the static record reads source {} gateway {:08x}", lease.source, lease.gateway);
    assert!(unsafe { read_volatile(mw) } != before, "net-refuse: the medium word did not move for the static address");
    assert!(api::rhu_net_connect(hs[2], &addr(OFF_SEGMENT, 80)) == net::ERR_NETUNREACH, "net-refuse: a destination off the segment with no gateway not refused ENETUNREACH");
    let bad = net::Lease { ip4: word(LEASE), mask: word([255, 0, 255, 0]), gateway: 0, dns: [0; 2], source: 0 };
    assert!(api::rhu_net_configure(0, &bad) == sched::ERR_INVAL && api::rhu_net_configure(1, core::ptr::null()) == sched::ERR_INVAL, "net-refuse: a mask that is not a prefix, or a medium past the count, not refused EINVAL");
    assert!(api::rhu_net_configure(0, core::ptr::null()) == 0, "net-refuse: the DHCP client not armed again");
    let got = wait_until(mw, deadline_in(4000), || {
        let r = api::rhu_net_lease(0, &mut lease);
        if r == 0 && lease.source == net::SOURCE_DHCP { Some(()) } else { None }
    });
    assert!(got.is_some() && lease.gateway == word(PEER), "net-refuse: no lease by DHCP within 4 s of arming the client again (source {})", lease.source);
    for h in hs {
        assert!(api::rhu_net_close(h) == 0, "net-refuse: a close refused");
    }
    assert!(api::rhu_net_close(d) == 0, "net-refuse: the datagram close refused");
    kprintln!("[api] PASS net-refuse (IPv6 EAFNOSUPPORT, a bad type and protocol EPROTONOSUPPORT, the fifth stream EMFILE, a second bind EADDRINUSE, a bound or foreign bind EINVAL, the unconnected stream ENOTCONN, a listen or accept on a datagram socket EINVAL, unsupported options ENOTSUP with the read-back form proven, bad words and three bad handles EINVAL at every door, a null and an IPv6 peer refused, the peer's reset ECONNREFUSED read once, the unowned address EHOSTUNREACH at the connect budget, a static address without a gateway with the off-segment connect ENETUNREACH and a bad mask EINVAL, the DHCP client armed again with its lease)");
}

static WOKEN: AtomicU32 = AtomicU32::new(0);
static SNAP: AtomicU32 = AtomicU32::new(0);

extern "C" fn port_waiter(_: usize) {
    let w = api::rhu_net_events_word();
    let r = api::rhu_futex_wait(w, SNAP.load(Ordering::Acquire), sched::now_ns() + 2_000_000_000);
    WOKEN.store(if r == 0 { 1 } else { 2 }, Ordering::Release);
}

/// The event port's properties: the bump function moves the global word by one
/// and wakes a parked waiter; an idle connected stream makes no bump (no
/// storm: a waiter on the global word sleeps to its deadline).
fn check_net_port() {
    kprintln!("[api] net-port: the bump door moves the global word by one and wakes a parked waiter; an idle connected stream makes no bump across two of the thread's safety deadlines (1100 ms)");
    let gw = api::rhu_net_events_word();
    assert!(!gw.is_null(), "net-port: the global word is null");
    // A connected pair, idle.
    let l = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let c = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    assert!(l >= 0 && c >= 0 && api::rhu_net_bind(l, &addr(LEASE, 7778)) == 0 && api::rhu_net_listen(l, 1) == 0 && api::rhu_net_connect(c, &addr(LEASE, 7778)) == net::ERR_INPROGRESS, "net-port: the pair's setup refused");
    assert!(wait_mask(c, net::WRITABLE as i32, deadline_in(3000)) == net::WRITABLE as i32 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-port: the pair did not connect");
    let s = api::rhu_net_accept(l, core::ptr::null_mut());
    assert!(s >= 0, "net-port: the accept answered {}", s);
    let _ = wait_mask(s, net::WRITABLE as i32, deadline_in(3000));
    sched::sleep_ms(20); // the pump's passes after the handshake settle
    // The storm property: no kernel-caused bump without a mask change.
    let snap = unsafe { read_volatile(gw) };
    // The window spans two of the thread's safety deadlines: an idle stack passes only at that deadline, so a
    // shorter window could hold no pass at all and a storming thread would hide in it by chance.
    let window_ms = net::SAFETY_MS * 2 + 100;
    let r = api::rhu_futex_wait(gw, snap, deadline_in(window_ms));
    let moved = unsafe { read_volatile(gw) }.wrapping_sub(snap);
    assert!(r == sched::ERR_TIMEDOUT && moved == 0, "net-port: the global word moved {} times in {} ms with no socket changing; a wake without a mask change is a storm (the wait answered {})", moved, window_ms, r);
    // The bump function: the word by one, a parked waiter woken.
    SNAP.store(unsafe { read_volatile(gw) }, Ordering::Release);
    WOKEN.store(0, Ordering::Release);
    let _ = sched::spawn("port-wait", port_waiter, 0, 5, 2048);
    sched::sleep_ms(10);
    assert!(WOKEN.load(Ordering::Acquire) == 0, "net-port: the waiter did not park");
    api::rhu_net_events_bump();
    sched::sleep_ms(10);
    let after = unsafe { read_volatile(gw) };
    assert!(after.wrapping_sub(SNAP.load(Ordering::Acquire)) == 1, "net-port: the bump moved the word by {}", after.wrapping_sub(SNAP.load(Ordering::Acquire)));
    assert!(WOKEN.load(Ordering::Acquire) == 1, "net-port: the parked waiter was not woken by the bump (state {})", WOKEN.load(Ordering::Acquire));
    assert!(api::rhu_net_close(s) == 0 && api::rhu_net_close(c) == 0 && api::rhu_net_close(l) == 0, "net-port: a close refused");
    let w = witness();
    kprintln!("[api] net-port: {} mask changes and {} global bumps over the family's checks; the pump's final statistics: {} passes, the longest {} ms, the latest deadline wake {} ms late", w.mask_changes, w.global_bumps, w.pump_passes, w.pump_max_pass_ms, w.pump_max_late_ms);
    kprintln!("[api] PASS net-port (an idle connected stream moved the global word not once across two of the thread's safety deadlines (1100 ms), the waiter sleeping to its deadline; the bump door moved the word by exactly one and woke the parked waiter; the pair closed)");
}

/// The pool's stream sockets in use read `n` inside the bound: the network
/// thread returns an ended stream's socket at its next pass.
#[cfg(any(feature = "test-net-link-drop", feature = "test-net-listen-again", feature = "test-net-listen-reset"))]
fn wait_pool(n: u32, ms: u32) -> bool {
    let t0 = sched::now_ms();
    loop {
        if net::probe_streams_used() == n {
            return true;
        }
        if sched::now_ms().wrapping_sub(t0) >= ms {
            return false;
        }
        sched::sleep_ms(10);
    }
}

/// The virtual medium's link set and the header's recipe on the medium's
/// word: the word read, the link set, the futex wait with a 1,000 ms
/// deadline. Answers the wait's code and whether the word moved.
#[cfg(feature = "test-net-link-drop")]
fn set_link_and_wait(up: bool) -> (i32, bool) {
    let mw = api::rhu_net_medium_word(0);
    let before = unsafe { read_volatile(mw) };
    net::probe_set_link(up);
    let r = api::rhu_futex_wait(mw, before, deadline_in(1000));
    (r, unsafe { read_volatile(mw) } != before)
}

/// The link that changes (`test-net-link-drop`, the api gate's link case):
/// the virtual medium's link dropped and raised under open sockets. The
/// medium's word moves and wakes its waiter at every change; a connected
/// pair reads RHU_ECONNRESET at both ends and a connecting stream
/// RHU_ENETDOWN; the listener and the datagram socket are kept and serve
/// after the return; the lease is asked again by DHCP at the return and a
/// static address is left alone; a connection behind the listener that
/// nobody accepted is dropped and its socket listens again. The doctored
/// thread (`test-net-doctored-link`) reads no link change, and the first
/// wait on the medium's word refuses it.
#[cfg(feature = "test-net-link-drop")]
fn check_net_link() {
    kprintln!("[api] net-link: the virtual medium's link dropped and raised under open sockets -- the medium's word, a connected pair and a connecting stream ended with their codes, the listener and the datagram socket kept, the lease asked again at the return, a static address left alone, a connection nobody accepted");
    const BOTH: i32 = (net::READABLE | net::WRITABLE | net::HANGUP | net::ERROR) as i32;
    const FAILED: i32 = (net::WRITABLE | net::ERROR | net::HANGUP) as i32;
    let stream = || api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let four = *b"link";
    let mut buf = [0u8; 8];
    // The pool whole: the earlier checks' ended streams have left the stack.
    assert!(wait_pool(0, 2000), "net-link: {} stream sockets still in use 2000 ms after the earlier checks", net::probe_streams_used());
    let mut lease = net::Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 };
    assert!(api::rhu_net_lease(0, &mut lease) == 0 && lease.source == net::SOURCE_DHCP && lease.ip4 == word(LEASE), "net-link: the address is not held by DHCP at the start (source {})", lease.source);
    // The setup: a listener with one connection accepted, a client connecting to the address nobody owns, a datagram socket.
    let l = stream();
    let c = stream();
    assert!(l >= 0 && c >= 0 && api::rhu_net_bind(l, &addr(LEASE, 7801)) == 0 && api::rhu_net_listen(l, 1) == 0 && api::rhu_net_connect(c, &addr(LEASE, 7801)) == net::ERR_INPROGRESS, "net-link: the pair's setup refused");
    assert!(wait_mask(c, net::WRITABLE as i32, deadline_in(3000)) == net::WRITABLE as i32 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-link: the pair did not connect");
    let s = api::rhu_net_accept(l, core::ptr::null_mut());
    assert!(s >= 0, "net-link: the accept answered {}", s);
    let _ = wait_mask(s, net::WRITABLE as i32, deadline_in(3000));
    let k = stream();
    assert!(k >= 0 && api::rhu_net_connect(k, &addr(UNOWNED, 7802)) == net::ERR_INPROGRESS, "net-link: the connect to the address nobody owns did not start");
    let d = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
    assert!(d >= 0 && api::rhu_net_bind(d, &addr(LEASE, 7803)) == 0, "net-link: the datagram socket's setup refused");
    sched::sleep_ms(20); // the thread's passes after the handshakes settle
    assert!(net::probe_streams_used() == 4, "net-link: the pool reads {} stream sockets in use after the setup where 4 were expected", net::probe_streams_used());
    // The drop: the medium's word wakes its waiter, the info function and the witness read the link down.
    let (r, moved) = set_link_and_wait(false);
    assert!(r == 0 && moved, "net-link: the medium's word did not move within 1000 ms of the link's drop (the wait answered {})", r);
    let mut info = net::MediumInfo { kind: 9, link: 9, hw: [0; 6], reserved: 0 };
    assert!(api::rhu_net_medium(0, &mut info) == 0 && info.link == 0 && witness().link == 0, "net-link: after the drop the info function reads link {} and the witness {}", info.link, witness().link);
    // The connected streams, both ends: the mask a peer's reset gives, and its code.
    for (h, end) in [(c, "client"), (s, "accepted stream")] {
        let m = wait_mask(h, net::ERROR as i32, deadline_in(1000));
        let code = api::rhu_net_error(h);
        assert!(m == BOTH && code == net::ERR_CONNRESET, "net-link: the {}'s mask reads {} and its code {} after the drop where 15 and ECONNRESET (-104) were expected", end, m, code);
    }
    // The connecting stream; then a connect that starts while the link is down.
    let m = wait_mask(k, net::ERROR as i32, deadline_in(1000));
    let code = api::rhu_net_error(k);
    assert!(m == FAILED && code == net::ERR_NETDOWN, "net-link: the connecting stream's mask reads {} and its code {} after the drop where 14 and ENETDOWN (-100) were expected", m, code);
    assert!(api::rhu_net_close(k) == 0 && wait_pool(3, 1000), "net-link: the failed stream's socket did not return to the pool ({} in use)", net::probe_streams_used());
    let n = stream();
    let r = if n >= 0 { api::rhu_net_connect(n, &addr(LEASE, 7801)) } else { n };
    assert!(r == net::ERR_NETDOWN, "net-link: a connect while the link is down answered {} where ENETDOWN (-100) was expected", r);
    // Kept: the listener and the datagram socket, nothing pending on either.
    let (lm, le, de) = (api::rhu_net_readiness(l, 0), api::rhu_net_error(l), api::rhu_net_error(d));
    assert!(lm == 0 && le == 0 && de == 0, "net-link: after the drop the listener's mask reads {} with the code {} and the datagram socket's code {} where all three are 0", lm, le, de);
    // The return: the word, the link, and the lease asked again (the peer counts a new discover).
    let discovers = net::probe_medium().1.discovers;
    let (r, moved) = set_link_and_wait(true);
    assert!(r == 0 && moved, "net-link: the medium's word did not move within 1000 ms of the link's return (the wait answered {})", r);
    assert!(api::rhu_net_medium(0, &mut info) == 0 && info.link == 1 && witness().link == 1, "net-link: after the return the info function reads link {} and the witness {}", info.link, witness().link);
    let back = wait_until(api::rhu_net_medium_word(0), deadline_in(3000), || (api::rhu_net_lease(0, &mut lease) == 0 && lease.source == net::SOURCE_DHCP && lease.ip4 == word(LEASE) && net::probe_medium().1.discovers > discovers).then_some(()));
    assert!(back.is_some(), "net-link: the lease was not back by DHCP within 3000 ms of the return (source {}, {} discovers against {} before it)", lease.source, net::probe_medium().1.discovers, discovers);
    // Usable after the return: the listener accepts a new client and four bytes cross; the datagram socket reads its own four bytes.
    assert!(api::rhu_net_close(c) == 0 && api::rhu_net_close(s) == 0 && wait_pool(2, 1000), "net-link: the two ended streams' sockets did not return to the pool ({} in use)", net::probe_streams_used());
    assert!(api::rhu_net_connect(n, &addr(LEASE, 7801)) == net::ERR_INPROGRESS, "net-link: the connect after the return did not start");
    assert!(wait_mask(n, net::WRITABLE as i32, deadline_in(3000)) == net::WRITABLE as i32 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-link: the pair after the return did not connect");
    let s2 = api::rhu_net_accept(l, core::ptr::null_mut());
    assert!(s2 >= 0, "net-link: the accept after the return answered {}", s2);
    assert!(api::rhu_net_send(n, four.as_ptr(), 4, core::ptr::null()) == 4 && wait_mask(s2, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-link: the stream's four bytes did not arrive after the return");
    assert!(api::rhu_net_recv(s2, buf.as_mut_ptr(), 8, 0, core::ptr::null_mut()) == 4 && buf[..4] == four, "net-link: the stream's four bytes read unequal after the return");
    assert!(api::rhu_net_send(d, four.as_ptr(), 4, &addr(LEASE, 7803)) == 4 && wait_mask(d, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-link: the datagram did not come back to its socket after the return");
    buf = [0; 8];
    assert!(api::rhu_net_recv(d, buf.as_mut_ptr(), 8, 0, core::ptr::null_mut()) == 4 && buf[..4] == four, "net-link: the datagram's four bytes read unequal after the return");
    // A static address is left alone: the drop ends the pair like any connection, and the return asks nobody for a lease.
    let fixed = net::Lease { ip4: lease.ip4, mask: lease.mask, gateway: lease.gateway, dns: lease.dns, source: 0 };
    assert!(api::rhu_net_configure(0, &fixed) == 0, "net-link: the static configuration refused");
    let (changes, discovers) = (witness().lease_changes, net::probe_medium().1.discovers);
    let (r, moved) = set_link_and_wait(false);
    assert!(r == 0 && moved, "net-link: the medium's word did not move within 1000 ms of the second drop (the wait answered {})", r);
    for (h, end) in [(n, "client"), (s2, "accepted stream")] {
        let m = wait_mask(h, net::ERROR as i32, deadline_in(1000));
        let code = api::rhu_net_error(h);
        assert!(m == BOTH && code == net::ERR_CONNRESET, "net-link: the second pair's {} reads the mask {} and the code {} after the second drop where 15 and ECONNRESET (-104) were expected", end, m, code);
    }
    let (r, moved) = set_link_and_wait(true);
    assert!(r == 0 && moved, "net-link: the medium's word did not move within 1000 ms of the second return (the wait answered {})", r);
    sched::sleep_ms(200);
    let (w, now) = (witness(), net::probe_medium().1.discovers);
    assert!(
        now == discovers && api::rhu_net_lease(0, &mut lease) == 0 && lease.source == net::SOURCE_STATIC && lease.ip4 == fixed.ip4 && lease.mask == fixed.mask && lease.gateway == fixed.gateway && lease.dns == fixed.dns && w.lease_changes == changes,
        "net-link: under a static address the drop and the return moved the lease (source {}, {} lease changes against {}, {} discovers against {})",
        lease.source,
        w.lease_changes,
        changes,
        now,
        discovers
    );
    assert!(api::rhu_net_close(n) == 0 && api::rhu_net_close(s2) == 0 && wait_pool(1, 1000), "net-link: the second pair's sockets did not return to the pool ({} in use)", net::probe_streams_used());
    // A connection nobody accepted: the drop ends it and its socket listens again.
    let c3 = stream();
    assert!(c3 >= 0 && api::rhu_net_connect(c3, &addr(LEASE, 7801)) == net::ERR_INPROGRESS, "net-link: the connect that is not accepted did not start");
    assert!(wait_mask(c3, net::WRITABLE as i32, deadline_in(3000)) == net::WRITABLE as i32 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-link: the connection that is not accepted did not arrive at the listener");
    sched::sleep_ms(20);
    let (r, moved) = set_link_and_wait(false);
    assert!(r == 0 && moved, "net-link: the medium's word did not move within 1000 ms of the third drop (the wait answered {})", r);
    let (lm, again) = (api::rhu_net_readiness(l, 0), api::rhu_net_accept(l, core::ptr::null_mut()));
    let m = wait_mask(c3, net::ERROR as i32, deadline_in(1000));
    let code = api::rhu_net_error(c3);
    assert!(lm == 0 && again == sched::ERR_AGAIN && m == BOTH && code == net::ERR_CONNRESET, "net-link: after the drop under a connection nobody accepted the listener's mask reads {} and its accept {}, the client's mask {} and its code {} where 0, EAGAIN (-11), 15 and ECONNRESET (-104) were expected", lm, again, m, code);
    let (r, moved) = set_link_and_wait(true);
    assert!(r == 0 && moved, "net-link: the medium's word did not move within 1000 ms of the third return (the wait answered {})", r);
    assert!(api::rhu_net_close(c3) == 0, "net-link: the ended client's close refused");
    let c4 = stream();
    assert!(c4 >= 0 && api::rhu_net_connect(c4, &addr(LEASE, 7801)) == net::ERR_INPROGRESS && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-link: the listener took no connection after the drop that ended the one nobody accepted");
    let s4 = api::rhu_net_accept(l, core::ptr::null_mut());
    assert!(s4 >= 0, "net-link: the accept after the third return answered {}", s4);
    assert!(api::rhu_net_close(c4) == 0 && api::rhu_net_close(s4) == 0 && api::rhu_net_close(l) == 0 && api::rhu_net_close(d) == 0 && wait_pool(0, 2000), "net-link: a close refused, or the pool not empty 2000 ms after the closes ({} in use)", net::probe_streams_used());
    kprintln!("[api] PASS net-link (the link dropped and raised three times under open sockets: the medium's word moved and woke its waiter at every change with the info function and the witness reading the link; a connected pair read ECONNRESET at both ends and a connecting stream ENETDOWN, a connect during the drop ENETDOWN at the call; the listener and the datagram socket kept and serving after the return; the lease asked again by DHCP at the return; under a static address the lease left alone; a connection nobody accepted dropped with the listener accepting the next)");
}

/// A name with two address records (`test-net-resolve-two`, the api gate's
/// resolver case): the scripted peer's name that has two is read back as
/// both, in the answer's order, where the caller allows four; as the first
/// alone where it allows one; as none where it allows none. A name with
/// one record still answers one, and the records past the count written
/// are not touched. The doctored resolver (`test-net-doctored-resolve`)
/// keeps one record of an answer, and the first read refuses it.
#[cfg(feature = "test-net-resolve-two")]
fn check_net_resolve_two() {
    kprintln!("[api] net-resolve-two: a name with two address records -- both read in the answer's order where four are allowed, the first where one is, none where none is; a name with one record; the records past the count untouched");
    let (w0, pc0) = (witness(), net::probe_medium().1);
    // One query by the header's recipe, its result read with `max`; the read releases the slot.
    let ask = |name: &str, out: &mut [net::Addr; 4], max: u32| -> i32 {
        *out = [none_addr(); 4];
        let h = api::rhu_net_resolve(name.as_ptr(), name.len() as u32);
        assert!(h >= 0, "net-resolve-two: the query for {} was refused {}", name, h);
        let w = api::rhu_net_resolve_word(h);
        assert!(!w.is_null(), "net-resolve-two: the word of handle {} is null", h);
        let p = if max == 0 { core::ptr::null_mut() } else { out.as_mut_ptr() };
        let r = wait_until(w, deadline_in(4000), || {
            let r = api::rhu_net_resolve_result(h, p, max);
            if r == sched::ERR_AGAIN { None } else { Some(r) }
        });
        assert!(r.is_some(), "net-resolve-two: no answer for {} within 4000 ms", name);
        assert!(api::rhu_net_resolve_result(h, core::ptr::null_mut(), 0) == sched::ERR_INVAL, "net-resolve-two: the slot of {} was not released by its read", name);
        r.unwrap_or(sched::ERR_TIMEDOUT)
    };
    let mut out = [none_addr(); 4];
    let r = ask(PAIR_NAME, &mut out, 4);
    assert!(r == 2, "net-resolve-two: the name with two address records read {} where 2 were expected", r);
    assert!(
        out[0].family == net::FAMILY_IPV4 && out[0].port == 0 && ip_of(&out[0]) == PAIR_IPS[0] && out[1].family == net::FAMILY_IPV4 && out[1].port == 0 && ip_of(&out[1]) == PAIR_IPS[1] && out[2].family == net::FAMILY_NONE,
        "net-resolve-two: the two records read {:?} and {:?} with the families {} and {} and the third record's family {}",
        ip_of(&out[0]),
        ip_of(&out[1]),
        out[0].family,
        out[1].family,
        out[2].family
    );
    let r = ask(PAIR_NAME, &mut out, 1);
    assert!(r == 1 && ip_of(&out[0]) == PAIR_IPS[0] && out[1].family == net::FAMILY_NONE, "net-resolve-two: with one record allowed the name read {} {:?} and the second record's family {}", r, ip_of(&out[0]), out[1].family);
    let r = ask(PAIR_NAME, &mut out, 0);
    assert!(r == 0 && out[0].family == net::FAMILY_NONE, "net-resolve-two: with no record allowed the name read {} and the first record's family {}", r, out[0].family);
    let r = ask(NAMES[0].0, &mut out, 4);
    assert!(r == 1 && ip_of(&out[0]) == PEER && out[1].family == net::FAMILY_NONE, "net-resolve-two: the name with one record read {} {:?} and the second record's family {}", r, ip_of(&out[0]), out[1].family);
    let (w, pc) = (witness(), net::probe_medium().1);
    let (made, failed, answered, refused) = (w.resolves_made.wrapping_sub(w0.resolves_made), w.resolves_failed.wrapping_sub(w0.resolves_failed), pc.dns_answered.wrapping_sub(pc0.dns_answered), pc.dns_refused.wrapping_sub(pc0.dns_refused));
    assert!(made == 4 && failed == 0 && answered == 4 && refused == 0, "net-resolve-two: the counts moved by {} made, {} failed, {} answered and {} refused where 4, 0, 4 and 0 were expected", made, failed, answered, refused);
    kprintln!("[api] PASS net-resolve-two (a name with two address records answered both in the answer's order where four were allowed, the first alone where one was and none where none was, each read releasing its slot; a name with one record answered one; the records past the count untouched; four queries made and four answered)");
}

/// The second name server (`test-net-resolve-second`, the api gate's
/// second resolver case). A static record names two name servers: the
/// first is an address off the segment, so its queries leave by the
/// gateway and nobody answers them (the scripted peer drops and counts
/// them); the second is the peer. Two names are asked at once. The stack
/// asks the first server for its ten seconds and then the second, so the
/// name the peer has reads its record and the name it has not reads
/// RHU_EHOSTUNREACH -- the second server's answer and no timeout -- each
/// no sooner than 9500 ms and inside 15 000 ms. The DHCP client is armed
/// again behind it. The doctored stack (`test-net-doctored-dns2`) is
/// handed the first name server alone, and the first read refuses it.
#[cfg(feature = "test-net-resolve-second")]
fn check_net_resolve_second() {
    kprintln!("[api] net-resolve-second: a record with two name servers whose first never answers -- the second asked behind the first one's ten seconds, a name it has read as its record and a name it has not as EHOSTUNREACH");
    let mw = api::rhu_net_medium_word(0);
    let fixed = net::Lease { ip4: word(LEASE), mask: word(MASK), gateway: word(PEER), dns: [word(OFF_SEGMENT), word(PEER)], source: 0 };
    assert!(api::rhu_net_configure(0, &fixed) == 0, "net-resolve-second: the static record with two name servers was refused");
    let mut lease = net::Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 };
    assert!(api::rhu_net_lease(0, &mut lease) == 0 && lease.source == net::SOURCE_STATIC && lease.dns == fixed.dns, "net-resolve-second: the static record reads source {} with the name servers {:08x} and {:08x}", lease.source, lease.dns[0], lease.dns[1]);
    let (w0, pc0) = (witness(), net::probe_medium().1);
    // Both queries are filed before either is read: the first server's ten seconds pass once for the two.
    let ask = |name: &str| -> i32 {
        let h = api::rhu_net_resolve(name.as_ptr(), name.len() as u32);
        assert!(h >= 0, "net-resolve-second: the query for {} was refused {}", name, h);
        h
    };
    let t0 = sched::now_ms();
    let (h1, h2) = (ask(NAMES[0].0), ask(UNKNOWN_NAME));
    // A result by the header's recipe, with its age. The wait passes the two servers' budget, so a query that fails there reads its own code.
    let deadline_ns = deadline_in(25000);
    let read = |h: i32, out: &mut [net::Addr; 2]| -> (i32, u32) {
        *out = [none_addr(); 2];
        let w = api::rhu_net_resolve_word(h);
        assert!(!w.is_null(), "net-resolve-second: the word of handle {} is null", h);
        let r = wait_until(w, deadline_ns, || {
            let r = api::rhu_net_resolve_result(h, out.as_mut_ptr(), 2);
            if r == sched::ERR_AGAIN { None } else { Some(r) }
        });
        (r.unwrap_or(sched::ERR_AGAIN), sched::now_ms().wrapping_sub(t0))
    };
    let mut out = [none_addr(); 2];
    let (r1, dt1) = read(h1, &mut out);
    assert!(r1 == 1, "net-resolve-second: the name that the second name server has read {} where 1 record was expected (after {} ms)", r1, dt1);
    assert!(out[0].family == net::FAMILY_IPV4 && out[0].port == 0 && ip_of(&out[0]) == PEER && out[1].family == net::FAMILY_NONE, "net-resolve-second: the record read {:?} with the family {} and the second record's family {}", ip_of(&out[0]), out[0].family, out[1].family);
    assert!((9500..15000).contains(&dt1), "net-resolve-second: the record came after {} ms where the second name server is asked behind the first one's 10000 ms (9500 to 15000 ms)", dt1);
    let (r2, dt2) = read(h2, &mut out);
    assert!(r2 == net::ERR_HOSTUNREACH, "net-resolve-second: the name that the second name server has not read {} where EHOSTUNREACH was expected (after {} ms)", r2, dt2);
    assert!((9500..15000).contains(&dt2), "net-resolve-second: the name error came after {} ms where the second name server is asked behind the first one's 10000 ms (9500 to 15000 ms)", dt2);
    let (w, pc) = (witness(), net::probe_medium().1);
    let (made, failed, answered, refused, dropped) = (w.resolves_made.wrapping_sub(w0.resolves_made), w.resolves_failed.wrapping_sub(w0.resolves_failed), pc.dns_answered.wrapping_sub(pc0.dns_answered), pc.dns_refused.wrapping_sub(pc0.dns_refused), pc.dropped.wrapping_sub(pc0.dropped));
    assert!(made == 2 && failed == 1 && answered == 1 && refused == 1 && dropped >= 2, "net-resolve-second: the counts moved by {} made, {} failed, {} answered, {} refused and {} dropped by the peer where 2, 1, 1, 1 and at least 2 were expected", made, failed, answered, refused, dropped);
    kprintln!("[api] net-resolve-second: the record after {} ms and the name error after {} ms; {} queries for the first name server dropped by the peer", dt1, dt2, dropped);
    // The DHCP client armed again, the lease with its two name servers back.
    assert!(api::rhu_net_configure(0, core::ptr::null()) == 0, "net-resolve-second: the DHCP client not armed again");
    let got = wait_until(mw, deadline_in(4000), || {
        let r = api::rhu_net_lease(0, &mut lease);
        if r == 0 && lease.source == net::SOURCE_DHCP { Some(()) } else { None }
    });
    assert!(got.is_some() && lease.dns == [word(PEER), word(DNS2)], "net-resolve-second: no lease by DHCP with its two name servers within 4 s of arming the client again (source {})", lease.source);
    kprintln!("[api] PASS net-resolve-second (under a static record whose first name server never answers the second was asked behind the first one's ten seconds: a name it has answered with its record and a name it has not EHOSTUNREACH, each read between 9500 and 15000 ms; two queries made and one failed, the first name server's queries dropped by the peer; the DHCP client armed again with its lease)");
}

/// A listener's sockets read `n` inside the bound: the network thread
/// arms one for a listener left with none on the pass that frees a stream.
#[cfg(feature = "test-net-listen-again")]
fn wait_listening(l: i32, n: u32, ms: u32) -> bool {
    let t0 = sched::now_ms();
    loop {
        if net::probe_listening(l) == n {
            return true;
        }
        if sched::now_ms().wrapping_sub(t0) >= ms {
            return false;
        }
        sched::sleep_ms(10);
    }
}

/// The listener that listens again (`test-net-listen-again`, the api
/// gate's listener case). The pool is four stream sockets and every caller
/// is the row's own socket over the loopback, so the caller that cannot be
/// served is made before the pool is spent. An accept that finds the pool
/// spent leaves the listener with no socket; a caller meanwhile is refused
/// with a reset; the pass that returns a stream to the pool gives it to
/// the listener, and the next caller is accepted. The doctored thread
/// (`test-net-doctored-listen`) gives the listener no socket, and the wait
/// for the listener's socket refuses it.
#[cfg(feature = "test-net-listen-again")]
fn check_net_listen() {
    kprintln!("[api] net-listen: a listener whose accept finds the stream pool spent -- left with no socket, a caller refused with a reset meanwhile, listening again on the pass that returns a stream to the pool, the next caller accepted");
    const FAILED: i32 = (net::WRITABLE | net::ERROR | net::HANGUP) as i32;
    const PORT: u16 = 7811;
    let stream = || api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    let four = *b"back";
    let mut buf = [0u8; 32];
    // 1. The pool whole; a listener with a backlog of one.
    assert!(wait_pool(0, 2000), "net-listen: {} stream sockets still in use 2000 ms after the earlier checks", net::probe_streams_used());
    let w0 = witness();
    let l = stream();
    assert!(l >= 0 && api::rhu_net_bind(l, &addr(LEASE, PORT)) == 0 && api::rhu_net_listen(l, 1) == 0, "net-listen: the listener's setup refused");
    assert!(net::probe_listening(l) == 1, "net-listen: the new listener has {} sockets listening where 1 was expected", net::probe_listening(l));
    // 2. Three more streams: the pool spent.
    let (c1, x, c3) = (stream(), stream(), stream());
    let fifth = stream();
    assert!(c1 >= 0 && x >= 0 && c3 >= 0 && fifth == net::ERR_MFILE && net::probe_streams_used() == 4, "net-listen: the pool's setup read the handles {} {} {}, a fifth socket {} and {} streams in use where EMFILE (-24) and 4 were expected", c1, x, c3, fifth, net::probe_streams_used());
    // 3. The first caller.
    assert!(api::rhu_net_connect(c1, &addr(LEASE, PORT)) == net::ERR_INPROGRESS, "net-listen: the first caller's connect did not start");
    assert!(wait_mask(c1, net::WRITABLE as i32, deadline_in(3000)) == net::WRITABLE as i32 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-listen: the first caller did not connect");
    // 4. The accept finds the pool spent: the listener is left with no socket, and says nothing of it.
    let mut peer = none_addr();
    let (mut cl, mut cp) = (none_addr(), none_addr());
    let s1 = api::rhu_net_accept(l, &mut peer);
    assert!(s1 >= 0 && api::rhu_net_name(c1, &mut cl, &mut cp) == 0 && ip_of(&peer) == LEASE && peer.port == cl.port, "net-listen: the first accept answered {} with the peer's port {} against the caller's own {}", s1, peer.port, cl.port);
    let _ = wait_mask(s1, net::WRITABLE as i32, deadline_in(3000)); // the thread's pass after the accept
    let (n, lm, again, le) = (net::probe_listening(l), api::rhu_net_readiness(l, 0), api::rhu_net_accept(l, core::ptr::null_mut()), api::rhu_net_error(l));
    assert!(n == 0 && lm == 0 && again == sched::ERR_AGAIN && le == 0, "net-listen: after the accept that found the pool spent the listener has {} sockets, the mask {}, its accept {} and its code {} where 0, 0, EAGAIN (-11) and 0 were expected", n, lm, again, le);
    // 5. The caller that cannot be served: refused with a reset, well inside the connect budget.
    let t0 = sched::now_ms();
    assert!(api::rhu_net_connect(c3, &addr(LEASE, PORT)) == net::ERR_INPROGRESS, "net-listen: the second caller's connect did not start");
    let m = wait_mask(c3, net::ERROR as i32, deadline_in(1000));
    let (code, dt) = (api::rhu_net_error(c3), sched::now_ms().wrapping_sub(t0));
    assert!(m == FAILED && code == net::ERR_CONNREFUSED && dt < 1000, "net-listen: the caller while the listener had no socket read the mask {} and the code {} after {} ms where 14 and ECONNREFUSED (-111) inside 1000 ms were expected", m, code, dt);
    let (n, lm, le) = (net::probe_listening(l), api::rhu_net_readiness(l, 0), api::rhu_net_error(l));
    assert!(n == 0 && lm == 0 && le == 0, "net-listen: after the refused caller the listener has {} sockets, the mask {} and the code {} where all three are 0", n, lm, le);
    // 6. A stream returns to the pool: the listener listens again, on the pass that freed it.
    assert!(api::rhu_net_close(x) == 0, "net-listen: the idle stream's close refused");
    assert!(wait_listening(l, 1, 1000), "net-listen: the listener did not listen again within 1000 ms of a stream's return to the pool ({} sockets listening)", net::probe_listening(l));
    let (used, sixth) = (net::probe_streams_used(), stream());
    assert!(used == 4 && sixth == net::ERR_MFILE, "net-listen: after the listener took the returned stream the pool reads {} in use and a new socket {} where 4 and EMFILE (-24) were expected", used, sixth);
    // 7. The refused caller closed: its socket is free for the next caller.
    assert!(api::rhu_net_close(c3) == 0 && wait_pool(3, 1000), "net-listen: the refused caller's socket did not return to the pool ({} in use)", net::probe_streams_used());
    // 8. The next caller is served.
    let c4 = stream();
    assert!(c4 >= 0 && api::rhu_net_connect(c4, &addr(LEASE, PORT)) == net::ERR_INPROGRESS, "net-listen: the next caller's connect did not start ({})", c4);
    let m = wait_mask(c4, net::WRITABLE as i32, deadline_in(3000));
    assert!(m == net::WRITABLE as i32 && api::rhu_net_error(c4) == 0 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-listen: the next caller did not connect to the listener that listened again (its mask {})", m);
    let s4 = api::rhu_net_accept(l, &mut peer);
    assert!(s4 >= 0 && api::rhu_net_name(c4, &mut cl, &mut cp) == 0 && ip_of(&peer) == LEASE && peer.port == cl.port, "net-listen: the second accept answered {} with the peer's port {} against the caller's own {}", s4, peer.port, cl.port);
    let _ = wait_mask(s4, net::WRITABLE as i32, deadline_in(3000));
    assert!(net::probe_listening(l) == 0 && net::probe_streams_used() == 4, "net-listen: after the second accept the listener has {} sockets and the pool {} in use where 0 and 4 were expected", net::probe_listening(l), net::probe_streams_used());
    // 9. Bytes both ways on the new connection.
    assert!(api::rhu_net_send(c4, letters.as_ptr(), 26, core::ptr::null()) == 26 && wait_mask(s4, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-listen: the caller's 26 letters did not arrive");
    assert!(api::rhu_net_recv(s4, buf.as_mut_ptr(), 32, 0, core::ptr::null_mut()) == 26 && &buf[..26] == letters, "net-listen: the caller's 26 letters read unequal");
    assert!(api::rhu_net_send(s4, four.as_ptr(), 4, core::ptr::null()) == 4 && wait_mask(c4, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-listen: the accepted stream's four bytes did not arrive");
    buf = [0; 32];
    assert!(api::rhu_net_recv(c4, buf.as_mut_ptr(), 32, 0, core::ptr::null_mut()) == 4 && buf[..4] == four, "net-listen: the accepted stream's four bytes read unequal");
    // 10. That connection closed: the listener listens again a second time.
    assert!(api::rhu_net_close(c4) == 0 && api::rhu_net_close(s4) == 0, "net-listen: a close of the second connection refused");
    assert!(wait_listening(l, 1, 2000) && wait_pool(3, 2000), "net-listen: 2000 ms after the second connection's closes the listener has {} sockets and the pool {} in use where 1 and 3 were expected", net::probe_listening(l), net::probe_streams_used());
    // 11. The rest closed: the pool whole.
    assert!(api::rhu_net_close(c1) == 0 && api::rhu_net_close(s1) == 0 && api::rhu_net_close(l) == 0 && wait_pool(0, 2000), "net-listen: a close refused, or the pool not empty 2000 ms after the closes ({} in use)", net::probe_streams_used());
    let w = witness();
    let (accepts, connects) = (w.accepts.wrapping_sub(w0.accepts), w.connects.wrapping_sub(w0.connects));
    assert!(accepts == 2 && connects == 2, "net-listen: the witness counts {} accepts and {} connects in this check where 2 and 2 were expected", accepts, connects);
    kprintln!("[api] PASS net-listen (the accept that found the stream pool spent left the listener with no socket, its mask clear and no error on it; a caller meanwhile was refused with a reset, ECONNREFUSED pending inside one second; the listener listened again on the pass that returned a stream to the pool, the pool still spent; the next caller was accepted with its address and its bytes crossed both ways; the listener was left with no socket a second time and listened again; the pool whole at the end)");
}

/// A listener's sockets that listen read `n` inside the bound.
#[cfg(feature = "test-net-listen-reset")]
fn wait_backlog_listening(l: i32, n: u32, ms: u32) -> bool {
    let t0 = sched::now_ms();
    loop {
        if net::probe_backlog_listening(l) == n {
            return true;
        }
        if sched::now_ms().wrapping_sub(t0) >= ms {
            return false;
        }
        sched::sleep_ms(10);
    }
}

/// The reset before the accept (`test-net-listen-reset`, the api gate's
/// reset case). A caller connects to a listener with a backlog of one and
/// is reset before an accept takes it: the stack closes the listener's
/// socket, the network thread's next pass makes it listen again in its
/// place, and the next caller is accepted. Then the bind while a
/// connection is still closing: the accepted handle is closed while its
/// caller keeps its own end, the listener is closed, and a new socket
/// binds the same port and listens while the pool still counts the closing
/// socket; a caller is accepted on it. The doctored thread
/// (`test-net-doctored-listen`) puts no socket back, and the wait for the
/// listener's socket refuses it.
#[cfg(feature = "test-net-listen-reset")]
fn check_net_listen_reset() {
    kprintln!("[api] net-listen-reset: a caller reset before the accept -- the listener's socket that the stack closed listens again on the network thread's next pass and the next caller is accepted; then a bind to the port of a connection that is still closing");
    const PORT: u16 = 7812;
    let stream = || api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    let four = *b"back";
    let mut buf = [0u8; 32];
    // 1. The pool whole; a listener with a backlog of one.
    assert!(wait_pool(0, 2000), "net-listen-reset: {} stream sockets still in use 2000 ms after the earlier checks", net::probe_streams_used());
    let w0 = witness();
    let l = stream();
    assert!(l >= 0 && api::rhu_net_bind(l, &addr(LEASE, PORT)) == 0 && api::rhu_net_listen(l, 1) == 0, "net-listen-reset: the listener's setup refused");
    assert!(net::probe_backlog_listening(l) == 1, "net-listen-reset: the new listener has {} sockets listening where 1 was expected", net::probe_backlog_listening(l));
    // 2. A caller connects, and nobody accepts it.
    let c1 = stream();
    assert!(c1 >= 0 && api::rhu_net_connect(c1, &addr(LEASE, PORT)) == net::ERR_INPROGRESS, "net-listen-reset: the first caller's connect did not start ({})", c1);
    assert!(wait_mask(c1, net::WRITABLE as i32, deadline_in(3000)) == net::WRITABLE as i32 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-listen-reset: the first caller did not connect");
    assert!(net::probe_backlog_listening(l) == 0, "net-listen-reset: with its one connection waiting the listener has {} sockets listening where 0 was expected", net::probe_backlog_listening(l));
    // 3. The caller is reset before the accept: the listener's socket listens again, on the pass that read the reset.
    net::probe_reset(c1);
    assert!(wait_backlog_listening(l, 1, 1000), "net-listen-reset: the listener's socket did not listen again within 1000 ms of its caller's reset ({} sockets listening)", net::probe_backlog_listening(l));
    let (lm, again, le) = (api::rhu_net_readiness(l, 0), api::rhu_net_accept(l, core::ptr::null_mut()), api::rhu_net_error(l));
    assert!(lm == 0 && again == sched::ERR_AGAIN && le == 0, "net-listen-reset: behind the reset the listener reads the mask {}, its accept {} and its code {} where 0, EAGAIN (-11) and 0 were expected", lm, again, le);
    // 4. The reset caller's handle closed: the pool holds the listener's socket alone.
    assert!(api::rhu_net_close(c1) == 0 && wait_pool(1, 1000), "net-listen-reset: the reset caller's socket did not return to the pool ({} in use)", net::probe_streams_used());
    // 5. The next caller is accepted, and its bytes arrive.
    let c2 = stream();
    assert!(c2 >= 0 && api::rhu_net_connect(c2, &addr(LEASE, PORT)) == net::ERR_INPROGRESS, "net-listen-reset: the next caller's connect did not start ({})", c2);
    let m = wait_mask(c2, net::WRITABLE as i32, deadline_in(3000));
    assert!(m == net::WRITABLE as i32 && api::rhu_net_error(c2) == 0 && wait_mask(l, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-listen-reset: the next caller did not connect to the listener whose socket listened again (its mask {})", m);
    let mut peer = none_addr();
    let (mut cl, mut cp) = (none_addr(), none_addr());
    let s2 = api::rhu_net_accept(l, &mut peer);
    assert!(s2 >= 0 && api::rhu_net_name(c2, &mut cl, &mut cp) == 0 && ip_of(&peer) == LEASE && peer.port == cl.port, "net-listen-reset: the accept answered {} with the peer's port {} against the caller's own {}", s2, peer.port, cl.port);
    assert!(api::rhu_net_send(c2, letters.as_ptr(), 26, core::ptr::null()) == 26 && wait_mask(s2, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-listen-reset: the caller's 26 letters did not arrive");
    assert!(api::rhu_net_recv(s2, buf.as_mut_ptr(), 32, 0, core::ptr::null_mut()) == 26 && &buf[..26] == letters, "net-listen-reset: the caller's 26 letters read unequal");
    // 6. The accepted handle closed while its caller keeps its own end: that connection stays in its close, its socket counted.
    assert!(api::rhu_net_close(s2) == 0, "net-listen-reset: the accepted handle's close refused");
    sched::sleep_ms(200);
    assert!(net::probe_streams_used() == 3, "net-listen-reset: 200 ms behind the accepted handle's close the pool reads {} in use where 3 were expected (the listener's socket, the caller's and the closing one)", net::probe_streams_used());
    // 7. The listener closed: the caller's socket and the closing one remain.
    assert!(api::rhu_net_close(l) == 0 && wait_pool(2, 1000), "net-listen-reset: the listener's close refused, or its socket did not return to the pool ({} in use)", net::probe_streams_used());
    // 8. A new socket binds the same port and listens while the old connection is still closing.
    let l2 = stream();
    let (b, li, used) = (api::rhu_net_bind(l2, &addr(LEASE, PORT)), api::rhu_net_listen(l2, 1), net::probe_streams_used());
    assert!(l2 >= 0 && b == 0 && li == 0 && used == 3 && net::probe_backlog_listening(l2) == 1, "net-listen-reset: the bind to the port of a connection that is still closing answered {} and its listen {} with {} streams in use where 0, 0 and 3 were expected", b, li, used);
    // 9. A caller is accepted on the new listener, and bytes come back to it.
    let c3 = stream();
    assert!(c3 >= 0 && api::rhu_net_connect(c3, &addr(LEASE, PORT)) == net::ERR_INPROGRESS, "net-listen-reset: the new listener's caller did not start its connect ({})", c3);
    let m = wait_mask(c3, net::WRITABLE as i32, deadline_in(3000));
    assert!(m == net::WRITABLE as i32 && api::rhu_net_error(c3) == 0 && wait_mask(l2, net::READABLE as i32, deadline_in(3000)) == net::READABLE as i32, "net-listen-reset: the caller did not connect to the new listener (its mask {})", m);
    let s3 = api::rhu_net_accept(l2, &mut peer);
    assert!(s3 >= 0 && api::rhu_net_name(c3, &mut cl, &mut cp) == 0 && ip_of(&peer) == LEASE && peer.port == cl.port, "net-listen-reset: the new listener's accept answered {} with the peer's port {} against the caller's own {}", s3, peer.port, cl.port);
    assert!(api::rhu_net_send(s3, four.as_ptr(), 4, core::ptr::null()) == 4 && wait_mask(c3, net::READABLE as i32, deadline_in(3000)) & net::READABLE as i32 != 0, "net-listen-reset: the accepted stream's four bytes did not arrive");
    buf = [0; 32];
    assert!(api::rhu_net_recv(c3, buf.as_mut_ptr(), 32, 0, core::ptr::null_mut()) == 4 && buf[..4] == four, "net-listen-reset: the accepted stream's four bytes read unequal");
    // 10. Everything closed, the old caller first: the old connection ends its close and the pool is whole.
    assert!(api::rhu_net_close(c2) == 0 && api::rhu_net_close(c3) == 0 && api::rhu_net_close(s3) == 0 && api::rhu_net_close(l2) == 0 && wait_pool(0, 2000), "net-listen-reset: a close refused, or the pool not empty 2000 ms after the closes ({} in use)", net::probe_streams_used());
    let w = witness();
    let (accepts, connects) = (w.accepts.wrapping_sub(w0.accepts), w.connects.wrapping_sub(w0.connects));
    assert!(accepts == 2 && connects == 3, "net-listen-reset: the witness counts {} accepts and {} connects in this check where 2 and 3 were expected", accepts, connects);
    kprintln!("[api] PASS net-listen-reset (a caller reset before the accept left the listener's one socket closed by the stack; within one second it listened again in its place, the listener's mask clear, its accept EAGAIN and no error on it; the next caller was accepted and its 26 letters arrived; then, the accepted handle closed while its caller kept its own end, the listener closed and a new socket bound the same port and listened while the pool still counted the closing socket; a caller was accepted on it and four bytes came back; the pool whole at the end, 2 accepts and 3 connects on the witness)");
}

/// A bind to port 0 asks for any free port of the kind: the row gives one
/// from 49152 up by the allocator of an unbound connect or sender, the name
/// function reads it back, a listener bound so listens and accepts, and the
/// refusals stand (a second bind, a foreign address, which spends no port).
/// The control (`test-net-doctored-bind`) gives the port and records the
/// caller's 0, and the first name read refuses it.
fn check_net_bind_any() {
    kprintln!("[api] net-bind-any: a bind to port 0 over the loopback -- a stream and its client each given a port from 49152 up and read back, the refusals, the listener's accept, two datagram sockets given two ports");
    let settled = wait_until(api::rhu_net_events_word(), deadline_in(3000), || if net::probe_streams_used() == 0 { Some(()) } else { None });
    assert!(settled.is_some(), "net-bind-any: {} stream sockets still tied 3 s after their closes", net::probe_streams_used());
    let port_of = |h: i32| {
        let mut local = none_addr();
        assert!(api::rhu_net_name(h, &mut local, core::ptr::null_mut()) == 0, "net-bind-any: the name of handle {} refused", h);
        local.port
    };
    let l = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    let c = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_STREAM, 0);
    assert!(l >= 0 && c >= 0 && l != c, "net-bind-any: the stream sockets read {} and {}", l, c);
    assert!(api::rhu_net_bind(l, &addr(LEASE, 0)) == 0, "net-bind-any: a stream's bind to port 0 refused");
    let p1 = port_of(l);
    assert!((49152..=65535).contains(&p1), "net-bind-any: the stream bound to port 0 reads its port as {} where a port from 49152 up was expected", p1);
    assert!(api::rhu_net_bind(l, &addr(LEASE, 0)) == sched::ERR_INVAL && api::rhu_net_bind(l, &addr(LEASE, 7100)) == sched::ERR_INVAL && port_of(l) == p1, "net-bind-any: a second bind of the bound stream not refused EINVAL, or its port moved");
    assert!(api::rhu_net_bind(c, &addr(PEER, 0)) == sched::ERR_INVAL, "net-bind-any: a foreign address with port 0 not refused EINVAL");
    assert!(api::rhu_net_bind(c, &addr(LEASE, 0)) == 0, "net-bind-any: the client's bind to port 0 refused");
    let p2 = port_of(c);
    let next = if p1 == 65535 { 49152 } else { p1 + 1 };
    assert!(p2 == next, "net-bind-any: the client's port reads {} where {} was expected, the port after the listener's {} (the refused bind spent a port, or the allocator is another)", p2, next, p1);
    assert!(api::rhu_net_listen(l, 1) == 0, "net-bind-any: the listen on the given port refused");
    let r = api::rhu_net_connect(c, &addr(LEASE, p1 as u16));
    assert!(r == net::ERR_INPROGRESS, "net-bind-any: the connect answered {} where EINPROGRESS was expected", r);
    let m = wait_mask(c, net::WRITABLE as i32, deadline_in(3000));
    assert!(m == net::WRITABLE as i32, "net-bind-any: the client's mask at establishment reads {}", m);
    let m = wait_mask(l, net::READABLE as i32, deadline_in(3000));
    assert!(m == net::READABLE as i32, "net-bind-any: the listener's mask reads {}", m);
    let mut peer = none_addr();
    let s = api::rhu_net_accept(l, &mut peer);
    assert!(s >= 0 && ip_of(&peer) == LEASE && peer.port == p2 && port_of(l) == p1, "net-bind-any: the accept answered {} with the peer's port {} (the client's given port {}) and the listener's port {} (given {})", s, peer.port, p2, port_of(l), p1);
    let x = [0x5Au8];
    assert!(api::rhu_net_send(c, x.as_ptr(), 1, core::ptr::null()) == 1, "net-bind-any: the byte's send refused");
    let m = wait_mask(s, net::READABLE as i32, deadline_in(3000));
    let mut got = [0u8; 4];
    assert!(m & net::READABLE as i32 != 0 && api::rhu_net_recv(s, got.as_mut_ptr(), 4, 0, core::ptr::null_mut()) == 1 && got[0] == 0x5A, "net-bind-any: the byte did not cross (the server's mask {})", m);
    // Two datagram sockets, the pool of record.
    let a = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
    let b = api::rhu_net_socket(net::FAMILY_IPV4, net::TYPE_DATAGRAM, 0);
    assert!(a >= 0 && b >= 0, "net-bind-any: the datagram sockets read {} and {}", a, b);
    assert!(api::rhu_net_bind(a, &addr([0, 0, 0, 0], 0)) == 0 && api::rhu_net_bind(b, &addr(LEASE, 0)) == 0, "net-bind-any: a datagram socket's bind to port 0 refused");
    let (pa, pb) = (port_of(a), port_of(b));
    assert!((49152..=65535).contains(&pa) && (49152..=65535).contains(&pb) && pa != pb, "net-bind-any: the datagram sockets' ports read {} and {}", pa, pb);
    assert!(api::rhu_net_bind(a, &addr(LEASE, 0)) == sched::ERR_INVAL && port_of(a) == pa, "net-bind-any: a second bind of the bound datagram socket not refused EINVAL, or its port moved");
    assert!(api::rhu_net_readiness(a, 0) == net::WRITABLE as i32 || wait_mask(a, net::WRITABLE as i32, deadline_in(1000)) == net::WRITABLE as i32, "net-bind-any: a bound datagram socket not writable");
    let msg = [7u8; 8];
    assert!(api::rhu_net_send(a, msg.as_ptr(), 8, &addr(LEASE, pb as u16)) == 8, "net-bind-any: the datagram's send refused");
    let m = wait_mask(b, net::READABLE as i32, deadline_in(3000));
    let mut from = none_addr();
    let mut buf = [0u8; 16];
    assert!(m & net::READABLE as i32 != 0 && api::rhu_net_recv(b, buf.as_mut_ptr(), 16, 0, &mut from) == 8 && buf[..8] == msg && ip_of(&from) == LEASE && from.port == pa, "net-bind-any: the datagram with the sender's given port did not arrive (the receiver's mask {}, the sender's port read {} where {} was given)", m, from.port, pa);
    for h in [s, c, l, a, b] {
        assert!(api::rhu_net_close(h) == 0, "net-bind-any: a close refused");
    }
    let settled = wait_until(api::rhu_net_events_word(), deadline_in(3000), || if net::probe_streams_used() == 0 { Some(()) } else { None });
    assert!(settled.is_some(), "net-bind-any: {} stream sockets still tied 3 s after the closes", net::probe_streams_used());
    kprintln!("[api] PASS net-bind-any (a stream bound to port 0 given a port from 49152 up, its name reading it back and a second bind EINVAL; a foreign address with port 0 EINVAL with no port spent, the client's bind to port 0 given the very next port; the listener on its given port accepting the client from the client's given port, a byte across; two datagram sockets bound to port 0 given two ports from 49152 up, a second bind EINVAL, a datagram across carrying the sender's given port; the five closes and the stream pool whole)");
}
