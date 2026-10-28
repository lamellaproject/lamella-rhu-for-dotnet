//! The network doors' scripted peer, pure: the other end of the emulator rows' virtual
//! medium. It reads the Ethernet frames the kernel's stack transmits and
//! answers, byte for byte from the protocols' public specifications, the
//! exchanges a loopback cannot answer -- an ARP request for its address
//! (RFC 826), a DHCP discover and request (RFC 2131 and 2132: an offer and
//! an acknowledgement carrying one fixed lease record), a DNS query for one
//! of its three names or for a name it does not have (RFC 1035: an answer
//! or a name error), an ICMP echo request (RFC 792: the reply), and a TCP
//! connection attempt to its one port (RFC 793: a reset, so a refused
//! connect is proven on the wire). Every frame it does not answer is handed
//! back to the stack as received (the loopback), unless it was addressed to
//! the peer alone, which it drops.
//!
//! Every checksum the peer writes is `net_math`'s (RFC 1071), which the
//! host tier pins against the stack's own parser; the host tier parses
//! every reply built here with the stack's wire types and verifies their
//! checksums. The scenario the gate wants is in the
//! peer's state: the FIRST discover it ever sees goes unanswered, so the
//! stack's retry path is proven with no network, and a doctored peer
//! (`arp_silent`) never answers an ARP request, the api gate's control.

use crate::net_math;

/// The peer's hardware address (locally administered, unicast) and the
/// stack's own as the virtual medium derives it (the two differ in their
/// last byte).
pub const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0xFE];
pub const OUR_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
pub const BROADCAST_MAC: [u8; 6] = [0xFF; 6];

/// The segment (RFC 5737's TEST-NET-1, never routed): the peer is the DHCP
/// server, the router and the first name server; the lease it hands out;
/// the second name server it names; an address nobody on the segment owns
/// (an ARP for it is never answered: the connect budget's honest path);
/// an address off the segment (TEST-NET-2).
pub const PEER_IP: [u8; 4] = [192, 0, 2, 1];
pub const LEASE_IP: [u8; 4] = [192, 0, 2, 2];
pub const LEASE_MASK: [u8; 4] = [255, 255, 255, 0];
pub const LEASE_DNS2: [u8; 4] = [192, 0, 2, 3];
pub const LEASE_SECONDS: u32 = 3600;
pub const UNOWNED_IP: [u8; 4] = [192, 0, 2, 9];
pub const OFF_SEGMENT_IP: [u8; 4] = [198, 51, 100, 1];
pub const BROADCAST_IP: [u8; 4] = [255, 255, 255, 255];

/// The one TCP port the peer answers: with a reset.
pub const REFUSED_PORT: u16 = 7;

/// The names the peer resolves (RFC 6761's reserved `.test`) and the one it
/// does not have.
pub const NAMES: [(&str, [u8; 4]); 3] = [("peer.rhu.test", PEER_IP), ("second.rhu.test", LEASE_DNS2), ("third.rhu.test", [192, 0, 2, 4])];
pub const UNKNOWN_NAME: &str = "nowhere.rhu.test";
pub const DNS_TTL: u32 = 300;

/// The largest reply the peer builds (a DHCP acknowledgement, 320 bytes)
/// and the frame unit of the medium.
pub const MAX_REPLY: usize = 1514;

const ETH: usize = 14;
const IP: usize = 20;
const UDP: usize = 8;
const ETHERTYPE_ARP: u16 = 0x0806;
const ETHERTYPE_IPV4: u16 = 0x0800;
const PROTO_ICMP: u8 = 1;
const PROTO_TCP: u8 = 6;
const PROTO_UDP: u8 = 17;
const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;
const DNS_PORT: u16 = 53;
const DHCP_FIXED: usize = 240; // the fixed fields and the magic cookie
const DHCP_MAGIC: [u8; 4] = [0x63, 0x82, 0x53, 0x63];
const DHCP_DISCOVER: u8 = 1;
const DHCP_OFFER: u8 = 2;
const DHCP_REQUEST: u8 = 3;
const DHCP_ACK: u8 = 5;

/// What the peer did with a transmitted frame: handed it back as received
/// (the loopback), swallowed it, or answered it with `n` bytes in `out`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Pass,
    Drop,
    Reply(usize),
}

/// The peer's state and counters (read by the api flavor's checks through
/// the medium's witness).
pub struct Peer {
    /// The doctored peer of the api gate's control: never answers an ARP request.
    pub arp_silent: bool,
    pub discovers: u32,
    pub offers: u32,
    pub requests: u32,
    pub acks: u32,
    pub arps_answered: u32,
    pub dns_answered: u32,
    pub dns_refused: u32,
    pub echoes: u32,
    pub resets: u32,
    pub dropped: u32,
    /// The transaction id of the last discover seen and of the first.
    pub first_xid: u32,
    pub last_xid: u32,
    /// The head of the last frame dropped (a frame to the peer's address it
    /// did not answer) and that frame's length: the record a check prints
    /// when a frame the peer should have answered went unanswered.
    pub dropped_len: u32,
    pub dropped_head: [u8; 48],
}

impl Peer {
    pub const fn new(arp_silent: bool) -> Self {
        Peer { arp_silent, discovers: 0, offers: 0, requests: 0, acks: 0, arps_answered: 0, dns_answered: 0, dns_refused: 0, echoes: 0, resets: 0, dropped: 0, first_xid: 0, last_xid: 0, dropped_len: 0, dropped_head: [0; 48] }
    }

    /// The peer's answer to one transmitted frame. `out` holds at least
    /// `MAX_REPLY` bytes.
    pub fn answer(&mut self, frame: &[u8], out: &mut [u8]) -> Verdict {
        if frame.len() < ETH || out.len() < MAX_REPLY {
            return Verdict::Pass;
        }
        let to_peer = frame[0..6] == PEER_MAC;
        let broadcast = frame[0..6] == BROADCAST_MAC;
        let ethertype = be16(&frame[12..14]);
        let v = match ethertype {
            ETHERTYPE_ARP => self.answer_arp(frame, out),
            ETHERTYPE_IPV4 => self.answer_ipv4(frame, out, to_peer, broadcast),
            _ => Verdict::Pass,
        };
        if v == Verdict::Pass && to_peer {
            self.dropped += 1;
            let n = frame.len().min(self.dropped_head.len());
            self.dropped_head[..n].copy_from_slice(&frame[..n]);
            self.dropped_len = frame.len() as u32;
            return Verdict::Drop;
        }
        v
    }

    fn answer_arp(&mut self, frame: &[u8], out: &mut [u8]) -> Verdict {
        let a = &frame[ETH..];
        if a.len() < 28 || be16(&a[0..2]) != 1 || be16(&a[2..4]) != ETHERTYPE_IPV4 || a[4] != 6 || a[5] != 4 || be16(&a[6..8]) != 1 {
            return Verdict::Pass;
        }
        if a[24..28] != PEER_IP {
            return Verdict::Pass;
        }
        if self.arp_silent {
            return Verdict::Pass;
        }
        // The reply: to the asker's hardware and protocol addresses, from the peer's.
        let sha = &a[8..14];
        let spa = &a[14..18];
        eth_header(out, sha, &PEER_MAC, ETHERTYPE_ARP);
        let r = &mut out[ETH..ETH + 28];
        r[0..2].copy_from_slice(&1u16.to_be_bytes());
        r[2..4].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
        r[4] = 6;
        r[5] = 4;
        r[6..8].copy_from_slice(&2u16.to_be_bytes());
        r[8..14].copy_from_slice(&PEER_MAC);
        r[14..18].copy_from_slice(&PEER_IP);
        r[18..24].copy_from_slice(sha);
        r[24..28].copy_from_slice(spa);
        // An Ethernet frame is at least 60 bytes before its check sequence; the padding is zero.
        for b in &mut out[ETH + 28..60] {
            *b = 0;
        }
        self.arps_answered += 1;
        Verdict::Reply(60)
    }

    fn answer_ipv4(&mut self, frame: &[u8], out: &mut [u8], to_peer: bool, broadcast: bool) -> Verdict {
        let ip = &frame[ETH..];
        if ip.len() < IP || ip[0] != 0x45 {
            return Verdict::Pass;
        }
        let total = be16(&ip[2..4]) as usize;
        if total < IP || total > ip.len() {
            return Verdict::Pass;
        }
        let proto = ip[9];
        let src_ip: [u8; 4] = [ip[12], ip[13], ip[14], ip[15]];
        let dst_ip: [u8; 4] = [ip[16], ip[17], ip[18], ip[19]];
        let payload = &ip[IP..total];
        let for_peer = dst_ip == PEER_IP && (to_peer || broadcast);
        match proto {
            PROTO_UDP if payload.len() >= UDP => {
                let (sp, dp) = (be16(&payload[0..2]), be16(&payload[2..4]));
                let ulen = be16(&payload[4..6]) as usize;
                if ulen < UDP || ulen > payload.len() {
                    return Verdict::Pass;
                }
                let data = &payload[UDP..ulen];
                if dp == DHCP_SERVER_PORT && sp == DHCP_CLIENT_PORT && dst_ip == BROADCAST_IP {
                    return self.answer_dhcp(data, out);
                }
                if dp == DNS_PORT && for_peer {
                    return self.answer_dns(data, &frame[6..12], &src_ip, sp, out);
                }
                Verdict::Pass
            }
            PROTO_ICMP if for_peer && payload.len() >= 8 && payload[0] == 8 && payload[1] == 0 => {
                let n = ETH + IP + payload.len();
                eth_header(out, &frame[6..12], &PEER_MAC, ETHERTYPE_IPV4);
                let icmp = &mut out[ETH + IP..n];
                icmp.copy_from_slice(payload);
                icmp[0] = 0;
                icmp[2..4].copy_from_slice(&[0, 0]);
                let sum = net_math::icmpv4_checksum(icmp);
                icmp[2..4].copy_from_slice(&sum.to_be_bytes());
                ipv4_header(&mut out[ETH..ETH + IP], PROTO_ICMP, &PEER_IP, &src_ip, (IP + payload.len()) as u16);
                self.echoes += 1;
                Verdict::Reply(n)
            }
            PROTO_TCP if for_peer && payload.len() >= 20 && payload[13] & 0x02 != 0 && payload[13] & 0x10 == 0 => {
                // A connection attempt (SYN alone) to the peer's one port: a reset that acknowledges the SYN.
                let dp = be16(&payload[2..4]);
                if dp != REFUSED_PORT {
                    return Verdict::Pass;
                }
                let sp = be16(&payload[0..2]);
                let seq = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                eth_header(out, &frame[6..12], &PEER_MAC, ETHERTYPE_IPV4);
                let t = &mut out[ETH + IP..ETH + IP + 20];
                t.fill(0);
                t[0..2].copy_from_slice(&dp.to_be_bytes());
                t[2..4].copy_from_slice(&sp.to_be_bytes());
                t[8..12].copy_from_slice(&seq.wrapping_add(1).to_be_bytes());
                t[12] = 0x50;
                t[13] = 0x14; // RST | ACK
                let sum = net_math::tcp_checksum_v4(PEER_IP, src_ip, t);
                t[16..18].copy_from_slice(&sum.to_be_bytes());
                ipv4_header(&mut out[ETH..ETH + IP], PROTO_TCP, &PEER_IP, &src_ip, (IP + 20) as u16);
                for b in &mut out[ETH + IP + 20..60] {
                    *b = 0;
                }
                self.resets += 1;
                Verdict::Reply(60)
            }
            _ => Verdict::Pass,
        }
    }

    /// A client's discover or request: the offer or the acknowledgement
    /// with the fixed lease, the transaction id and the client's hardware
    /// address copied back; the first discover ever seen goes unanswered
    /// (the retry scenario).
    fn answer_dhcp(&mut self, data: &[u8], out: &mut [u8]) -> Verdict {
        if data.len() < DHCP_FIXED || data[0] != 1 || data[1] != 1 || data[2] != 6 || data[236..240] != DHCP_MAGIC {
            return Verdict::Pass;
        }
        let Some(kind) = dhcp_option(&data[DHCP_FIXED..], 53).and_then(|o| o.first().copied()) else {
            return Verdict::Pass;
        };
        let xid = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let reply_kind = match kind {
            DHCP_DISCOVER => {
                self.discovers += 1;
                self.last_xid = xid;
                if self.discovers == 1 {
                    self.first_xid = xid;
                    self.dropped += 1;
                    return Verdict::Drop;
                }
                self.offers += 1;
                DHCP_OFFER
            }
            DHCP_REQUEST => {
                self.requests += 1;
                self.acks += 1;
                DHCP_ACK
            }
            _ => return Verdict::Drop,
        };
        // The reply is broadcast: the client may not hold an address yet.
        eth_header(out, &BROADCAST_MAC, &PEER_MAC, ETHERTYPE_IPV4);
        let d = &mut out[ETH + IP + UDP..];
        d[..DHCP_FIXED].fill(0);
        d[0] = 2; // a reply
        d[1] = 1;
        d[2] = 6;
        d[4..8].copy_from_slice(&data[4..8]);
        d[10..12].copy_from_slice(&data[10..12]); // the flags (the broadcast bit as the client set it)
        d[16..20].copy_from_slice(&LEASE_IP); // yiaddr
        d[20..24].copy_from_slice(&PEER_IP); // siaddr
        d[28..44].copy_from_slice(&data[28..44]); // chaddr
        d[236..240].copy_from_slice(&DHCP_MAGIC);
        let mut o = DHCP_FIXED;
        o = put_option(d, o, 53, &[reply_kind]);
        o = put_option(d, o, 54, &PEER_IP);
        o = put_option(d, o, 51, &LEASE_SECONDS.to_be_bytes());
        o = put_option(d, o, 1, &LEASE_MASK);
        o = put_option(d, o, 3, &PEER_IP);
        let mut dns = [0u8; 8];
        dns[..4].copy_from_slice(&PEER_IP);
        dns[4..].copy_from_slice(&LEASE_DNS2);
        o = put_option(d, o, 6, &dns);
        d[o] = 255;
        o += 1;
        if o & 1 != 0 {
            d[o] = 0;
            o += 1;
        }
        let n = udp_and_ip(out, &PEER_IP, &BROADCAST_IP, DHCP_SERVER_PORT, DHCP_CLIENT_PORT, o);
        Verdict::Reply(n)
    }

    /// A query for one of the peer's names: the answer with the A record;
    /// for any other name the name error (NXDOMAIN).
    fn answer_dns(&mut self, q: &[u8], asker_mac: &[u8], asker: &[u8; 4], asker_port: u16, out: &mut [u8]) -> Verdict {
        if q.len() < 12 || q[2] & 0x80 != 0 || be16(&q[4..6]) != 1 {
            return Verdict::Pass;
        }
        // The question: the name's labels, then the type and class.
        let mut i = 12;
        let mut name = [0u8; 256];
        let mut n = 0usize;
        loop {
            if i >= q.len() {
                return Verdict::Pass;
            }
            let l = q[i] as usize;
            i += 1;
            if l == 0 {
                break;
            }
            if l & 0xC0 != 0 || i + l > q.len() || n + l + 1 > 255 {
                return Verdict::Pass;
            }
            if n > 0 {
                name[n] = b'.';
                n += 1;
            }
            name[n..n + l].copy_from_slice(&q[i..i + l]);
            n += l;
            i += l;
        }
        if i + 4 > q.len() {
            return Verdict::Pass;
        }
        let qend = i + 4;
        let (qtype, qclass) = (be16(&q[i..i + 2]), be16(&q[i + 2..i + 4]));
        let answer = if qtype == 1 && qclass == 1 { lookup(&name[..n]) } else { None };
        eth_header(out, asker_mac, &PEER_MAC, ETHERTYPE_IPV4);
        let d = &mut out[ETH + IP + UDP..];
        d[0..2].copy_from_slice(&q[0..2]);
        let flags: u16 = if answer.is_some() { 0x8180 } else { 0x8183 };
        d[2..4].copy_from_slice(&flags.to_be_bytes());
        d[4..6].copy_from_slice(&1u16.to_be_bytes());
        d[6..8].copy_from_slice(&(answer.is_some() as u16).to_be_bytes());
        d[8..12].fill(0);
        d[12..qend].copy_from_slice(&q[12..qend]);
        let mut o = qend;
        if let Some(addr) = answer {
            d[o..o + 2].copy_from_slice(&[0xC0, 0x0C]);
            d[o + 2..o + 4].copy_from_slice(&1u16.to_be_bytes());
            d[o + 4..o + 6].copy_from_slice(&1u16.to_be_bytes());
            d[o + 6..o + 10].copy_from_slice(&DNS_TTL.to_be_bytes());
            d[o + 10..o + 12].copy_from_slice(&4u16.to_be_bytes());
            d[o + 12..o + 16].copy_from_slice(&addr);
            o += 16;
            self.dns_answered += 1;
        } else {
            self.dns_refused += 1;
        }
        let n = udp_and_ip(out, &PEER_IP, asker, DNS_PORT, asker_port, o);
        Verdict::Reply(n)
    }
}

/// The address of a name the peer has (the labels joined by dots, as the
/// question spells them; case-insensitive as RFC 1035 asks).
pub fn lookup(name: &[u8]) -> Option<[u8; 4]> {
    NAMES.iter().find(|(n, _)| n.len() == name.len() && n.bytes().zip(name.iter()).all(|(a, &b)| a.eq_ignore_ascii_case(&b))).map(|(_, a)| *a)
}

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

/// The value of a DHCP option by its tag, scanning the options field.
fn dhcp_option(opts: &[u8], tag: u8) -> Option<&[u8]> {
    let mut i = 0;
    while i < opts.len() {
        let t = opts[i];
        if t == 255 {
            return None;
        }
        if t == 0 {
            i += 1;
            continue;
        }
        if i + 1 >= opts.len() {
            return None;
        }
        let l = opts[i + 1] as usize;
        if i + 2 + l > opts.len() {
            return None;
        }
        if t == tag {
            return Some(&opts[i + 2..i + 2 + l]);
        }
        i += 2 + l;
    }
    None
}

fn put_option(d: &mut [u8], at: usize, tag: u8, value: &[u8]) -> usize {
    d[at] = tag;
    d[at + 1] = value.len() as u8;
    d[at + 2..at + 2 + value.len()].copy_from_slice(value);
    at + 2 + value.len()
}

fn eth_header(out: &mut [u8], dst: &[u8], src: &[u8], ethertype: u16) {
    out[0..6].copy_from_slice(dst);
    out[6..12].copy_from_slice(src);
    out[12..14].copy_from_slice(&ethertype.to_be_bytes());
}

/// An IPv4 header without options: the identification 0, don't-fragment,
/// the time to live 64, the checksum computed.
fn ipv4_header(h: &mut [u8], proto: u8, src: &[u8; 4], dst: &[u8; 4], total_len: u16) {
    h[0] = 0x45;
    h[1] = 0;
    h[2..4].copy_from_slice(&total_len.to_be_bytes());
    h[4..6].copy_from_slice(&[0, 0]);
    h[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    h[8] = 64;
    h[9] = proto;
    h[10..12].copy_from_slice(&[0, 0]);
    h[12..16].copy_from_slice(src);
    h[16..20].copy_from_slice(dst);
    let sum = net_math::ipv4_header_checksum(h);
    h[10..12].copy_from_slice(&sum.to_be_bytes());
}

/// The UDP header and the IPv4 header around a payload of `len` bytes
/// already written at its place in `out` (the Ethernet header is the
/// caller's). Answers the frame's length, padded to 60.
fn udp_and_ip(out: &mut [u8], src: &[u8; 4], dst: &[u8; 4], sp: u16, dp: u16, len: usize) -> usize {
    let ulen = UDP + len;
    {
        let u = &mut out[ETH + IP..ETH + IP + ulen];
        u[0..2].copy_from_slice(&sp.to_be_bytes());
        u[2..4].copy_from_slice(&dp.to_be_bytes());
        u[4..6].copy_from_slice(&(ulen as u16).to_be_bytes());
        u[6..8].copy_from_slice(&[0, 0]);
        let sum = net_math::udp_checksum_v4(*src, *dst, u);
        u[6..8].copy_from_slice(&sum.to_be_bytes());
    }
    ipv4_header(&mut out[ETH..ETH + IP], PROTO_UDP, src, dst, (IP + ulen) as u16);
    let mut n = ETH + IP + ulen;
    while n < 60 {
        out[n] = 0;
        n += 1;
    }
    n
}
