//! Transcript and artifact-text checks: the assertion helpers every gate
//! composes its cases from.

/// Count of raw CR (0x0D) bytes. Wire discipline is CRLF; a capture with
/// zero CR bytes means the capture path changed, whatever the content says.
pub fn cr_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == 0x0D).count()
}

/// The buffer with every CR byte removed (the comparison form; the raw
/// capture on disk is never rewritten).
pub fn lf_normalize(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().copied().filter(|&b| b != 0x0D).collect()
}

/// Fixed-string presence over the raw bytes (marker checks never use
/// patterns; a needle spans no line break by construction).
pub fn contains_fixed(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    hay.windows(n.len()).any(|w| w == n)
}

/// Equality after LF normalization of both sides (golden compares; the CR
/// discipline is asserted separately on the raw capture).
pub fn eq_lf_normalized(a: &[u8], b: &[u8]) -> bool {
    lf_normalize(a) == lf_normalize(b)
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Word-bounded symbol presence in an nm dump: the match may not touch a
/// `[A-Za-z0-9_]` byte on either side, so `__rhu_sched` never satisfies a
/// check for `__rhu_s`, and a `_ext`-suffixed relative never satisfies the
/// contract symbol's check.
pub fn word_sym_present(dump: &[u8], sym: &str) -> bool {
    let n = sym.as_bytes();
    if n.is_empty() {
        return false;
    }
    let mut i = 0;
    while i + n.len() <= dump.len() {
        if &dump[i..i + n.len()] == n {
            let left_ok = i == 0 || !is_word_byte(dump[i - 1]);
            let right = i + n.len();
            let right_ok = right == dump.len() || !is_word_byte(dump[right]);
            if left_ok && right_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Word-bounded symbol VALUE from an nm dump (the bracket-symbol
/// value asserts). nm line shape: `<hex> <type> <name>`; the name match
/// is word-bounded exactly as `word_sym_present`, and the value is the
/// line's leading hex field. None when the symbol is absent or its line
/// has no parseable leading value -- callers REFUSE on None (a missing
/// bracket is never a silent zero).
pub fn word_sym_value(dump: &[u8], sym: &str) -> Option<u64> {
    for (line, _) in lines_with_endings(&lf_normalize(dump)) {
        let mut fields = line.split(|&b| b == b' ' || b == b'\t').filter(|f| !f.is_empty());
        let (Some(val), _ty, Some(name)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        if name == sym.as_bytes() && fields.next().is_none() {
            let s = core::str::from_utf8(val).ok()?;
            return u64::from_str_radix(s, 16).ok();
        }
    }
    None
}

/// Lines of an LF-normalized buffer, each with a flag for whether it was
/// newline-terminated (the final line of a capture may not be; filters
/// preserve that property so filtered output stays byte-faithful).
fn lines_with_endings(lf: &[u8]) -> Vec<(&[u8], bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &b) in lf.iter().enumerate() {
        if b == b'\n' {
            out.push((&lf[start..i], true));
            start = i + 1;
        }
    }
    if start < lf.len() {
        out.push((&lf[start..], false));
    }
    out
}

/// The payload-only stream: LF-normalized, with kernel-prefixed lines
/// (`rhu ` banners, `[` subsystem tags) and blank lines removed. This is
/// the goldens' own capture method, applied identically on both sides of
/// every compare.
pub fn payload_stream(raw: &[u8]) -> Vec<u8> {
    let lf = lf_normalize(raw);
    let mut out = Vec::new();
    for (line, terminated) in lines_with_endings(&lf) {
        let keep = !(line.is_empty() || line.starts_with(b"rhu ") || line.starts_with(b"["));
        if keep {
            out.extend_from_slice(line);
            if terminated {
                out.push(b'\n');
            }
        }
    }
    out
}

/// The worker wake order as a compact digit string ("123" when correct):
/// every `worker <d> awake` occurrence's digit, in stream order.
pub fn wake_order(raw: &[u8]) -> String {
    let mut out = String::new();
    let hay = raw;
    let mut i = 0;
    let pre = b"worker ";
    let post = b" awake";
    while i + pre.len() + 1 + post.len() <= hay.len() {
        if &hay[i..i + pre.len()] == pre {
            let d = hay[i + pre.len()];
            if (b'1'..=b'3').contains(&d) && &hay[i + pre.len() + 1..i + pre.len() + 1 + post.len()] == post {
                out.push(d as char);
                i += pre.len() + 1 + post.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The last `gctest: collections=<n> free=<n>` line, parsed: the two
/// captured fields and the rest of that line after the free field (the
/// verdict's place). The golden's own final line is the source of
/// expected values -- no second copy to drift.
fn final_summary(stream: &[u8]) -> Option<(String, String, Vec<u8>)> {
    let lf = lf_normalize(stream);
    let mut found = None;
    for (line, _) in lines_with_endings(&lf) {
        let rest = match line.strip_prefix(b"gctest: collections=") {
            Some(r) => r,
            None => continue,
        };
        let gcs_end = rest.iter().position(|b| !b.is_ascii_digit()).unwrap_or(rest.len());
        if gcs_end == 0 {
            continue;
        }
        let after = &rest[gcs_end..];
        let free_digits = match after.strip_prefix(b" free=") {
            Some(f) => f,
            None => continue,
        };
        let free_end = free_digits
            .iter()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(free_digits.len());
        if free_end == 0 {
            continue;
        }
        found = Some((
            String::from_utf8_lossy(&rest[..gcs_end]).into_owned(),
            String::from_utf8_lossy(&free_digits[..free_end]).into_owned(),
            free_digits[free_end..].to_vec(),
        ));
    }
    found
}

/// Final free= value of the last gctest summary line.
pub fn final_free(stream: &[u8]) -> Option<String> {
    final_summary(stream).map(|(_, free, _)| free)
}

/// Final collections= value of the last gctest summary line.
pub fn final_gcs(stream: &[u8]) -> Option<String> {
    final_summary(stream).map(|(gcs, _, _)| gcs)
}

/// True when the last gctest summary line carries `needle` after its two
/// fields: a verdict is read on the verdict line and nowhere else in the
/// stream -- a diagnostic that merely mentions the words is not one, and
/// a stream with no summary line has no verdict.
pub fn final_verdict_has(stream: &[u8], needle: &str) -> bool {
    match final_summary(stream) {
        Some((_, _, tail)) => contains_fixed(&tail, needle),
        None => false,
    }
}
