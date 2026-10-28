//! The network stack's arithmetic matcher: the Internet checksum (RFC 1071) over bytes,
//! the IPv4 header checksum, and the IPv4 pseudo-header checksums of UDP,
//! TCP and ICMPv4 -- the sums the virtual medium's scripted peer writes
//! into the frames it answers with, so the stack's own parser verifies them
//! as a real peer's. The host tier's cases pin this matcher against that
//! parser; the kernel compiles this module under the `net`
//! feature alone, so with the feature off no row's image carries a byte of
//! it.

/// A 32-bit accumulator folded to 16 bits: the carries added back until
/// none is left (two rounds suffice for any sum of 16-bit words that fits
/// in 32 bits; the loop states the rule, not the count).
const fn fold(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum as u16
}

/// The ones'-complement sum of `data` as big-endian 16-bit words, folded,
/// NOT complemented, on top of `seed` (a folded sum already in hand -- a
/// pseudo-header's, or the words before a field the caller skips). A sum
/// taken in two calls must split on a word boundary: an odd first part
/// would pad its last byte as a word of its own.
pub const fn ones_complement_sum(seed: u16, data: &[u8]) -> u16 {
    let mut sum: u32 = seed as u32;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    fold(sum)
}

/// The Internet checksum of `data`: the folded sum, complemented.
pub const fn checksum(data: &[u8]) -> u16 {
    !ones_complement_sum(0, data)
}

/// The sum over a buffer with one 16-bit field taken as zero whatever it
/// holds: the words before the field at `field` (an even offset), then the
/// words after it, on top of `seed`.
const fn sum_skipping(seed: u16, data: &[u8], field: usize) -> u16 {
    let (head, _) = data.split_at(field);
    let (_, tail) = data.split_at(field + 2);
    ones_complement_sum(ones_complement_sum(seed, head), tail)
}

/// The IPv4 header checksum of `header` (the whole header, options
/// included, its checksum field at bytes 10 and 11 taken as zero whatever
/// it holds): the value the field must carry for a receiver's sum over the
/// header to come to all ones. The header is at least the 20 bytes of a
/// header without options.
pub const fn ipv4_header_checksum(header: &[u8]) -> u16 {
    assert!(header.len() >= 20, "an IPv4 header is at least 20 bytes");
    !sum_skipping(0, header, 10)
}

/// The pseudo-header sum of RFC 768 and RFC 793 over IPv4: the source and
/// destination addresses, a zero byte, the protocol number and the
/// transport length (the header and payload together), folded and not
/// complemented -- the seed of a transport checksum.
pub const fn pseudo_header_v4(src: [u8; 4], dst: [u8; 4], protocol: u8, length: u16) -> u16 {
    let words = [
        src[0], src[1], src[2], src[3], dst[0], dst[1], dst[2], dst[3], 0, protocol, (length >> 8) as u8, length as u8,
    ];
    ones_complement_sum(0, &words)
}

/// The protocol numbers the pseudo-header carries (the IANA assignments).
pub const PROTOCOL_ICMP: u8 = 1;
pub const PROTOCOL_TCP: u8 = 6;
pub const PROTOCOL_UDP: u8 = 17;

/// The UDP checksum over IPv4 of `datagram` (the 8-byte header with its
/// checksum field at bytes 6 and 7 taken as zero, then the payload): the
/// pseudo-header seeds the sum; a result of zero is sent as all ones, since
/// a zero field means no checksum was taken (RFC 768).
pub const fn udp_checksum_v4(src: [u8; 4], dst: [u8; 4], datagram: &[u8]) -> u16 {
    assert!(datagram.len() >= 8, "a UDP datagram is at least its 8-byte header");
    let seed = pseudo_header_v4(src, dst, PROTOCOL_UDP, datagram.len() as u16);
    let sum = !sum_skipping(seed, datagram, 6);
    if sum == 0 { 0xffff } else { sum }
}

/// The TCP checksum over IPv4 of `segment` (the header with its checksum
/// field at bytes 16 and 17 taken as zero, options and payload following):
/// the pseudo-header seeds the sum (RFC 793).
pub const fn tcp_checksum_v4(src: [u8; 4], dst: [u8; 4], segment: &[u8]) -> u16 {
    assert!(segment.len() >= 20, "a TCP segment is at least its 20-byte header");
    let seed = pseudo_header_v4(src, dst, PROTOCOL_TCP, segment.len() as u16);
    !sum_skipping(seed, segment, 16)
}

/// The ICMPv4 checksum of `message` (the type and code, the checksum field
/// at bytes 2 and 3 taken as zero, the rest of the message): no
/// pseudo-header (RFC 792).
pub const fn icmpv4_checksum(message: &[u8]) -> u16 {
    assert!(message.len() >= 4, "an ICMP message is at least its 4-byte header");
    !sum_skipping(0, message, 2)
}
