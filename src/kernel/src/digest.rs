//! SHA-256 (FIPS 180-4) as a stream: bytes fed in any slices, the digest
//! taken at the end. Pure arithmetic on `core` alone, with no table beyond
//! the standard's sixty-four constants and a schedule of sixteen words, so
//! one block costs 64 bytes of stack. The kernel checks the radio's three
//! images against their pins with it at a start, where they lie in the
//! board's flash.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

/// The digest's length in bytes.
pub const DIGEST_BYTES: usize = 32;

/// A digest in progress: the eight state words, the block being filled and
/// the count of bytes fed so far (112 bytes in all).
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    fill: usize,
    bytes: u64,
}

impl Sha256 {
    pub const fn new() -> Self {
        Sha256 { state: INITIAL, block: [0; 64], fill: 0, bytes: 0 }
    }

    /// `data` fed; any split of a message gives the digest of the whole.
    pub fn update(&mut self, mut data: &[u8]) {
        self.bytes = self.bytes.wrapping_add(data.len() as u64);
        if self.fill != 0 {
            let take = (64 - self.fill).min(data.len());
            self.block[self.fill..self.fill + take].copy_from_slice(&data[..take]);
            self.fill += take;
            data = &data[take..];
            if self.fill < 64 {
                return;
            }
            let block = self.block;
            compress(&mut self.state, &block);
            self.fill = 0;
        }
        let mut blocks = data.chunks_exact(64);
        for block in &mut blocks {
            compress(&mut self.state, block);
        }
        let rest = blocks.remainder();
        self.block[..rest.len()].copy_from_slice(rest);
        self.fill = rest.len();
    }

    /// The digest of everything fed: the padding (one 0x80 byte, zeros to
    /// 56 modulo 64, the bit length big-endian) and the state big-endian.
    pub fn finish(mut self) -> [u8; DIGEST_BYTES] {
        let bits = self.bytes.wrapping_mul(8);
        let zeros = if self.fill < 56 { 55 - self.fill } else { 119 - self.fill };
        let mut pad = [0u8; 72];
        pad[0] = 0x80;
        pad[1 + zeros..9 + zeros].copy_from_slice(&bits.to_be_bytes());
        self.update(&pad[..9 + zeros]);
        let mut out = [0u8; DIGEST_BYTES];
        for (i, word) in self.state.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// One block of 64 bytes folded into the state.
fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 16];
    for (i, word) in w.iter_mut().enumerate() {
        *word = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        if i >= 16 {
            // The schedule in place: word i is word i-16 plus the two small
            // sigmas of words i-15 and i-2 and word i-7, all modulo sixteen.
            let w15 = w[(i + 1) & 15];
            let w2 = w[(i + 14) & 15];
            let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
            let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
            w[i & 15] = w[i & 15].wrapping_add(s0).wrapping_add(w[(i + 9) & 15]).wrapping_add(s1);
        }
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i & 15]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (word, add) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *word = word.wrapping_add(add);
    }
}

/// A digest as the 64 lowercase hexadecimal characters a pin is published
/// in.
pub fn hex(digest: &[u8; DIGEST_BYTES]) -> [u8; 2 * DIGEST_BYTES] {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 2 * DIGEST_BYTES];
    for (i, b) in digest.iter().enumerate() {
        out[2 * i] = DIGITS[(b >> 4) as usize];
        out[2 * i + 1] = DIGITS[(b & 15) as usize];
    }
    out
}
