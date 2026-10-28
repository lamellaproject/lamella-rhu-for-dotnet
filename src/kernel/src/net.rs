//! The network doors' bodies: a program's sockets over the
//! kernel's own TCP/IP stack (smoltcp 0.14.0, vendored under
//! src/vendor/crates) on the medium the board has -- the twenty-six verbs
//! `api.rs` exports and `include/rhu.h` states, the `rhu_net_` family with
//! the Linux socket verbs. The address
//! record is sized for both families; the resolver answers address records
//! with the slot's lifetime stated; the option door reads back its form.
//!
//! THE TENANCY is one futex word for the whole network state, taken by
//! compare-and-swap and slept on when held, NEVER the kernel critical
//! section: a stack poll runs for milliseconds, and the section masks the
//! kernel band. THE MASKING CONSEQUENCE: this module adds NO system-wide masking, NO
//! new critical section and NO handler; the zero-latency band is never
//! masked by anything here; the only sections are the witness counters'
//! single-word updates, the few instructions every door family takes.

#![cfg_attr(not(rhu_net_rows), allow(dead_code))] // a row without a medium compiles the records and the refusing half alone

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::AtomicU32;

use crate::critical::with_cs;
use crate::sched;

/// The codes this family adds to the hosting API's set (`include/rhu.h`;
/// errno-mirroring by convention).
pub const ERR_MFILE: i32 = -24;
pub const ERR_PIPE: i32 = -32;
pub const ERR_MSGSIZE: i32 = -90;
pub const ERR_PROTONOSUPPORT: i32 = -93;
pub const ERR_NOTSUP: i32 = -95;
pub const ERR_AFNOSUPPORT: i32 = -97;
pub const ERR_ADDRINUSE: i32 = -98;
pub const ERR_NETDOWN: i32 = -100;
pub const ERR_NETUNREACH: i32 = -101;
#[allow(dead_code)] // the header's set; no path answers it (an abort is the close door's own act)
pub const ERR_CONNABORTED: i32 = -103;
pub const ERR_CONNRESET: i32 = -104;
pub const ERR_ISCONN: i32 = -106;
pub const ERR_NOTCONN: i32 = -107;
pub const ERR_CONNREFUSED: i32 = -111;
pub const ERR_HOSTUNREACH: i32 = -113;
pub const ERR_ALREADY: i32 = -114;
pub const ERR_INPROGRESS: i32 = -115;

/// The address families, socket types, medium kinds, readiness bits, the
/// receive flag, the shutdown words and the options the header states.
pub const FAMILY_NONE: u32 = 0;
pub const FAMILY_IPV4: u32 = 2;
pub const FAMILY_IPV6: u32 = 10;
pub const TYPE_STREAM: u32 = 1;
pub const TYPE_DATAGRAM: u32 = 2;
#[allow(dead_code)]
pub const KIND_NONE: u32 = 0;
#[allow(dead_code)]
pub const KIND_WIRELESS: u32 = 1;
#[allow(dead_code)]
pub const KIND_WIRED: u32 = 2;
pub const KIND_VIRTUAL: u32 = 3;
pub const SOURCE_NONE: u32 = 0;
pub const SOURCE_DHCP: u32 = 1;
pub const SOURCE_STATIC: u32 = 2;
pub const READABLE: u32 = 1;
pub const WRITABLE: u32 = 2;
pub const ERROR: u32 = 4;
pub const HANGUP: u32 = 8;
pub const RECV_PEEK: u32 = 1;
pub const SHUT_RD: u32 = 0;
pub const SHUT_WR: u32 = 1;
pub const SHUT_RDWR: u32 = 2;
pub const OPT_NODELAY: u32 = 1;
pub const OPT_KEEPALIVE: u32 = 2;
pub const OPT_REUSEADDR: u32 = 3;

/// The records the header states (`rhu_net_addr`, `rhu_net_medium_desc`,
/// `rhu_net_lease_desc`, `rhu_net_limits_desc`), byte for byte.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Addr {
    /// 2 = IPv4 (the Linux value); 0 = no address (an unconnected peer half).
    pub family: u32,
    /// The port in the host's order.
    pub port: u32,
    /// The address bytes in the wire's order, IPv4 in the first four.
    pub addr: [u8; 16],
    /// The scope word (IPv6's; 0).
    pub scope: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MediumInfo {
    pub kind: u32,
    pub link: u32,
    pub hw: [u8; 6],
    pub reserved: u16,
}

/// Every address as its four octets in the wire's order read as one word
/// (the first octet in the word's lowest byte on this little-endian machine,
/// as `sin_addr.s_addr` holds it).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Lease {
    pub ip4: u32,
    pub mask: u32,
    pub gateway: u32,
    pub dns: [u32; 2],
    pub source: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Limits {
    pub streams: u32,
    pub datagrams: u32,
    pub stream_rx: u32,
    pub stream_tx: u32,
    pub datagram_buf: u32,
    pub resolve_slots: u32,
}

/// The witness, by symbol. Cumulative since boot.
#[repr(C)]
pub struct NetWitness {
    pub medium_kind: u32,
    pub link: u32,
    pub hw: [u8; 8],
    pub lease: Lease,
    pub lease_changes: u32,
    pub lease_at_ms: u32,
    pub sockets_opened: u32,
    pub sockets_refused: u32,
    pub connects: u32,
    pub accepts: u32,
    pub bytes_in: u32,
    pub bytes_out: u32,
    pub resolves_made: u32,
    pub resolves_failed: u32,
    pub refused: u32,
    pub last_refused_handle: i32,
    pub last_refused_code: i32,
    pub mask_changes: u32,
    pub global_bumps: u32,
    pub pump_passes: u32,
    pub pump_wakes_word: u32,
    pub pump_wakes_deadline: u32,
    pub pump_wakes_again: u32,
    pub pump_max_pass_ms: u32,
    pub pump_passes_over_10: u32,
    pub pump_passes_over_50: u32,
    pub pump_max_late_ms: u32,
    pub pump_late_over_10: u32,
    pub pump_late_over_50: u32,
}

#[unsafe(export_name = "__rhu_net_witness")]
pub static mut WITNESS: NetWitness = NetWitness {
    medium_kind: 0,
    link: 0,
    hw: [0; 8],
    lease: Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: 0 },
    lease_changes: 0,
    lease_at_ms: 0,
    sockets_opened: 0,
    sockets_refused: 0,
    connects: 0,
    accepts: 0,
    bytes_in: 0,
    bytes_out: 0,
    resolves_made: 0,
    resolves_failed: 0,
    refused: 0,
    last_refused_handle: 0,
    last_refused_code: 0,
    mask_changes: 0,
    global_bumps: 0,
    pump_passes: 0,
    pump_wakes_word: 0,
    pump_wakes_deadline: 0,
    pump_wakes_again: 0,
    pump_max_pass_ms: 0,
    pump_passes_over_10: 0,
    pump_passes_over_50: 0,
    pump_max_late_ms: 0,
    pump_late_over_10: 0,
    pump_late_over_50: 0,
};

fn witness() -> *mut NetWitness {
    addr_of_mut!(WITNESS)
}

unsafe fn bump(field: *mut u32, by: u32) {
    unsafe { write_volatile(field, read_volatile(field).wrapping_add(by)) }
}

unsafe fn max_into(field: *mut u32, v: u32) {
    unsafe {
        if read_volatile(field) < v {
            write_volatile(field, v)
        }
    }
}

/// A refusal recorded: the witness's count, the handle (or the medium
/// index) and the code it was asked with.
fn refused(handle: i32, code: i32) -> i32 {
    with_cs(|| unsafe {
        let w = witness();
        bump(addr_of_mut!((*w).refused), 1);
        write_volatile(addr_of_mut!((*w).last_refused_handle), handle);
        write_volatile(addr_of_mut!((*w).last_refused_code), code);
    });
    code
}

fn aligned<T>(p: *const T) -> bool {
    !p.is_null() && (p as usize) & 3 == 0
}

/// The one word for all sockets (the event port's word) and the medium's
/// word, readable on every row (the storage exists where no medium does:
/// a hosting layer's recipe may snapshot them before the count door).
static GLOBAL: AtomicU32 = AtomicU32::new(0);
static MEDIUM_WORD: AtomicU32 = AtomicU32::new(0);
/// The word the network thread sleeps on: a door that queued work bumps
/// it; a medium's interrupt handler bumps it (the virtual medium has none).
pub static WAKE: AtomicU32 = AtomicU32::new(0);

pub fn events_word() -> *const u32 {
    GLOBAL.as_ptr()
}

/// The shim's self-wake: the global word moved and its waiters woken (a
/// registration change, a port's close) -- the one bump that is not a mask
/// change.
pub fn events_bump() {
    GLOBAL.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
    with_cs(|| unsafe { bump(addr_of_mut!((*witness()).global_bumps), 1) });
    sched::futex_wake(GLOBAL.as_ptr(), u32::MAX);
}

// ---------------- the rows with a medium: the doors over the stack ----------------

#[cfg(rhu_net_rows)]
mod served {
    use super::*;
    use core::sync::atomic::Ordering;

    use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage};
    use smoltcp::socket::{dhcpv4, dns, tcp, udp};
    use smoltcp::time::{Duration, Instant};
    use smoltcp::wire::{DnsQueryType, EthernetAddress, HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Address};

    use crate::board::facts;
    use crate::board::net_medium::{Adapter, Link, Medium};
    use crate::board::virt_net::{VirtNet, VirtWitness};
    use crate::net_peer;

    /// The row's pool (the emulator rows'
    /// row): the stream sockets and their buffer pair, the datagram sockets
    /// and theirs, the resolver's slots, the handle table. The doctored
    /// budget (the api gate's control) makes a stream buffer a megabyte, so
    /// the pool exceeds the row's window and the assertion below refuses the build.
    pub const STREAMS: usize = 4;
    pub const DATAGRAMS: usize = 2;
    #[cfg(not(feature = "test-net-doctored-budget"))]
    pub const STREAM_RX: usize = 2048;
    #[cfg(feature = "test-net-doctored-budget")]
    pub const STREAM_RX: usize = 1 << 20;
    pub const STREAM_TX: usize = 2048;
    pub const DGRAM_BUF: usize = 1536;
    pub const DGRAM_PACKETS: usize = 4;
    pub const RESOLVE_SLOTS: usize = 4;
    pub const HANDLES: usize = 8;
    const SOCKETS: usize = STREAMS + DATAGRAMS + 2; // the DHCP client and the resolver
    /// The connect budget: a stream still in its handshake after it is
    /// aborted with RHU_EHOSTUNREACH pending (the peer's hardware address or
    /// its answer never came); the stack's own SYN retries run inside it.
    pub const CONNECT_BUDGET_MS: u32 = 3000;
    /// The safety deadline: the network thread never sleeps longer (a
    /// lost-wake guard).
    pub const SAFETY_MS: u32 = 500;
    /// The keep-alive interval the option sets (a row fact).
    pub const KEEPALIVE_MS: u64 = 7500;
    /// A resolution the stack gave up on at its own retry budget (10 s) is
    /// a timeout; one refused earlier is the server's name error.
    const RESOLVE_TIMEOUT_MS: u32 = 9000;
    /// The network thread's priority (one notch more urgent than the hosted
    /// flat level, `api::HOSTED_PRIO_FLAT` 0x80) and its stack.
    pub const THREAD_PRIO: u8 = 0x7F;
    pub const THREAD_STACK: usize = 8192;
    const EPHEMERAL_FIRST: u16 = 49152;
    const NONE: usize = usize::MAX;
    pub const KIND: u32 = KIND_VIRTUAL;
    pub const KIND_WORD: &str = "virtual";

    #[repr(C)]
    struct Buffers {
        srx: [[u8; STREAM_RX]; STREAMS],
        stx: [[u8; STREAM_TX]; STREAMS],
        drx: [[u8; DGRAM_BUF]; DATAGRAMS],
        dtx: [[u8; DGRAM_BUF]; DATAGRAMS],
    }
    static mut BUFFERS: Buffers = Buffers { srx: [[0; STREAM_RX]; STREAMS], stx: [[0; STREAM_TX]; STREAMS], drx: [[0; DGRAM_BUF]; DATAGRAMS], dtx: [[0; DGRAM_BUF]; DATAGRAMS] };
    static mut DRX_META: [[udp::PacketMetadata; DGRAM_PACKETS]; DATAGRAMS] = [[udp::PacketMetadata::EMPTY; DGRAM_PACKETS]; DATAGRAMS];
    static mut DTX_META: [[udp::PacketMetadata; DGRAM_PACKETS]; DATAGRAMS] = [[udp::PacketMetadata::EMPTY; DGRAM_PACKETS]; DATAGRAMS];
    static mut STORAGE: [SocketStorage<'static>; SOCKETS] = [const { SocketStorage::EMPTY }; SOCKETS];
    static mut QUERIES: [Option<dns::DnsQuery>; RESOLVE_SLOTS] = [const { None }; RESOLVE_SLOTS];

    /// The pool's bytes: the statics above, the medium's rings and the
    /// stack's state, as the banner prints them.
    pub const POOL_BYTES: usize = core::mem::size_of::<Buffers>()
        + 2 * core::mem::size_of::<[[udp::PacketMetadata; DGRAM_PACKETS]; DATAGRAMS]>()
        + core::mem::size_of::<[SocketStorage<'static>; SOCKETS]>()
        + core::mem::size_of::<[Option<dns::DnsQuery>; RESOLVE_SLOTS]>()
        + core::mem::size_of::<Stack>();
    const _: () = assert!(POOL_BYTES <= facts::RAM_RANGES[0].bytes / 4, "the network pool exceeds a quarter of the row's RAM window");

    const FREE: u8 = 0;
    const STREAM: u8 = 1;
    const LISTENER: u8 = 2;
    const DATAGRAM: u8 = 3;

    #[derive(Clone, Copy)]
    struct Slot {
        kind: u8,
        /// The index into the stream or datagram socket arrays.
        sock: usize,
        connecting: bool,
        was_connected: bool,
        failed: bool,
        closing: bool,
        rd_shut: bool,
        reuse: bool,
        /// The last judged state was one a close ends in (TimeWait, LastAck
        /// or Closed), so a later Closed is no reset.
        last_ending: bool,
        started_ms: u32,
        pending: i32,
        mask: u32,
        risen: u32,
        local_port: u16,
        bound: bool,
        backlog: [usize; STREAMS],
        backlog_n: usize,
        peer: Option<(Ipv4Address, u16)>,
    }

    const EMPTY_SLOT: Slot = Slot {
        kind: FREE,
        sock: NONE,
        connecting: false,
        was_connected: false,
        failed: false,
        closing: false,
        rd_shut: false,
        reuse: false,
        last_ending: false,
        started_ms: 0,
        pending: 0,
        mask: 0,
        risen: 0,
        local_port: 0,
        bound: false,
        backlog: [NONE; STREAMS],
        backlog_n: 0,
        peer: None,
    };

    #[derive(Clone, Copy)]
    struct Resolve {
        used: bool,
        done: bool,
        qh: Option<dns::QueryHandle>,
        code: i32,
        n: u32,
        addrs: [[u8; 4]; 2],
        started_ms: u32,
    }

    const EMPTY_RESOLVE: Resolve = Resolve { used: false, done: false, qh: None, code: 0, n: 0, addrs: [[0; 4]; 2], started_ms: 0 };

    struct Stack {
        iface: Interface,
        sockets: SocketSet<'static>,
        medium: VirtNet,
        tcp: [SocketHandle; STREAMS],
        tcp_used: [bool; STREAMS],
        udp: [SocketHandle; DATAGRAMS],
        udp_used: [bool; DATAGRAMS],
        dhcp: Option<SocketHandle>,
        dns: SocketHandle,
        lease: Lease,
        our_ip: Option<Ipv4Address>,
        prefix: u8,
        gateway: Option<Ipv4Address>,
        dns_servers: [Option<Ipv4Address>; 2],
        slots: [Slot; HANDLES],
        resolve: [Resolve; RESOLVE_SLOTS],
        ephemeral: u16,
    }

    static mut STACK: Option<Stack> = None;
    /// The medium's wake word as the seam names it (the virtual medium's is
    /// this module's own `WAKE`; a wired medium's handler bumps its own),
    /// cached at init for the thread's sleep and the doors' kick.
    static WAKE_WORD: core::sync::atomic::AtomicPtr<AtomicU32> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
    /// The tenancy word (0 free, 1 held), a futex word.
    static TENANCY: AtomicU32 = AtomicU32::new(0);
    /// The api flavor's diagnostic of the tenancy: its holder (1 the network
    /// thread, 2 a door) and the network thread's phase (0 asleep, 1 the
    /// medium's service, 2 the stack's poll, 3 the judge), so a wait that
    /// never ends names them instead of hanging silently.
    #[cfg(feature = "api-smoke")]
    static DIAG_HOLDER: AtomicU32 = AtomicU32::new(0);
    #[cfg(feature = "api-smoke")]
    static DIAG_PHASE: AtomicU32 = AtomicU32::new(0);
    static EVENT: [AtomicU32; HANDLES] = [const { AtomicU32::new(0) }; HANDLES];
    static RESOLVE_WORD: [AtomicU32; RESOLVE_SLOTS] = [const { AtomicU32::new(0) }; RESOLVE_SLOTS];

    fn take() {
        loop {
            if TENANCY.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                return;
            }
            #[cfg(not(feature = "api-smoke"))]
            let _ = sched::futex_wait(TENANCY.as_ptr(), 1, sched::DEADLINE_NONE);
            // The api flavor's diagnostic: ten seconds without the tenancy is
            // a hang, named here with the holder and the thread's phase.
            #[cfg(feature = "api-smoke")]
            if sched::futex_wait(TENANCY.as_ptr(), 1, sched::now_ns() + 10_000_000_000) == sched::ERR_TIMEDOUT && TENANCY.load(Ordering::Acquire) != 0 {
                let (passes, longest) = unsafe { (read_volatile(core::ptr::addr_of!((*witness()).pump_passes)), read_volatile(core::ptr::addr_of!((*witness()).pump_max_pass_ms))) };
                panic!(
                    "net: the tenancy was held for 10 s -- the holder {} (1 the network thread, 2 a door), the network thread's phase {} (0 asleep, 1 the medium's service, 2 the stack's poll, 3 the judge), {} passes, the longest {} ms",
                    DIAG_HOLDER.load(Ordering::Relaxed),
                    DIAG_PHASE.load(Ordering::Relaxed),
                    passes,
                    longest
                );
            }
        }
    }

    fn give() {
        #[cfg(feature = "api-smoke")]
        DIAG_HOLDER.store(0, Ordering::Relaxed);
        TENANCY.store(0, Ordering::Release);
        sched::futex_wake(TENANCY.as_ptr(), 1);
    }

    fn stack() -> &'static mut Stack {
        unsafe { (*addr_of_mut!(STACK)).as_mut().expect("the network stack before its init") }
    }

    /// A door's body under the tenancy.
    fn with_stack<R>(f: impl FnOnce(&mut Stack) -> R) -> R {
        take();
        #[cfg(feature = "api-smoke")]
        DIAG_HOLDER.store(2, Ordering::Relaxed);
        let r = f(stack());
        give();
        r
    }

    fn wake_word() -> &'static AtomicU32 {
        let p = WAKE_WORD.load(Ordering::Acquire);
        if p.is_null() { &WAKE } else { unsafe { &*p } }
    }

    /// The network thread asked to poll now (a door queued work): the
    /// medium's wake word bumped and the thread woken.
    fn kick() {
        let w = wake_word();
        w.fetch_add(1, Ordering::AcqRel);
        sched::futex_wake(w.as_ptr(), 1);
    }

    /// The stack's clock: the scheduler's 64-bit nanosecond clock in
    /// milliseconds, monotonic across the tick counter's wrap.
    fn now_instant() -> Instant {
        Instant::from_millis((sched::now_ns() / 1_000_000) as i64)
    }

    fn word(o: [u8; 4]) -> u32 {
        u32::from_ne_bytes(o)
    }

    fn octets(w: u32) -> [u8; 4] {
        w.to_ne_bytes()
    }

    fn mask_of(prefix: u8) -> u32 {
        if prefix == 0 { 0 } else { word((u32::MAX << (32 - prefix as u32)).to_be_bytes()) }
    }

    fn write_addr(p: *mut Addr, ip: Option<(Ipv4Address, u16)>) {
        let mut a = Addr { family: FAMILY_NONE, port: 0, addr: [0; 16], scope: 0 };
        if let Some((ip, port)) = ip {
            a.family = FAMILY_IPV4;
            a.port = port as u32;
            a.addr[..4].copy_from_slice(&ip.octets());
        }
        unsafe { p.write(a) }
    }

    /// An address record read: IPv4 with a port that fits, else the code.
    fn read_addr(p: *const Addr) -> Result<(Ipv4Address, u16), i32> {
        if !aligned(p) {
            return Err(sched::ERR_INVAL);
        }
        let a = unsafe { p.read() };
        if a.family == FAMILY_IPV6 {
            return Err(ERR_AFNOSUPPORT);
        }
        if a.family != FAMILY_IPV4 || a.port > 0xFFFF {
            return Err(if a.family == FAMILY_IPV4 { sched::ERR_INVAL } else { ERR_AFNOSUPPORT });
        }
        Ok((Ipv4Address::new(a.addr[0], a.addr[1], a.addr[2], a.addr[3]), a.port as u16))
    }

    // ---------------- init and the thread ----------------

    /// The stack built over the row's medium: the interface at the medium's
    /// hardware address, seeded from the entropy door; the socket set with
    /// the stream and datagram sockets over the pool's buffers, the resolver
    /// over its slots and the DHCP client armed (the discover retried after
    /// one second, not the stack's ten: a switch port forwards a board's
    /// frames seconds after link-up); the medium's word moved for the link.
    pub fn init() {
        let mut seed = [0u8; 8];
        crate::entropy::fill(&mut seed);
        let mut cfg = Config::new(HardwareAddress::Ethernet(EthernetAddress(net_peer::OUR_MAC)));
        cfg.random_seed = u64::from_le_bytes(seed);
        let mut medium = VirtNet::new(&WAKE, cfg!(feature = "test-net-doctored-arp"));
        let iface = {
            let mut dev = Adapter { medium: &mut medium };
            Interface::new(cfg, &mut dev, Instant::from_millis(0))
        };
        let storage: &'static mut [SocketStorage<'static>; SOCKETS] = unsafe { &mut *addr_of_mut!(STORAGE) };
        let mut sockets = SocketSet::new(&mut storage[..]);
        // Each buffer a borrow of its own static element for the image's life
        // (a raw pointer per element: the pool is handed out once, here).
        let b = addr_of_mut!(BUFFERS);
        let mut tcp_h = [SocketHandle::default(); STREAMS];
        for (i, h) in tcp_h.iter_mut().enumerate() {
            let rx: &'static mut [u8; STREAM_RX] = unsafe { &mut *addr_of_mut!((*b).srx[i]) };
            let tx: &'static mut [u8; STREAM_TX] = unsafe { &mut *addr_of_mut!((*b).stx[i]) };
            *h = sockets.add(tcp::Socket::new(tcp::SocketBuffer::new(&mut rx[..]), tcp::SocketBuffer::new(&mut tx[..])));
        }
        let mut udp_h = [SocketHandle::default(); DATAGRAMS];
        for (i, h) in udp_h.iter_mut().enumerate() {
            let rm: &'static mut [udp::PacketMetadata; DGRAM_PACKETS] = unsafe { &mut *addr_of_mut!(DRX_META[i]) };
            let rb: &'static mut [u8; DGRAM_BUF] = unsafe { &mut *addr_of_mut!((*b).drx[i]) };
            let tm: &'static mut [udp::PacketMetadata; DGRAM_PACKETS] = unsafe { &mut *addr_of_mut!(DTX_META[i]) };
            let tb: &'static mut [u8; DGRAM_BUF] = unsafe { &mut *addr_of_mut!((*b).dtx[i]) };
            *h = sockets.add(udp::Socket::new(udp::PacketBuffer::new(&mut rm[..], &mut rb[..]), udp::PacketBuffer::new(&mut tm[..], &mut tb[..])));
        }
        let queries: &'static mut [Option<dns::DnsQuery>; RESOLVE_SLOTS] = unsafe { &mut *addr_of_mut!(QUERIES) };
        let dns_h = sockets.add(dns::Socket::new(&[], &mut queries[..]));
        let dhcp_h = sockets.add(new_dhcp());
        let link = medium.link();
        let hw = medium.hardware_address().unwrap_or([0; 6]);
        WAKE_WORD.store(medium.wake_word() as *const AtomicU32 as *mut AtomicU32, Ordering::Release);
        unsafe {
            *addr_of_mut!(STACK) = Some(Stack {
                iface,
                sockets,
                medium,
                tcp: tcp_h,
                tcp_used: [false; STREAMS],
                udp: udp_h,
                udp_used: [false; DATAGRAMS],
                dhcp: Some(dhcp_h),
                dns: dns_h,
                lease: Lease { ip4: 0, mask: 0, gateway: 0, dns: [0; 2], source: SOURCE_NONE },
                our_ip: None,
                prefix: 0,
                gateway: None,
                dns_servers: [None; 2],
                slots: [EMPTY_SLOT; HANDLES],
                resolve: [EMPTY_RESOLVE; RESOLVE_SLOTS],
                ephemeral: EPHEMERAL_FIRST,
            });
        }
        with_cs(|| unsafe {
            let w = witness();
            write_volatile(addr_of_mut!((*w).medium_kind), KIND);
            write_volatile(addr_of_mut!((*w).link), (link == Link::Up) as u32);
            let mut hw8 = [0u8; 8];
            hw8[..6].copy_from_slice(&hw);
            write_volatile(addr_of_mut!((*w).hw), hw8);
        });
        MEDIUM_WORD.fetch_add(1, Ordering::AcqRel);
    }

    fn new_dhcp() -> dhcpv4::Socket<'static> {
        let mut s = dhcpv4::Socket::new();
        let mut rc = dhcpv4::RetryConfig::default();
        rc.discover_timeout = Duration::from_secs(1);
        s.set_retry_config(rc);
        s
    }

    /// The network thread spawned (after the scheduler's init).
    pub fn start() {
        sched::spawn("net", pump, 0, THREAD_PRIO, THREAD_STACK);
    }

    extern "C" fn pump(_: usize) {
        let wake = wake_word();
        loop {
            let snap = wake.load(Ordering::Acquire);
            take();
            #[cfg(feature = "api-smoke")]
            DIAG_HOLDER.store(1, Ordering::Relaxed);
            let t0 = sched::now_ms();
            let (changed, delay_ms, resolved) = pass(stack(), t0);
            let pass_ms = sched::now_ms().wrapping_sub(t0);
            give();
            // The wakes, outside the tenancy: the socket words were bumped
            // inside it; the global word moves here.
            for h in 0..HANDLES {
                if changed & (1 << h) != 0 {
                    sched::futex_wake(EVENT[h].as_ptr(), u32::MAX);
                }
            }
            if changed != 0 {
                GLOBAL.fetch_add(1, Ordering::AcqRel);
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).global_bumps), 1) });
                sched::futex_wake(GLOBAL.as_ptr(), u32::MAX);
            }
            #[cfg(feature = "test-net-doctored-bump")]
            {
                // The doctored thread: the global word moved every pass
                // without a mask change (the api gate's control; the port
                // check counts the spurious wake and refuses).
                GLOBAL.fetch_add(1, Ordering::AcqRel);
                sched::futex_wake(GLOBAL.as_ptr(), u32::MAX);
            }
            for i in 0..RESOLVE_SLOTS {
                if resolved & (1 << i) != 0 {
                    sched::futex_wake(RESOLVE_WORD[i].as_ptr(), u32::MAX);
                }
            }
            #[cfg(feature = "test-net-doctored-slow")]
            {
                // The doctored slow pump: half a second of busy work per
                // pass (the api gate's control; the pump check's statistics
                // must move).
                let t = sched::now_ms();
                while sched::now_ms().wrapping_sub(t) < 500 {
                    core::hint::spin_loop();
                }
            }
            let total_ms = sched::now_ms().wrapping_sub(t0).max(pass_ms);
            unsafe {
                let w = witness();
                bump(addr_of_mut!((*w).pump_passes), 1);
                max_into(addr_of_mut!((*w).pump_max_pass_ms), total_ms);
                if total_ms > 10 {
                    bump(addr_of_mut!((*w).pump_passes_over_10), 1);
                }
                if total_ms > 50 {
                    bump(addr_of_mut!((*w).pump_passes_over_50), 1);
                }
            }
            let deadline_ms = t0.wrapping_add(delay_ms);
            let deadline_ns = sched::now_ns() + (delay_ms as u64).saturating_sub(sched::now_ms().wrapping_sub(t0) as u64) * 1_000_000;
            match sched::futex_wait(wake.as_ptr(), snap, deadline_ns) {
                sched::ERR_TIMEDOUT => {
                    let late = sched::now_ms().wrapping_sub(deadline_ms);
                    unsafe {
                        let w = witness();
                        bump(addr_of_mut!((*w).pump_wakes_deadline), 1);
                        if late < 0x8000_0000 {
                            max_into(addr_of_mut!((*w).pump_max_late_ms), late);
                            if late > 10 {
                                bump(addr_of_mut!((*w).pump_late_over_10), 1);
                            }
                            if late > 50 {
                                bump(addr_of_mut!((*w).pump_late_over_50), 1);
                            }
                        }
                    }
                }
                sched::ERR_AGAIN => unsafe { bump(addr_of_mut!((*witness()).pump_wakes_again), 1) },
                _ => unsafe { bump(addr_of_mut!((*witness()).pump_wakes_word), 1) },
            }
        }
    }

    /// One pass under the tenancy: the medium serviced and the stack polled
    /// until nothing moves, the lease events, the resolver's answers, every
    /// handle's readiness. Answers the handles whose words moved, the delay
    /// to the next pass and the resolutions that completed.
    fn pass(st: &mut Stack, now_ms: u32) -> (u32, u32, u32) {
        let now = now_instant();
        for _ in 0..16 {
            #[cfg(feature = "api-smoke")]
            DIAG_PHASE.store(1, Ordering::Relaxed);
            st.medium.service(now_ms);
            #[cfg(feature = "api-smoke")]
            DIAG_PHASE.store(2, Ordering::Relaxed);
            let Stack { iface, sockets, medium, .. } = st;
            let mut dev = Adapter { medium };
            let _ = iface.poll(now, &mut dev, sockets);
            if !st.medium.has_staged() {
                break;
            }
        }
        lease_events(st, now_ms);
        let resolved = resolver_answers(st, now_ms);
        #[cfg(feature = "api-smoke")]
        DIAG_PHASE.store(3, Ordering::Relaxed);
        let changed = judge(st, now_ms);
        free_closed(st);
        #[cfg(feature = "api-smoke")]
        DIAG_PHASE.store(0, Ordering::Relaxed);
        let mut delay = SAFETY_MS;
        if let Some(d) = st.iface.poll_delay(now, &st.sockets) {
            delay = delay.min(d.total_millis().min(u32::MAX as u64) as u32);
        }
        for s in st.slots.iter() {
            if s.kind == STREAM && s.connecting {
                let left = CONNECT_BUDGET_MS.saturating_sub(now_ms.wrapping_sub(s.started_ms));
                delay = delay.min(left.max(1));
            }
        }
        (changed, delay.max(1), resolved)
    }

    /// The DHCP client's events: the lease recorded, the address, the route
    /// and the name servers set, the medium's word moved.
    fn lease_events(st: &mut Stack, now_ms: u32) {
        let Some(dh) = st.dhcp else { return };
        let event = {
            let d = st.sockets.get_mut::<dhcpv4::Socket>(dh);
            match d.poll() {
                None => None,
                Some(dhcpv4::Event::Deconfigured) => Some(None),
                Some(dhcpv4::Event::Configured(c)) => {
                    let mut dns = [None; 2];
                    for (i, s) in c.dns_servers.iter().take(2).enumerate() {
                        dns[i] = Some(*s);
                    }
                    Some(Some((c.address.address(), c.address.prefix_len(), c.router, dns)))
                }
            }
        };
        match event {
            None => {}
            Some(None) => apply_address(st, None, 0, None, [None; 2], SOURCE_NONE, now_ms),
            Some(Some((ip, prefix, gw, dns))) => apply_address(st, Some(ip), prefix, gw, dns, SOURCE_DHCP, now_ms),
        }
    }

    /// The interface's address, default route and name servers set from a
    /// lease or a static record (or cleared), the lease record and the
    /// witness updated, the medium's word moved.
    fn apply_address(st: &mut Stack, ip: Option<Ipv4Address>, prefix: u8, gw: Option<Ipv4Address>, dns: [Option<Ipv4Address>; 2], source: u32, now_ms: u32) {
        st.iface.update_ip_addrs(|a| {
            a.clear();
            if let Some(ip) = ip {
                let _ = a.push(IpCidr::new(IpAddress::Ipv4(ip), prefix));
            }
        });
        match gw {
            Some(g) => {
                let _ = st.iface.routes_mut().add_default_ipv4_route(g);
            }
            None => {
                st.iface.routes_mut().remove_default_ipv4_route();
            }
        }
        let mut servers: [IpAddress; 2] = [IpAddress::Ipv4(Ipv4Address::UNSPECIFIED); 2];
        let mut n = 0;
        for s in dns.iter().flatten() {
            servers[n] = IpAddress::Ipv4(*s);
            n += 1;
        }
        st.sockets.get_mut::<dns::Socket>(st.dns).update_servers(&servers[..n]);
        st.our_ip = ip;
        st.prefix = prefix;
        st.gateway = gw;
        st.dns_servers = dns;
        st.lease = Lease {
            ip4: ip.map(|a| word(a.octets())).unwrap_or(0),
            mask: if ip.is_some() { mask_of(prefix) } else { 0 },
            gateway: gw.map(|a| word(a.octets())).unwrap_or(0),
            dns: [dns[0].map(|a| word(a.octets())).unwrap_or(0), dns[1].map(|a| word(a.octets())).unwrap_or(0)],
            source,
        };
        let lease = st.lease;
        with_cs(|| unsafe {
            let w = witness();
            write_volatile(addr_of_mut!((*w).lease), lease);
            bump(addr_of_mut!((*w).lease_changes), 1);
            if source != SOURCE_NONE {
                write_volatile(addr_of_mut!((*w).lease_at_ms), now_ms);
            }
        });
        MEDIUM_WORD.fetch_add(1, Ordering::AcqRel);
        sched::futex_wake(MEDIUM_WORD.as_ptr(), u32::MAX);
    }

    /// The resolver's pending queries read: an answer or a failure stored
    /// in the kernel's slot, its word bumped (the wake after the tenancy).
    fn resolver_answers(st: &mut Stack, now_ms: u32) -> u32 {
        let mut done = 0u32;
        let d = st.sockets.get_mut::<dns::Socket>(st.dns);
        for (i, r) in st.resolve.iter_mut().enumerate() {
            if !r.used || r.done {
                continue;
            }
            let Some(qh) = r.qh else { continue };
            match d.get_query_result(qh) {
                Err(dns::GetQueryResultError::Pending) => {}
                Ok(addrs) => {
                    r.n = 0;
                    for a in addrs.iter().take(2) {
                        let IpAddress::Ipv4(v4) = a;
                        r.addrs[r.n as usize] = v4.octets();
                        r.n += 1;
                    }
                    r.code = if r.n == 0 { ERR_HOSTUNREACH } else { 0 };
                    r.done = true;
                    r.qh = None;
                }
                Err(dns::GetQueryResultError::Failed) => {
                    r.code = if now_ms.wrapping_sub(r.started_ms) >= RESOLVE_TIMEOUT_MS { sched::ERR_TIMEDOUT } else { ERR_HOSTUNREACH };
                    r.done = true;
                    r.qh = None;
                    with_cs(|| unsafe { bump(addr_of_mut!((*witness()).resolves_failed), 1) });
                }
            }
            if r.done {
                RESOLVE_WORD[i].fetch_add(1, Ordering::AcqRel);
                done |= 1 << i;
            }
        }
        done
    }

    /// The abort of a connection that never synchronized (SYN-SENT at the
    /// budget; a stream closed by the program while connecting or after a
    /// failed connect). The stack's abort alone keeps the tuple to send a
    /// reset -- to a host whose hardware address it may never learn, so it
    /// would ask for it once a second for ever, and the neighbor cache's
    /// one rate limit (the stack's, not per socket) then refuses every other
    /// socket's discovery: the resolver's query never left the row. A reset
    /// for a connection nobody acknowledged says
    /// nothing to anybody: the tuple is dropped through the stack's own
    /// reset (a listen on the socket's port, then the abort) and the socket
    /// ends closed with nothing pending.
    fn abort_unsynchronized(s: &mut tcp::Socket) {
        let port = s.local_endpoint().map(|e| e.port).unwrap_or(0);
        s.abort();
        if port != 0 {
            let _ = s.listen(port);
            s.abort();
        }
    }

    fn is_ending(s: tcp::State) -> bool {
        matches!(s, tcp::State::TimeWait | tcp::State::LastAck | tcp::State::Closed)
    }

    /// Every handle's readiness mask computed from the live socket state:
    /// the connect edge (established, refused with a reset, or aborted at
    /// the budget), the level bits, the hang-up on the peer's close, the
    /// error on a reset; a changed mask bumps the handle's word and records
    /// the risen bits. Answers the handles that changed.
    fn judge(st: &mut Stack, now_ms: u32) -> u32 {
        let mut changed = 0u32;
        let Stack { sockets, slots, tcp, udp, .. } = st;
        for (h, slot) in slots.iter_mut().enumerate() {
            let mut mask = 0u32;
            match slot.kind {
                STREAM => {
                    let s = sockets.get_mut::<tcp::Socket>(tcp[slot.sock]);
                    let state = s.state();
                    if slot.connecting {
                        match state {
                            tcp::State::Closed => {
                                slot.connecting = false;
                                slot.failed = true;
                                slot.pending = ERR_CONNREFUSED;
                            }
                            tcp::State::SynSent | tcp::State::SynReceived => {
                                if now_ms.wrapping_sub(slot.started_ms) >= CONNECT_BUDGET_MS {
                                    abort_unsynchronized(s);
                                    slot.connecting = false;
                                    slot.failed = true;
                                    slot.pending = ERR_HOSTUNREACH;
                                }
                            }
                            _ => {
                                slot.connecting = false;
                                slot.was_connected = true;
                                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).connects), 1) });
                            }
                        }
                    }
                    if slot.failed {
                        mask = WRITABLE | ERROR | HANGUP;
                    } else if slot.was_connected {
                        let peer_closed = !s.may_recv();
                        if state == tcp::State::Closed && !slot.last_ending && !slot.closing && slot.pending == 0 {
                            slot.pending = ERR_CONNRESET;
                        }
                        if s.can_recv() || peer_closed || slot.rd_shut {
                            mask |= READABLE;
                        }
                        if s.can_send() || slot.pending != 0 {
                            mask |= WRITABLE;
                        }
                        if peer_closed {
                            mask |= HANGUP;
                        }
                        if slot.pending != 0 {
                            mask |= ERROR;
                        }
                    }
                    slot.last_ending = is_ending(state);
                }
                LISTENER => {
                    for &i in slot.backlog[..slot.backlog_n].iter() {
                        let s = sockets.get::<tcp::Socket>(tcp[i]);
                        if matches!(s.state(), tcp::State::Established | tcp::State::CloseWait) {
                            mask |= READABLE;
                        }
                    }
                }
                DATAGRAM => {
                    let s = sockets.get::<udp::Socket>(udp[slot.sock]);
                    if s.can_recv() || slot.rd_shut {
                        mask |= READABLE;
                    }
                    if s.is_open() && s.can_send() {
                        mask |= WRITABLE;
                    }
                }
                _ => continue,
            }
            if mask != slot.mask {
                slot.risen |= mask & !slot.mask;
                slot.mask = mask;
                EVENT[h].fetch_add(1, Ordering::AcqRel);
                changed |= 1 << h;
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).mask_changes), 1) });
            }
        }
        changed
    }

    /// A stream socket whose handle was closed is returned to the pool once
    /// the stack removed its connection (its close handshake ran on this
    /// thread meanwhile; its buffer pair was tied until now).
    fn free_closed(st: &mut Stack) {
        let mut owned = [false; STREAMS];
        for s in st.slots.iter() {
            if s.kind == STREAM && s.sock != NONE {
                owned[s.sock] = true;
            }
            if s.kind == LISTENER {
                for &i in s.backlog[..s.backlog_n].iter() {
                    owned[i] = true;
                }
            }
        }
        for i in 0..STREAMS {
            if st.tcp_used[i] && !owned[i] && !st.sockets.get::<tcp::Socket>(st.tcp[i]).is_open() {
                st.tcp_used[i] = false;
            }
        }
    }

    // ---------------- the media and the address ----------------

    pub fn count() -> i32 {
        1
    }

    pub fn medium_info(medium: u32, out: *mut MediumInfo) -> i32 {
        if medium != 0 || !aligned(out) {
            return refused(medium as i32, sched::ERR_INVAL);
        }
        let (link, hw) = with_stack(|st| (st.medium.link(), st.medium.hardware_address().unwrap_or([0; 6])));
        unsafe { out.write(MediumInfo { kind: KIND, link: (link == Link::Up) as u32, hw, reserved: 0 }) };
        0
    }

    pub fn medium_word(medium: u32) -> *const u32 {
        if medium != 0 { core::ptr::null() } else { MEDIUM_WORD.as_ptr() }
    }

    /// NULL: the DHCP client armed (the default; a static address dropped);
    /// a record: the static address, mask, gateway and name servers set, the
    /// client stopped. The medium's word moves either way.
    pub fn configure(medium: u32, fixed: *const Lease) -> i32 {
        if medium != 0 {
            return refused(medium as i32, sched::ERR_INVAL);
        }
        if fixed.is_null() {
            with_stack(|st| {
                if st.dhcp.is_none() {
                    apply_address(st, None, 0, None, [None; 2], SOURCE_NONE, sched::now_ms());
                    st.dhcp = Some(st.sockets.add(new_dhcp()));
                }
            });
            kick();
            return 0;
        }
        if !aligned(fixed) {
            return refused(0, sched::ERR_INVAL);
        }
        let rec = unsafe { fixed.read() };
        let mask = u32::from_be_bytes(octets(rec.mask));
        let prefix = mask.leading_ones() as u8;
        if rec.ip4 == 0 || mask == 0 || mask != mask_of(prefix).swap_bytes().swap_bytes() && u32::from_be_bytes(octets(mask_of(prefix))) != mask {
            return refused(0, sched::ERR_INVAL);
        }
        with_stack(|st| {
            if let Some(dh) = st.dhcp.take() {
                st.sockets.remove(dh);
            }
            let o = octets(rec.ip4);
            let ip = Ipv4Address::new(o[0], o[1], o[2], o[3]);
            let gw = if rec.gateway == 0 { None } else { let g = octets(rec.gateway); Some(Ipv4Address::new(g[0], g[1], g[2], g[3])) };
            let dns = [
                if rec.dns[0] == 0 { None } else { let d = octets(rec.dns[0]); Some(Ipv4Address::new(d[0], d[1], d[2], d[3])) },
                if rec.dns[1] == 0 { None } else { let d = octets(rec.dns[1]); Some(Ipv4Address::new(d[0], d[1], d[2], d[3])) },
            ];
            apply_address(st, Some(ip), prefix, gw, dns, SOURCE_STATIC, sched::now_ms());
        });
        kick();
        0
    }

    pub fn lease(medium: u32, out: *mut Lease) -> i32 {
        if medium != 0 || !aligned(out) {
            return refused(medium as i32, sched::ERR_INVAL);
        }
        let l = with_stack(|st| st.lease);
        if l.source == SOURCE_NONE {
            return sched::ERR_AGAIN;
        }
        unsafe { out.write(l) };
        0
    }

    pub fn limits(out: *mut Limits) -> i32 {
        if !aligned(out) {
            return refused(0, sched::ERR_INVAL);
        }
        unsafe {
            out.write(Limits {
                streams: STREAMS as u32,
                datagrams: DATAGRAMS as u32,
                stream_rx: STREAM_RX as u32,
                stream_tx: STREAM_TX as u32,
                datagram_buf: DGRAM_BUF as u32,
                resolve_slots: RESOLVE_SLOTS as u32,
            })
        };
        0
    }

    // ---------------- name resolution ----------------

    /// A query filed through the stack's resolver into one of the kernel's
    /// slots; the handle answered at once. The slot lives until
    /// `resolve_result` answers anything but RHU_EAGAIN.
    pub fn resolve(name: *const u8, len: u32) -> i32 {
        if name.is_null() || len == 0 || len > 253 {
            return refused(-1, sched::ERR_INVAL);
        }
        let bytes = unsafe { core::slice::from_raw_parts(name, len as usize) };
        let Ok(s) = core::str::from_utf8(bytes) else {
            return refused(-1, sched::ERR_INVAL);
        };
        if !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.') {
            return refused(-1, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            if st.dns_servers.iter().all(|d| d.is_none()) {
                return Err(ERR_NETUNREACH);
            }
            let Some(i) = st.resolve.iter().position(|r| !r.used) else {
                return Err(sched::ERR_AGAIN);
            };
            let Stack { iface, sockets, dns, .. } = st;
            let d = sockets.get_mut::<dns::Socket>(*dns);
            match d.start_query(iface.context(), s, DnsQueryType::A) {
                Ok(qh) => {
                    st.resolve[i] = Resolve { used: true, done: false, qh: Some(qh), code: 0, n: 0, addrs: [[0; 4]; 2], started_ms: sched::now_ms() };
                    Ok(i as i32)
                }
                Err(dns::StartQueryError::NoFreeSlot) => Err(sched::ERR_AGAIN),
                Err(_) => Err(sched::ERR_INVAL),
            }
        });
        match r {
            Ok(h) => {
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).resolves_made), 1) });
                kick();
                h
            }
            Err(code) => refused(-1, code),
        }
    }

    /// The answer as address records (the port 0): the count written;
    /// RHU_EAGAIN while pending; RHU_EHOSTUNREACH for a name the server has
    /// not; RHU_ETIMEDOUT at the stack's budget. Any final answer releases the slot.
    pub fn resolve_result(h: i32, out: *mut Addr, max: u32) -> i32 {
        if h < 0 || h as usize >= RESOLVE_SLOTS || (max > 0 && !aligned(out)) {
            return refused(h, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            let r = st.resolve[h as usize];
            if !r.used {
                return Err(sched::ERR_INVAL);
            }
            if !r.done {
                return Err(sched::ERR_AGAIN);
            }
            st.resolve[h as usize] = EMPTY_RESOLVE;
            Ok(r)
        });
        match r {
            Err(code) if code == sched::ERR_AGAIN => code,
            Err(code) => refused(h, code),
            Ok(r) => {
                if r.code != 0 {
                    return r.code;
                }
                let n = (r.n as usize).min(max as usize);
                for i in 0..n {
                    let a = r.addrs[i];
                    write_addr(unsafe { out.add(i) }, Some((Ipv4Address::new(a[0], a[1], a[2], a[3]), 0)));
                }
                n as i32
            }
        }
    }

    pub fn resolve_word(h: i32) -> *const u32 {
        if h < 0 || h as usize >= RESOLVE_SLOTS {
            return core::ptr::null();
        }
        let used = with_stack(|st| st.resolve[h as usize].used);
        if used { RESOLVE_WORD[h as usize].as_ptr() } else { core::ptr::null() }
    }

    // ---------------- sockets ----------------

    fn free_stream(st: &Stack) -> Option<usize> {
        st.tcp_used.iter().position(|u| !u)
    }

    /// Whether a slot holds `port` for the protocol (a datagram socket, or a
    /// stream or listener).
    fn on_port(s: &Slot, port: u16, datagram: bool) -> bool {
        s.bound && s.local_port == port && if datagram { s.kind == DATAGRAM } else { s.kind == STREAM || s.kind == LISTENER }
    }

    /// The next ephemeral port no slot of the protocol holds (the slots are
    /// few and the range wide, so the walk is short).
    fn next_ephemeral(st: &mut Stack, datagram: bool) -> u16 {
        loop {
            let p = st.ephemeral;
            st.ephemeral = if p == u16::MAX { EPHEMERAL_FIRST } else { p + 1 };
            if !st.slots.iter().any(|s| on_port(s, p, datagram)) {
                return p;
            }
        }
    }

    /// Whether another slot holds `port` for the protocol, unless that slot
    /// and the binding one (`reuse`) both allow address reuse.
    fn port_taken(st: &Stack, port: u16, datagram: bool, except: usize, reuse: bool) -> bool {
        st.slots.iter().enumerate().any(|(i, s)| i != except && on_port(s, port, datagram) && !(s.reuse && reuse))
    }

    fn reachable(st: &Stack, ip: Ipv4Address) -> bool {
        let Some(our) = st.our_ip else { return false };
        if ip == our {
            return true;
        }
        let m = u32::from_be_bytes(octets(mask_of(st.prefix)));
        if (u32::from_be_bytes(ip.octets()) & m) == (u32::from_be_bytes(our.octets()) & m) {
            return true;
        }
        st.gateway.is_some()
    }

    /// A handle for a stream or datagram socket, its buffers carved from the
    /// pool here: the family IPv4 alone (IPv6 RHU_EAFNOSUPPORT), the two
    /// types with their protocols (0, 6 or 17), the pool's end RHU_EMFILE.
    pub fn socket(family: u32, kind: u32, protocol: u32) -> i32 {
        if family != FAMILY_IPV4 {
            return refused(-1, ERR_AFNOSUPPORT);
        }
        let ok = matches!((kind, protocol), (TYPE_STREAM, 0 | 6) | (TYPE_DATAGRAM, 0 | 17));
        if !ok {
            return refused(-1, ERR_PROTONOSUPPORT);
        }
        let r = with_stack(|st| {
            let Some(h) = st.slots.iter().position(|s| s.kind == FREE) else {
                return Err(ERR_MFILE);
            };
            let mut slot = EMPTY_SLOT;
            if kind == TYPE_STREAM {
                let Some(i) = free_stream(st) else { return Err(ERR_MFILE) };
                st.tcp_used[i] = true;
                slot.kind = STREAM;
                slot.sock = i;
            } else {
                let Some(i) = st.udp_used.iter().position(|u| !u) else { return Err(ERR_MFILE) };
                st.udp_used[i] = true;
                slot.kind = DATAGRAM;
                slot.sock = i;
            }
            st.slots[h] = slot;
            Ok(h as i32)
        });
        match r {
            Ok(h) => {
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).sockets_opened), 1) });
                h
            }
            Err(code) => {
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).sockets_refused), 1) });
                refused(-1, code)
            }
        }
    }

    fn slot_of(st: &Stack, h: i32) -> Option<usize> {
        if h < 0 || h as usize >= HANDLES || st.slots[h as usize].kind == FREE { None } else { Some(h as usize) }
    }

    pub fn bind(h: i32, local: *const Addr) -> i32 {
        let (ip, port) = match read_addr(local) {
            Ok(a) => a,
            Err(code) => return refused(h, code),
        };
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return sched::ERR_INVAL };
            let slot = st.slots[i];
            if slot.bound || port == 0 {
                return sched::ERR_INVAL;
            }
            if !ip.is_unspecified() && Some(ip) != st.our_ip {
                return sched::ERR_INVAL;
            }
            match slot.kind {
                STREAM => {
                    if slot.connecting || slot.was_connected {
                        return sched::ERR_INVAL;
                    }
                    if port_taken(st, port, false, i, slot.reuse) {
                        return ERR_ADDRINUSE;
                    }
                }
                DATAGRAM => {
                    if port_taken(st, port, true, i, slot.reuse) {
                        return ERR_ADDRINUSE;
                    }
                    let s = st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]);
                    if s.bind(port).is_err() {
                        return sched::ERR_INVAL;
                    }
                }
                _ => return sched::ERR_INVAL,
            }
            st.slots[i].bound = true;
            st.slots[i].local_port = port;
            0
        });
        if r != 0 { refused(h, r) } else { 0 }
    }

    /// The bound stream listens on its port with up to `backlog` sockets
    /// pre-listening behind the one handle (bounded by the pool).
    pub fn listen(h: i32, backlog: u32) -> i32 {
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return sched::ERR_INVAL };
            let slot = st.slots[i];
            if slot.kind != STREAM || !slot.bound || slot.connecting || slot.was_connected || backlog == 0 {
                return sched::ERR_INVAL;
            }
            let port = slot.local_port;
            if st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]).listen(port).is_err() {
                return sched::ERR_INVAL;
            }
            let mut s = st.slots[i];
            s.kind = LISTENER;
            s.backlog[0] = s.sock;
            s.backlog_n = 1;
            s.sock = NONE;
            let want = (backlog as usize).min(STREAMS);
            while s.backlog_n < want {
                let Some(j) = free_stream(st) else { break };
                if st.sockets.get_mut::<tcp::Socket>(st.tcp[j]).listen(port).is_err() {
                    break;
                }
                st.tcp_used[j] = true;
                s.backlog[s.backlog_n] = j;
                s.backlog_n += 1;
            }
            st.slots[i] = s;
            0
        });
        if r != 0 {
            return refused(h, r);
        }
        kick();
        0
    }

    /// The first completed connection behind a listener moved to a new
    /// handle, a fresh socket armed in its place where the pool has one.
    pub fn accept(h: i32, peer: *mut Addr) -> i32 {
        if !peer.is_null() && !aligned(peer) {
            return refused(h, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let mut l = st.slots[i];
            if l.kind != LISTENER {
                return Err(sched::ERR_INVAL);
            }
            let mut pick = None;
            for k in 0..l.backlog_n {
                let s = st.sockets.get::<tcp::Socket>(st.tcp[l.backlog[k]]);
                if matches!(s.state(), tcp::State::Established | tcp::State::CloseWait) {
                    pick = Some(k);
                    break;
                }
            }
            let Some(k) = pick else { return Err(sched::ERR_AGAIN) };
            let Some(nh) = st.slots.iter().position(|s| s.kind == FREE) else { return Err(ERR_MFILE) };
            let sock = l.backlog[k];
            // The backlog closes over the gap; a fresh listener in its place when the pool has one.
            for m in k..l.backlog_n - 1 {
                l.backlog[m] = l.backlog[m + 1];
            }
            l.backlog_n -= 1;
            l.backlog[l.backlog_n] = NONE;
            let port = l.local_port;
            if let Some(j) = free_stream(st) {
                if st.sockets.get_mut::<tcp::Socket>(st.tcp[j]).listen(port).is_ok() {
                    st.tcp_used[j] = true;
                    l.backlog[l.backlog_n] = j;
                    l.backlog_n += 1;
                }
            }
            st.slots[i] = l;
            let remote = st.sockets.get::<tcp::Socket>(st.tcp[sock]).remote_endpoint().and_then(|e| match e.addr {
                IpAddress::Ipv4(a) => Some((a, e.port)),
            });
            let mut ns = EMPTY_SLOT;
            ns.kind = STREAM;
            ns.sock = sock;
            ns.was_connected = true;
            ns.bound = true;
            ns.local_port = port;
            ns.peer = remote;
            st.slots[nh] = ns;
            Ok((nh as i32, remote))
        });
        match r {
            Err(code) if code == sched::ERR_AGAIN => code,
            Err(code) => refused(h, code),
            Ok((nh, remote)) => {
                if !peer.is_null() {
                    write_addr(peer, remote);
                }
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).accepts), 1) });
                kick();
                nh
            }
        }
    }

    /// A stream's handshake started (RHU_EINPROGRESS: the readiness word
    /// reports its end), or a datagram socket's default peer set (0).
    pub fn connect(h: i32, peer: *const Addr) -> i32 {
        let (ip, port) = match read_addr(peer) {
            Ok(a) => a,
            Err(code) => return refused(h, code),
        };
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return sched::ERR_INVAL };
            let slot = st.slots[i];
            match slot.kind {
                DATAGRAM => {
                    if !slot.bound {
                        let p = next_ephemeral(st, true);
                        if st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]).bind(p).is_err() {
                            return sched::ERR_INVAL;
                        }
                        st.slots[i].bound = true;
                        st.slots[i].local_port = p;
                    }
                    st.slots[i].peer = Some((ip, port));
                    0
                }
                STREAM => {
                    if slot.connecting {
                        return ERR_ALREADY;
                    }
                    if slot.was_connected || slot.failed {
                        return ERR_ISCONN;
                    }
                    if st.medium.link() != Link::Up {
                        return ERR_NETDOWN;
                    }
                    if port == 0 || !reachable(st, ip) {
                        return ERR_NETUNREACH;
                    }
                    let local = if slot.bound { slot.local_port } else { next_ephemeral(st, false) };
                    let Stack { iface, sockets, tcp, .. } = st;
                    let s = sockets.get_mut::<tcp::Socket>(tcp[slot.sock]);
                    match s.connect(iface.context(), (IpAddress::Ipv4(ip), port), local) {
                        Ok(()) => {
                            let sl = &mut st.slots[i];
                            sl.connecting = true;
                            sl.started_ms = sched::now_ms();
                            sl.bound = true;
                            sl.local_port = local;
                            sl.peer = Some((ip, port));
                            sl.mask = 0;
                            ERR_INPROGRESS
                        }
                        Err(tcp::ConnectError::InvalidState) => ERR_ISCONN,
                        Err(tcp::ConnectError::Unaddressable) => ERR_NETUNREACH,
                    }
                }
                _ => sched::ERR_INVAL,
            }
        });
        if r == ERR_INPROGRESS || r == 0 {
            kick();
            return r;
        }
        refused(h, r)
    }

    /// The bytes taken (a stream takes what fits, never waiting; a datagram
    /// whole or not at all), a pending error consumed first.
    pub fn send(h: i32, buf: *const u8, n: u32, to: *const Addr) -> i32 {
        if buf.is_null() && n > 0 {
            return refused(h, sched::ERR_INVAL);
        }
        let dest = if to.is_null() {
            None
        } else {
            match read_addr(to) {
                Ok(a) => Some(a),
                Err(code) => return refused(h, code),
            }
        };
        let data = if n == 0 { &[][..] } else { unsafe { core::slice::from_raw_parts(buf, n as usize) } };
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let slot = st.slots[i];
            match slot.kind {
                STREAM => {
                    if slot.pending != 0 {
                        let e = slot.pending;
                        st.slots[i].pending = 0;
                        return Err(e);
                    }
                    if slot.connecting {
                        return Err(sched::ERR_AGAIN);
                    }
                    if !slot.was_connected {
                        return Err(ERR_NOTCONN);
                    }
                    let s = st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]);
                    if slot.closing || !s.may_send() {
                        return Err(ERR_PIPE);
                    }
                    match s.send_slice(data) {
                        Ok(0) if n > 0 => Err(sched::ERR_AGAIN),
                        Ok(k) => Ok(k as i32),
                        Err(tcp::SendError::InvalidState) => Err(ERR_PIPE),
                    }
                }
                DATAGRAM => {
                    let Some((ip, port)) = dest.or(slot.peer) else { return Err(ERR_NOTCONN) };
                    if n as usize > DGRAM_BUF {
                        return Err(ERR_MSGSIZE);
                    }
                    if !reachable(st, ip) || port == 0 {
                        return Err(ERR_NETUNREACH);
                    }
                    if !slot.bound {
                        let p = next_ephemeral(st, true);
                        if st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]).bind(p).is_err() {
                            return Err(sched::ERR_INVAL);
                        }
                        st.slots[i].bound = true;
                        st.slots[i].local_port = p;
                    }
                    let s = st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]);
                    match s.send_slice(data, IpEndpoint::new(IpAddress::Ipv4(ip), port)) {
                        Ok(()) => Ok(n as i32),
                        Err(udp::SendError::BufferFull) => Err(sched::ERR_AGAIN),
                        Err(udp::SendError::Unaddressable) => Err(ERR_NETUNREACH),
                    }
                }
                _ => Err(sched::ERR_INVAL),
            }
        });
        match r {
            Ok(k) => {
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).bytes_out), k as u32) });
                kick();
                k
            }
            Err(code) if code == sched::ERR_AGAIN => code,
            Err(code) => refused(h, code),
        }
    }

    /// The bytes read (0 = the peer closed a stream; a datagram whole), or
    /// peeked with the flag (nothing consumed); a pending error consumed first.
    pub fn recv(h: i32, buf: *mut u8, n: u32, flags: u32, from: *mut Addr) -> i32 {
        if (buf.is_null() && n > 0) || flags > RECV_PEEK || (!from.is_null() && !aligned(from)) {
            return refused(h, sched::ERR_INVAL);
        }
        let data = if n == 0 { &mut [][..] } else { unsafe { core::slice::from_raw_parts_mut(buf, n as usize) } };
        let peek = flags == RECV_PEEK;
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let slot = st.slots[i];
            match slot.kind {
                STREAM => {
                    if slot.pending != 0 {
                        let e = slot.pending;
                        st.slots[i].pending = 0;
                        return Err(e);
                    }
                    if slot.connecting {
                        return Err(sched::ERR_AGAIN);
                    }
                    if !slot.was_connected {
                        return Err(ERR_NOTCONN);
                    }
                    if slot.rd_shut {
                        return Ok((0, slot.peer));
                    }
                    let s = st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]);
                    let may_recv = s.may_recv();
                    let got = if peek { s.peek_slice(data) } else { s.recv_slice(data) };
                    match got {
                        Ok(0) if n > 0 && may_recv => Err(sched::ERR_AGAIN),
                        Ok(k) => Ok((k as i32, slot.peer)),
                        Err(tcp::RecvError::Finished) => Ok((0, slot.peer)),
                        Err(tcp::RecvError::InvalidState) => Ok((0, slot.peer)),
                    }
                }
                DATAGRAM => {
                    if slot.rd_shut {
                        return Ok((0, slot.peer));
                    }
                    let s = st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]);
                    let got = if peek { s.peek_slice(data).map(|(k, m)| (k, *m)) } else { s.recv_slice(data) };
                    // The level answered by the readiness door is the judge's, refreshed at the thread's pass; a
                    // read that empties the queue (a truncated one dequeues too) lowers READABLE here, so a caller
                    // more urgent than the thread reads the socket's state at the call, never the pass before.
                    if !peek && !s.can_recv() {
                        st.slots[i].mask &= !READABLE;
                    }
                    match got {
                        Ok((k, meta)) => {
                            let IpAddress::Ipv4(a) = meta.endpoint.addr;
                            Ok((k as i32, Some((a, meta.endpoint.port))))
                        }
                        Err(udp::RecvError::Exhausted) => Err(sched::ERR_AGAIN),
                        Err(udp::RecvError::Truncated) => Err(ERR_MSGSIZE),
                    }
                }
                _ => Err(sched::ERR_INVAL),
            }
        });
        match r {
            Ok((k, peer)) => {
                if !from.is_null() {
                    write_addr(from, peer);
                }
                if !peek {
                    with_cs(|| unsafe { bump(addr_of_mut!((*witness()).bytes_in), k as u32) });
                    kick();
                }
                k
            }
            Err(code) if code == sched::ERR_AGAIN => code,
            Err(code) => refused(h, code),
        }
    }

    /// A stream's queued receive bytes; a datagram socket's next datagram length.
    pub fn available(h: i32) -> i32 {
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let slot = st.slots[i];
            Ok(match slot.kind {
                STREAM => {
                    if !slot.was_connected {
                        0
                    } else {
                        st.sockets.get::<tcp::Socket>(st.tcp[slot.sock]).recv_queue() as i32
                    }
                }
                DATAGRAM => match st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]).peek() {
                    Ok((p, _)) => p.len() as i32,
                    Err(_) => 0,
                },
                _ => return Err(sched::ERR_INVAL),
            })
        });
        match r {
            Ok(k) => k,
            Err(code) => refused(h, code),
        }
    }

    /// 0 read (a slot flag: later reads answer 0), 1 write (the stream's
    /// close handshake), 2 both; 0 on any valid handle.
    pub fn shutdown(h: i32, how: u32) -> i32 {
        if how > SHUT_RDWR {
            return refused(h, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return sched::ERR_INVAL };
            let slot = st.slots[i];
            if how == SHUT_RD || how == SHUT_RDWR {
                st.slots[i].rd_shut = true;
            }
            if (how == SHUT_WR || how == SHUT_RDWR) && slot.kind == STREAM && (slot.was_connected || slot.connecting) {
                st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]).close();
                st.slots[i].closing = true;
            }
            0
        });
        if r != 0 {
            return refused(h, r);
        }
        kick();
        0
    }

    /// The handle released at once and its waiters woken; a stream's close
    /// handshake and its buffer pair stay on the network thread until the
    /// stack removes the connection; a listener's sockets are aborted.
    pub fn close(h: i32) -> i32 {
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return sched::ERR_INVAL };
            let slot = st.slots[i];
            match slot.kind {
                STREAM => {
                    let s = st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]);
                    if slot.was_connected && !slot.failed {
                        s.close();
                    } else {
                        abort_unsynchronized(s);
                    }
                }
                LISTENER => {
                    for k in 0..slot.backlog_n {
                        st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.backlog[k]]).abort();
                    }
                }
                DATAGRAM => {
                    st.sockets.get_mut::<udp::Socket>(st.udp[slot.sock]).close();
                    st.udp_used[slot.sock] = false;
                }
                _ => return sched::ERR_INVAL,
            }
            st.slots[i] = EMPTY_SLOT;
            0
        });
        if r != 0 {
            return refused(h, r);
        }
        EVENT[h as usize].fetch_add(1, Ordering::AcqRel);
        sched::futex_wake(EVENT[h as usize].as_ptr(), u32::MAX);
        GLOBAL.fetch_add(1, Ordering::AcqRel);
        with_cs(|| unsafe { bump(addr_of_mut!((*witness()).global_bumps), 1) });
        sched::futex_wake(GLOBAL.as_ptr(), u32::MAX);
        kick();
        0
    }

    /// The poll-shaped mask: `take` 0 the level, 1 the risen bits since the
    /// last take (cleared).
    pub fn readiness(h: i32, take_bits: u32) -> i32 {
        if take_bits > 1 {
            return refused(h, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            if take_bits == 0 {
                Ok(st.slots[i].mask as i32)
            } else {
                let r = st.slots[i].risen;
                st.slots[i].risen = 0;
                Ok(r as i32)
            }
        });
        match r {
            Ok(m) => m,
            Err(code) => refused(h, code),
        }
    }

    /// The pending code, read once and cleared; 0 when none.
    pub fn error(h: i32) -> i32 {
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let e = st.slots[i].pending;
            st.slots[i].pending = 0;
            Ok(e)
        });
        match r {
            Ok(e) => e,
            Err(code) => refused(h, code),
        }
    }

    pub fn event_word(h: i32) -> *const u32 {
        let used = with_stack(|st| slot_of(st, h).is_some());
        if used { EVENT[h as usize].as_ptr() } else { core::ptr::null() }
    }

    /// The local half always written (a bound socket's ephemeral port
    /// included); the peer half family 0 when unconnected.
    pub fn name(h: i32, local: *mut Addr, peer: *mut Addr) -> i32 {
        if !aligned(local) || (!peer.is_null() && !aligned(peer)) {
            return refused(h, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let slot = st.slots[i];
            let our = st.our_ip.unwrap_or(Ipv4Address::UNSPECIFIED);
            Ok(match slot.kind {
                STREAM => {
                    let s = st.sockets.get::<tcp::Socket>(st.tcp[slot.sock]);
                    let l = match s.local_endpoint() {
                        Some(IpEndpoint { addr: IpAddress::Ipv4(a), port }) => (a, port),
                        None => (if slot.bound { our } else { Ipv4Address::UNSPECIFIED }, slot.local_port),
                    };
                    let p = if slot.was_connected { s.remote_endpoint().map(|e| match e.addr { IpAddress::Ipv4(a) => (a, e.port) }).or(slot.peer) } else { None };
                    (l, p)
                }
                LISTENER => ((our, slot.local_port), None),
                DATAGRAM => ((if slot.bound { our } else { Ipv4Address::UNSPECIFIED }, slot.local_port), slot.peer),
                _ => return Err(sched::ERR_INVAL),
            })
        });
        match r {
            Ok((l, p)) => {
                write_addr(local, Some(l));
                if !peer.is_null() {
                    write_addr(peer, p);
                }
                0
            }
            Err(code) => refused(h, code),
        }
    }

    /// `set` 1 writes the option and answers 0; `set` 0 answers its value.
    /// 1 no-delay, 2 keep-alive (the interval a row fact), 3 reuse-address;
    /// RHU_ENOTSUP for any other, and for the stream options on a datagram socket.
    pub fn option(h: i32, option: u32, set: u32, value: i32) -> i32 {
        if set > 1 {
            return refused(h, sched::ERR_INVAL);
        }
        let r = with_stack(|st| {
            let Some(i) = slot_of(st, h) else { return Err(sched::ERR_INVAL) };
            let slot = st.slots[i];
            match (slot.kind, option) {
                (STREAM, OPT_NODELAY) => {
                    let s = st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]);
                    if set == 1 {
                        s.set_nagle_enabled(value == 0);
                        Ok(0)
                    } else {
                        Ok(!s.nagle_enabled() as i32)
                    }
                }
                (STREAM, OPT_KEEPALIVE) => {
                    let s = st.sockets.get_mut::<tcp::Socket>(st.tcp[slot.sock]);
                    if set == 1 {
                        s.set_keep_alive(if value != 0 { Some(Duration::from_millis(KEEPALIVE_MS)) } else { None });
                        Ok(0)
                    } else {
                        Ok(s.keep_alive().is_some() as i32)
                    }
                }
                (STREAM | LISTENER | DATAGRAM, OPT_REUSEADDR) => {
                    if set == 1 {
                        st.slots[i].reuse = value != 0;
                        Ok(0)
                    } else {
                        Ok(slot.reuse as i32)
                    }
                }
                _ => Err(ERR_NOTSUP),
            }
        });
        match r {
            Ok(v) => v,
            Err(code) => refused(h, code),
        }
    }

    // ---------------- the probes the api flavor's checks read ----------------

    /// The virtual medium's counters and the scripted peer's, copied under the tenancy.
    pub fn probe_medium() -> (VirtWitness, PeerCounts) {
        with_stack(|st| {
            let p = st.medium.peer();
            (
                st.medium.witness,
                PeerCounts {
                    discovers: p.discovers,
                    offers: p.offers,
                    requests: p.requests,
                    acks: p.acks,
                    arps_answered: p.arps_answered,
                    dns_answered: p.dns_answered,
                    dns_refused: p.dns_refused,
                    resets: p.resets,
                    dropped: p.dropped,
                    first_xid: p.first_xid,
                    last_xid: p.last_xid,
                    dropped_len: p.dropped_len,
                    dropped_head: p.dropped_head,
                },
            )
        })
    }

    /// The stream sockets in use (the pool's accounting, a closed stream's
    /// pair tied until the stack removes it).
    /// The interface's view for a check's record: its first IPv4 address and prefix (zeros without one), the
    /// stack's own poll delay in ms at the call (u32::MAX for none), whether a frame is staged.
    pub fn probe_iface() -> ([u8; 4], u8, u32, bool) {
        with_stack(|st| {
            let (a, p) = st.iface.ip_addrs().first().map(|c| match c { IpCidr::Ipv4(c) => (c.address().octets(), c.prefix_len()) }).unwrap_or(([0; 4], 0));
            let now = now_instant();
            let Stack { iface, sockets, medium, .. } = st;
            let d = iface.poll_delay(now, sockets).map(|d| d.total_millis().min(u32::MAX as u64) as u32).unwrap_or(u32::MAX);
            (a, p, d, medium.has_staged())
        })
    }

    pub fn probe_streams_used() -> u32 {
        with_stack(|st| st.tcp_used.iter().filter(|u| **u).count() as u32)
    }
}

/// The scripted peer's counters as the probe copies them.
#[derive(Clone, Copy)]
pub struct PeerCounts {
    pub discovers: u32,
    pub offers: u32,
    pub requests: u32,
    pub acks: u32,
    pub arps_answered: u32,
    pub dns_answered: u32,
    pub dns_refused: u32,
    pub resets: u32,
    pub dropped: u32,
    pub first_xid: u32,
    pub last_xid: u32,
    pub dropped_len: u32,
    pub dropped_head: [u8; 48],
}

impl Default for PeerCounts {
    // By hand: the standard library derives Default for arrays of 32 at most, and the head holds 48 bytes
    // (the Ethernet and IP headers with the ports after them).
    fn default() -> Self {
        PeerCounts {
            discovers: 0,
            offers: 0,
            requests: 0,
            acks: 0,
            arps_answered: 0,
            dns_answered: 0,
            dns_refused: 0,
            resets: 0,
            dropped: 0,
            first_xid: 0,
            last_xid: 0,
            dropped_len: 0,
            dropped_head: [0; 48],
        }
    }
}

#[cfg(rhu_net_rows)]
pub use served::{
    accept, available, bind, close, configure, connect, count, error, event_word, init, lease, limits, listen, medium_info, medium_word, name, option, probe_iface, probe_medium, probe_streams_used, readiness, recv, resolve, resolve_result, resolve_word, send, shutdown, socket, start, CONNECT_BUDGET_MS, SAFETY_MS, DATAGRAMS, KIND_WORD,
    POOL_BYTES, STREAMS,
};

// ---------------- the rows without a medium: the count 0, every other door ENODEV ----------------

#[cfg(not(rhu_net_rows))]
pub fn count() -> i32 {
    0
}
#[cfg(not(rhu_net_rows))]
pub fn medium_info(medium: u32, _out: *mut MediumInfo) -> i32 {
    refused(medium as i32, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn medium_word(_medium: u32) -> *const u32 {
    core::ptr::null()
}
#[cfg(not(rhu_net_rows))]
pub fn configure(medium: u32, _fixed: *const Lease) -> i32 {
    refused(medium as i32, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn lease(medium: u32, _out: *mut Lease) -> i32 {
    refused(medium as i32, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn limits(_out: *mut Limits) -> i32 {
    refused(0, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn resolve(_name: *const u8, _len: u32) -> i32 {
    refused(-1, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn resolve_result(h: i32, _out: *mut Addr, _max: u32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn resolve_word(_h: i32) -> *const u32 {
    core::ptr::null()
}
#[cfg(not(rhu_net_rows))]
pub fn socket(_family: u32, _kind: u32, _protocol: u32) -> i32 {
    refused(-1, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn bind(h: i32, _local: *const Addr) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn listen(h: i32, _backlog: u32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn accept(h: i32, _peer: *mut Addr) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn connect(h: i32, _peer: *const Addr) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn send(h: i32, _buf: *const u8, _n: u32, _to: *const Addr) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn recv(h: i32, _buf: *mut u8, _n: u32, _flags: u32, _from: *mut Addr) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn available(h: i32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn shutdown(h: i32, _how: u32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn close(h: i32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn readiness(h: i32, _take: u32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn error(h: i32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn event_word(_h: i32) -> *const u32 {
    core::ptr::null()
}
#[cfg(not(rhu_net_rows))]
pub fn name(h: i32, _local: *mut Addr, _peer: *mut Addr) -> i32 {
    refused(h, sched::ERR_NODEV)
}
#[cfg(not(rhu_net_rows))]
pub fn option(h: i32, _option: u32, _set: u32, _value: i32) -> i32 {
    refused(h, sched::ERR_NODEV)
}
