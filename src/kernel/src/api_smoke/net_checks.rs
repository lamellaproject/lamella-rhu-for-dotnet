//! The network doors' eight checks of the api flavor, on the
//! emulator rows over the virtual medium and its scripted peer: the pump's
//! statistics, the medium and the lease, the retry scenario, the resolver,
//! a stream over the loopback, datagrams, the refusals, the event port's
//! properties. Every marker is constant text the api gate pins; the rows'
//! own numbers (the passes, the milliseconds) are on lines the gate does
//! not pin. Every wait is the recipe the header states -- the door, the
//! snapshot, the door again, the futex wait with a deadline -- because this
//! thread is more urgent than the network thread and a spin would starve it.

use core::ptr::{addr_of, read_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::net_peer::{LEASE_DNS2 as DNS2, LEASE_IP as LEASE, LEASE_MASK as MASK, NAMES, OFF_SEGMENT_IP as OFF_SEGMENT, OUR_MAC, PEER_IP as PEER, REFUSED_PORT, UNKNOWN_NAME, UNOWNED_IP as UNOWNED};
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
/// echo through the peek and the available door, the half-close, the closes.
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
/// address record round-tripped, the peek and the available door, an
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
    assert!(api::rhu_net_bind(hs[2], &addr(PEER, 7002)) == sched::ERR_INVAL && api::rhu_net_bind(hs[2], &addr(LEASE, 0)) == sched::ERR_INVAL, "net-refuse: a foreign address or the port 0 not refused EINVAL");
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
    kprintln!("[api] PASS net-refuse (IPv6 EAFNOSUPPORT, a bad type and protocol EPROTONOSUPPORT, the fifth stream EMFILE, a second bind EADDRINUSE, a bound, foreign or zero-port bind EINVAL, the unconnected stream ENOTCONN, a listen or accept on a datagram socket EINVAL, unsupported options ENOTSUP with the read-back form proven, bad words and three bad handles EINVAL at every door, a null and an IPv6 peer refused, the peer's reset ECONNREFUSED read once, the unowned address EHOSTUNREACH at the connect budget, a static address without a gateway with the off-segment connect ENETUNREACH and a bad mask EINVAL, the DHCP client armed again with its lease)");
}

static WOKEN: AtomicU32 = AtomicU32::new(0);
static SNAP: AtomicU32 = AtomicU32::new(0);

extern "C" fn port_waiter(_: usize) {
    let w = api::rhu_net_events_word();
    let r = api::rhu_futex_wait(w, SNAP.load(Ordering::Acquire), sched::now_ns() + 2_000_000_000);
    WOKEN.store(if r == 0 { 1 } else { 2 }, Ordering::Release);
}

/// The event port's properties: the bump door moves the global word by one
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
    // The bump door: the word by one, a parked waiter woken.
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
