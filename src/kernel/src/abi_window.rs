//! The hosting-API contract generation window: which stated generation a
//! payload's own entry may carry and still run on this kernel. Pure -- no
//! I/O, no state -- so the payload flavor judges with it before the entry
//! runs and the host tier tests it on the host toolchain.

/// The judgement over the handshake's four facts: whether the payload
/// provided its own entry, the generation it states, this kernel's
/// generation, the window's floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The kernel's default entry; the payload states nothing.
    DefaultUnstated,
    /// The kernel's default entry; the payload states a generation inside
    /// the window (the kernel reports it and runs its default).
    DefaultStated(u32),
    /// The payload's own entry, built for a generation inside the window.
    ProvidedInside(u32),
    /// The payload's own entry with no stated generation: refused.
    RefusedUnstated,
    /// A stated generation outside the window: refused, the entry never
    /// runs.
    RefusedOutside(u32),
}

/// Judge the handshake. Total over its inputs; `compat <= abi` is the
/// caller's invariant (asserted where the two constants live).
pub fn judge(provided: bool, stated: u32, abi: u32, compat: u32) -> Verdict {
    let inside = stated >= compat && stated <= abi;
    match (provided, stated, inside) {
        (false, 0, _) => Verdict::DefaultUnstated,
        (true, 0, _) => Verdict::RefusedUnstated,
        (false, n, true) => Verdict::DefaultStated(n),
        (true, n, true) => Verdict::ProvidedInside(n),
        (_, n, false) => Verdict::RefusedOutside(n),
    }
}
