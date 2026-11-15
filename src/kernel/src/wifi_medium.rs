//! The wireless medium's pure half: the wifi driver behind the medium
//! seam, generic over the driver's transport and clock, so the host tier
//! drives this same code over the driver's scripted transport and a row
//! binds it to its radio's bus.
//!
//! WHAT IT DOES AT A START. The board hands it what it found in flash: the
//! stored network's record, or why there is none, and the radio's corner.
//! With no valid record, or with one that says to join on request, the
//! radio stays off and not one operation reaches the transport. Otherwise
//! the corner's directory is read and the three images are digested where
//! they lie, one erase unit a service call; images that are not this
//! kernel's pinned bytes leave the radio off as well. Then the driver is
//! taken up its ladder: the attach, the upload, the opening, the radio up,
//! the join. A record that accepts WPA2 or WPA3 asks the firmware first
//! whether it runs the WPA3 exchange; when it does, a scan looks for the
//! stored name, and the join is WPA3 where the network offers it and WPA2
//! otherwise. That scan asks for the name itself when its first pass does
//! not hear it, so a network that does not broadcast its name is heard
//! too. A program's start is never held for any of this.
//!
//! WHAT IT DOES LATER. The driver keeps a joined network joined by itself,
//! and looks for a lost one by its name as well, whatever the record says
//! of reconnection: a network that the board joins at a start is joined
//! again by itself. A network found offering only the other kind, under a
//! record that accepts both and with a secret that the other kind takes,
//! is joined with the other kind and no line is said for it. A network
//! that refuses the stored secret is tried again behind a rest that grows
//! from five seconds to five minutes, for as long as the program runs,
//! with one line at each refusal; the row's seed spreads those rests. A
//! network that offers no kind the record can use is looked for as one
//! that is not seen is, with one line. A stored secret whose length the
//! join's kind does not take ends the trying. A refusal of the driver
//! parks the medium with the link down.
//!
//! WHAT IT DOES AT THE PROGRAM'S END. The network is the program's while it
//! runs. At its end the medium tells the network that it leaves, where it
//! holds one, and waits for the answer no longer than a quarter of a
//! second; then the row lowers the radio's power line, whatever the answer
//! was, and the medium is down for good: nothing reaches the driver or the
//! transport again, the link reads down and the stack finds no room to
//! send. A radio that was never powered is left as it is.
//!
//! THE SEAM. The service call polls the driver until it asks to wait, a
//! frame arrived or the call's budget is spent, and never before the
//! instant the driver named unless the chip's interrupt line asks for it,
//! as the next paragraph has it. A received frame is copied into a hold of
//! this medium's own, where the stack reads it in place; a frame that
//! arrives while the hold is full is dropped and counted. The transmit
//! slot is the driver's own buffer: the commit writes it under the chip's
//! credit window, and a frame the window holds back is written by a later
//! poll.
//!
//! THE CHIP'S INTERRUPT LINE. The driver answers a wait with an instant
//! alone, or with the chip's interrupt line and an instant, whichever
//! comes first. Behind an answer that named the line, a row that reads the
//! line gives the medium its level at the start of a service call, and a
//! high line polls the driver at that call, before the instant; behind an
//! answer that named an instant alone the line is not read. And the row
//! arms the line's interrupt across the thread's wait where the medium
//! says so: where the driver's last answer named the line, except behind a
//! poll that the line itself asked for and that found nothing to read. The
//! instant then stands, so a line that stays high costs one poll more a
//! cadence and no more. The driver's instant bounds every wait, with the
//! line or without.
//!
//! NOTHING OF THE NETWORK IS SAID. What a scan hears is kept as two
//! answers about the stored name and nothing else. No line this module
//! writes carries a name, an address or a secret.

use core::fmt;
use core::sync::atomic::AtomicU32;

use lamella_wifi_cyw4343x::event::status::NO_NETWORKS;
use lamella_wifi_cyw4343x::{Advertised, Clock, Credential, Driver, JoinFailure, LinkState, Micros, Outcome, Passphrase, Refusal, SaePassword, Security, Transport, Wake};

use crate::board::net_medium::{Link, Medium, RxSide, TxSide};
use crate::wifi_images::{self, Check, Pins, Places, Step, UNIT};
use crate::wifi_stored::{Absent, Record, BOOT_BEFORE_PROGRAM, BOOT_ON_REQUEST, KIND_OPEN, KIND_WPA2_OR_WPA3, KIND_WPA3};

/// The largest frame the medium carries: the Ethernet frame without its
/// check sequence.
pub const FRAME_UNIT: usize = 1514;
/// The hold's bytes: one frame, rounded up to a word.
pub const HOLD: usize = 1516;
/// The budget of one service call.
pub const BUDGET_US: Micros = 4_000;
/// The polls of one service call at most, whatever the clock says.
const POLLS: u32 = 512;
/// The wait after a poll that found the driver with nothing to do.
const IDLE_US: Micros = 10_000;
/// The longest wait for the network's answer at the program's end: 25 of
/// the driver's service cadences.
pub const END_BOUND_US: Micros = 250_000;

/// The stage name of a request the driver did not accept; the status is
/// the request's number in the ladder.
pub const STAGE_REQUEST: &str = "medium request";
const REQUEST_ATTACH: u32 = 1;
const REQUEST_UPLOAD: u32 = 2;
const REQUEST_OPEN: u32 = 3;
const REQUEST_CAPABILITIES: u32 = 4;
const REQUEST_UP: u32 = 5;
const REQUEST_SCAN: u32 = 6;
const REQUEST_JOIN: u32 = 7;

/// Where the medium stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// The radio is off and stays off: nothing reaches the transport.
    Off,
    /// The corner's directory is read at the next service call.
    Corner,
    /// The three images are being digested, one erase unit a service call.
    Checking,
    Attaching,
    Uploading,
    Opening,
    /// The firmware is asked whether it runs the WPA3 exchange.
    Asking,
    RadioUp,
    /// A scan looks for the stored name.
    Scanning,
    /// The join is asked; from here the driver keeps the link.
    Joining,
    /// The radio is up and holds no network: it stopped trying.
    Stopped,
    /// The driver refused; it is not polled again.
    Parked,
    /// The program ended: the radio is switched off, and nothing reaches
    /// the driver or the transport again.
    Down,
}

/// How the medium came down, for a row's witness: the network answered the
/// disconnect; it did not answer within the bound, or the driver refused
/// meanwhile; no disconnect was asked, since no network was held; or the
/// row cut the power by itself and told the medium afterwards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End {
    Answered = 1,
    Unanswered = 2,
    NotAsked = 3,
    Cut = 4,
}

/// What the medium has to say, each once, for the console.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Note {
    Starting,
    /// A record set to join before the program starts: it starts as the
    /// others do.
    StartingOlderSetting,
    OnRequest,
    NothingStored,
    UnknownSetting,
    /// The corner does not give this kernel its images, and why.
    FirmwareNotFound(wifi_images::Fault),
    /// The first try of a start failed and the driver keeps trying: the
    /// radio did not see the network, or the try ended another way.
    NotFound,
    FirstTryFailed,
    Joined,
    /// The network did not accept the stored secret, by the handshake's
    /// timeout; the driver tries again in so many seconds.
    SecretRefused { again_s: u32 },
    /// A WPA3 join failed at its authentication, where a secret that does
    /// not match fails and where other things fail as well; the driver
    /// tries again in so many seconds.
    AuthenticationFailed { again_s: u32 },
    /// The stored secret's length is outside the bounds of the kind that
    /// the join needs: no join is asked.
    SecretUnusable,
    /// The network was found offering no kind that the record can use; the
    /// driver keeps looking for it.
    NoAcceptedKind,
    LinkLost,
    Refused(Refusal),
    /// The program ended and the radio's power line was lowered.
    Ended,
}

impl Note {
    /// The note of the first failed try of a start, by what the driver's
    /// failure says: the firmware's own join scan found no network of the
    /// stored name, or the driver's scan did not see it; or anything else
    /// the driver tries again after.
    pub fn first_try(failure: JoinFailure) -> Note {
        match failure {
            JoinFailure::NotFound | JoinFailure::Association { status: NO_NETWORKS, .. } => Note::NotFound,
            _ => Note::FirstTryFailed,
        }
    }

    /// The console's line: the state in a few words, with no instruction and
    /// no name of a tool or of a version in it. No line carries a network's
    /// name, an address or a secret.
    pub fn write_line(&self, out: &mut impl fmt::Write) -> fmt::Result {
        match *self {
            Note::NotFound => out.write_str("[wifi] stored network not found; still looking"),
            Note::FirstTryFailed => out.write_str("[wifi] join did not complete; retrying"),
            Note::Starting => out.write_str("[wifi] starting; joining the stored network"),
            Note::StartingOlderSetting => out.write_str("[wifi] starting; joining the stored network"),
            Note::OnRequest => out.write_str("[wifi] off: stored network is not set to join at start"),
            Note::NothingStored => out.write_str("[wifi] off: no network stored"),
            Note::UnknownSetting => out.write_str("[wifi] off: stored network has an unknown setting"),
            Note::FirmwareNotFound(_) => out.write_str("[wifi] off: radio firmware not found"),
            Note::Joined => out.write_str("[wifi] joined"),
            Note::SecretRefused { again_s } => write!(out, "[wifi] passphrase not accepted; retrying in {} s", again_s),
            Note::AuthenticationFailed { again_s } => write!(out, "[wifi] authentication failed; retrying in {} s", again_s),
            Note::SecretUnusable => out.write_str("[wifi] stopped: stored passphrase has an invalid length"),
            Note::NoAcceptedKind => out.write_str("[wifi] stored network found, security type does not match; still looking"),
            Note::LinkLost => out.write_str("[wifi] link lost; rejoining"),
            Note::Refused(refusal) => write!(
                out,
                "[wifi] off: radio did not start (stage {}, status 0x{:08X})",
                refusal.stage, refusal.status
            ),
            Note::Ended => out.write_str("[wifi] program ended; radio off"),
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_line(f)
    }
}

/// The medium's counters.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Counts {
    /// The driver's polls.
    pub polls: u32,
    /// The joins that completed and the links that were lost.
    pub joined: u32,
    pub lost: u32,
    /// The join attempts that failed with the driver trying again.
    pub retried: u32,
    /// The received frames the hold could not keep.
    pub dropped: u32,
    /// The notes that found no room.
    pub notes_lost: u32,
    /// The longest service call on the medium's clock.
    pub service_max_us: u32,
    /// The service calls' time on the medium's clock, summed; it wraps.
    pub service_us: u32,
    /// The last failed attempt: its class and its status words.
    pub failed: [u32; 4],
}

/// A failed attempt as four words for a row's witness: the class -- 1 an
/// association failure, 2 the supplicant, 3 a loss during the attempt, 4 no
/// completing event, 5 the network not seen, 6 a mismatch -- and the status
/// words of the driver's report for that class. They are codes of the
/// protocol; what a mismatched network advertised is not among them.
pub fn failed_words(failure: JoinFailure) -> [u32; 4] {
    match failure {
        JoinFailure::Association { status, assoc_status, auth_status } => [1, status, assoc_status, auth_status],
        JoinFailure::Supplicant { reason } => [2, reason, 0, 0],
        JoinFailure::Lost { number, reason } => [3, number, reason, 0],
        JoinFailure::Timeout => [4, 0, 0, 0],
        JoinFailure::NotFound => [5, 0, 0, 0],
        JoinFailure::Mismatch { .. } => [6, 0, 0, 0],
    }
}

/// The receive side: one frame held for the stack.
pub struct Hold {
    len: usize,
    dropped: u32,
    buf: [u8; HOLD],
}

impl Hold {
    pub const fn new() -> Self {
        Hold { len: 0, dropped: 0, buf: [0; HOLD] }
    }

    /// A received frame offered: kept when the hold is empty and the frame
    /// fits, dropped and counted otherwise.
    pub fn offer(&mut self, frame: &[u8]) -> bool {
        if self.len != 0 || frame.is_empty() || frame.len() > HOLD {
            self.dropped = self.dropped.wrapping_add(1);
            return false;
        }
        self.buf[..frame.len()].copy_from_slice(frame);
        self.len = frame.len();
        true
    }

    pub fn is_full(&self) -> bool {
        self.len != 0
    }

    /// The frames dropped.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// A held frame given up: the link it came over is gone.
    fn clear(&mut self) {
        if self.len != 0 {
            self.len = 0;
            self.dropped = self.dropped.wrapping_add(1);
        }
    }
}

impl RxSide for Hold {
    fn peek(&mut self) -> Option<&mut [u8]> {
        if self.len == 0 {
            return None;
        }
        Some(&mut self.buf[..self.len])
    }

    fn consume(&mut self) {
        self.len = 0;
    }
}

/// The transmit side: the driver and its bus.
pub struct Radio<'b, T: Transport> {
    driver: Driver<'b>,
    bus: T,
    /// The medium is down: the link reads down and nothing is sent,
    /// whatever the driver still says.
    down: bool,
}

impl<T: Transport> Radio<'_, T> {
    fn link_up(&self) -> bool {
        !self.down && self.driver.is_ready() && self.driver.link() == LinkState::Up
    }
}

impl<T: Transport> TxSide for Radio<'_, T> {
    fn room(&self) -> bool {
        self.link_up() && !self.driver.sending()
    }

    fn slot(&mut self, len: usize) -> Option<&mut [u8]> {
        if self.down {
            return None;
        }
        self.driver.stage(len)
    }

    fn commit(&mut self, _len: usize) {
        if self.down {
            return;
        }
        // Written now when the chip takes it, by a later poll otherwise. A
        // refusal of the bus parks the driver; the next service call reads
        // it there.
        let _ = self.driver.flush(&mut self.bus);
    }
}

const NOTES: usize = 4;

/// The wireless medium.
pub struct Wifi<'b, T: Transport, C: Clock> {
    hold: Hold,
    radio: Radio<'b, T>,
    clock: C,
    wake: &'b AtomicU32,
    room: &'b [u8],
    pins: &'b Pins,
    record: Option<Record<'b>>,
    check: Option<Check>,
    places: Option<Places>,
    stage: Stage,
    /// The instant before which the driver is not polled again.
    next: Micros,
    /// The last service call handed the stack a frame.
    more: bool,
    /// The driver's last answer named the chip's interrupt line: the row
    /// arms the line's interrupt across the thread's wait.
    irq: bool,
    /// The row read the line high: this service call polls the driver once
    /// before the instant it named.
    early: bool,
    /// The firmware runs the WPA3 exchange; the scan saw the stored name;
    /// the name was seen offering WPA3; the join was last asked as WPA3.
    sae: bool,
    seen: bool,
    offered: bool,
    wpa3: bool,
    /// The line of a network that offers no kind the record can use was
    /// said since the last join: it is one line for the change of state,
    /// and none for a later round of the looking.
    kind_said: bool,
    notes: [Option<Note>; NOTES],
    counts: Counts,
}

impl<'b, T: Transport, C: Clock> Wifi<'b, T, C> {
    /// The medium over `bus` and `clock`. `stored` is the stored network's
    /// current record or why there is none; `room` is the radio's corner
    /// as it lies in flash, without the stored network's two units.
    pub fn new(bus: T, clock: C, wake: &'b AtomicU32, stored: Result<Record<'b>, Absent>, room: &'b [u8], pins: &'b Pins) -> Self {
        // A record that says to join before the program starts is taken as
        // one that says to join while it runs: no start is held.
        let (record, stage, note) = match stored {
            Err(Absent::Nothing) => (None, Stage::Off, Some(Note::NothingStored)),
            Err(Absent::Unknown) => (None, Stage::Off, Some(Note::UnknownSetting)),
            Ok(record) if record.boot == BOOT_ON_REQUEST => (None, Stage::Off, Some(Note::OnRequest)),
            Ok(record) => (Some(record), Stage::Corner, None),
        };
        Wifi {
            hold: Hold::new(),
            radio: Radio { driver: Driver::new(), bus, down: false },
            clock,
            wake,
            room,
            pins,
            record,
            check: None,
            places: None,
            stage,
            next: 0,
            more: false,
            irq: false,
            early: false,
            sae: false,
            seen: false,
            offered: false,
            wpa3: false,
            kind_said: false,
            notes: [note, None, None, None],
            counts: Counts { polls: 0, joined: 0, lost: 0, retried: 0, dropped: 0, notes_lost: 0, service_max_us: 0, service_us: 0, failed: [0; 4] },
        }
    }

    /// The seed of the spread on the driver's rests behind refused
    /// secrets, a word of the row's random source: handed to the driver,
    /// and nothing of it kept here. The row gives it behind the attach
    /// request, at the service call in which the radio starts; a medium
    /// that was given none rests without a spread.
    pub fn seed(&mut self, seed: u32) {
        self.radio.driver.seed(seed);
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// The counters, the hold's drops among them.
    pub fn counts(&self) -> Counts {
        Counts { dropped: self.hold.dropped, ..self.counts }
    }

    /// The oldest note not yet taken.
    pub fn take_note(&mut self) -> Option<Note> {
        let note = self.notes[0].take();
        self.notes.rotate_left(1);
        note
    }

    /// Whether the last service call handed the stack a frame, so that
    /// another call right after the stack's poll may find the next.
    pub fn more(&self) -> bool {
        self.more
    }

    /// How long the medium can wait for its next service call: nothing
    /// when it has no deadline of its own, zero when work is ready.
    pub fn wait_us(&mut self) -> Option<Micros> {
        match self.stage {
            Stage::Off | Stage::Parked | Stage::Down => None,
            _ => Some(self.next.saturating_sub(self.clock.now_us())),
        }
    }

    /// The driver, for a row's witness.
    pub fn driver(&self) -> &Driver<'b> {
        &self.radio.driver
    }

    /// The bus, for a row's witness.
    pub fn bus(&self) -> &T {
        &self.radio.bus
    }

    /// Whether the driver is polled: the radio is powered, and the medium
    /// is neither parked nor down.
    fn polled(&self) -> bool {
        !matches!(self.stage, Stage::Off | Stage::Corner | Stage::Checking | Stage::Parked | Stage::Down)
    }

    /// The chip's interrupt line at the start of a service call, on a row
    /// that reads it. Where the driver is polled and its last answer named
    /// the line, `sample` is called with the bus, once: the row disarms
    /// the line's interrupt in it and answers the line's level, and a high
    /// line polls the driver at this service call, before the instant it
    /// named. Behind an answer that named an instant alone the line is not
    /// read, and the wait is never cut short. Answers the level, and
    /// nothing where the line was not read.
    pub fn sample(&mut self, sample: impl FnOnce(&mut T) -> bool) -> Option<bool> {
        if !self.irq || !self.polled() {
            return None;
        }
        let high = sample(&mut self.radio.bus);
        if high {
            self.next = 0;
            self.early = true;
        }
        Some(high)
    }

    /// The end of a pass, on a row that arms the chip's interrupt line
    /// across the thread's wait. `arm` is called with the bus where the
    /// driver is polled and its last answer named the line -- but not
    /// behind a poll that a high line asked for and that found nothing to
    /// read: the driver's instant stands then, and the line is armed again
    /// behind the poll at that instant. Answers whether `arm` was called.
    /// The line is armed here and nowhere else, so a service call finds it
    /// armed only where `sample` disarms it.
    pub fn arm(&mut self, arm: impl FnOnce(&mut T)) -> bool {
        if !self.irq || !self.polled() {
            return false;
        }
        arm(&mut self.radio.bus);
        true
    }

    /// The program's end. Where the radio had been powered -- the stage is
    /// beyond the corner's check -- the driver's disconnect is asked. The
    /// driver accepts it when it is ready and holds a network, and is then
    /// polled here, at the instants it names, until the disconnect's
    /// outcome, a refusal or `END_BOUND_US`; a frame that arrives
    /// meanwhile is not kept. Then `lower` is called with the bus, once,
    /// whatever the disconnect answered: the row lowers the radio's power
    /// line in it. Where the radio was never powered nothing is asked,
    /// `lower` is not called and no line is said. In every case the medium
    /// is down from here on. Answers how it came down, and nothing where
    /// it was down already.
    pub fn end(&mut self, lower: impl FnOnce(&mut T)) -> Option<End> {
        if self.stage == Stage::Down {
            return None;
        }
        let mut end = End::NotAsked;
        if !matches!(self.stage, Stage::Off | Stage::Corner | Stage::Checking) {
            if self.radio.driver.disconnect() {
                end = End::Unanswered;
                let bound = self.clock.now_us() + END_BOUND_US;
                loop {
                    let until = match self.radio.driver.poll(&mut self.radio.bus, &mut self.clock) {
                        Wake::Done(Outcome::Disconnected) => {
                            end = End::Answered;
                            break;
                        }
                        // A refusal, or a driver with nothing left to do:
                        // no answer comes.
                        Wake::Done(Outcome::Refused(_)) | Wake::Idle => break,
                        Wake::Done(_) | Wake::Again => 0,
                        Wake::At(instant) | Wake::Irq(instant) => instant,
                    };
                    let now = self.clock.now_us();
                    if now >= bound {
                        break;
                    }
                    if until > now {
                        self.clock.delay_us((until.min(bound) - now) as u32);
                    }
                }
            }
            lower(&mut self.radio.bus);
            self.note(Note::Ended);
        }
        self.radio.down = true;
        self.stage = Stage::Down;
        Some(end)
    }

    /// The medium down with nothing asked of the driver or of the bus: the
    /// row lowered the power line by itself, where the end above could not
    /// run, and tells the medium at its next service call.
    pub fn cut(&mut self) {
        self.radio.down = true;
        self.stage = Stage::Down;
    }

    fn note(&mut self, note: Note) {
        match self.notes.iter_mut().find(|slot| slot.is_none()) {
            Some(slot) => *slot = Some(note),
            None => self.counts.notes_lost = self.counts.notes_lost.wrapping_add(1),
        }
    }

    fn off(&mut self, note: Note) {
        self.stage = Stage::Off;
        self.note(note);
    }

    fn park(&mut self, refusal: Refusal) {
        if self.stage != Stage::Parked {
            self.stage = Stage::Parked;
            self.hold.clear();
            self.note(Note::Refused(refusal));
        }
    }

    /// A request the driver did not accept: the medium's own refusal.
    fn asked(&mut self, accepted: bool, request: u32, then: Stage) {
        if accepted {
            self.stage = then;
        } else {
            self.park(Refusal::new(STAGE_REQUEST, request));
        }
    }

    fn read_corner(&mut self) {
        match self.room.get(..UNIT).ok_or(wifi_images::Fault::Unit).and_then(|unit| wifi_images::places(unit, self.pins)) {
            Ok(places) => {
                self.places = Some(places);
                self.check = Some(Check::new(places));
                self.stage = Stage::Checking;
            }
            Err(fault) => self.off(Note::FirmwareNotFound(fault)),
        }
    }

    fn check_step(&mut self) {
        let Some(check) = self.check.as_mut() else {
            self.off(Note::FirmwareNotFound(wifi_images::Fault::Unit));
            return;
        };
        match check.step(self.room, self.pins, UNIT) {
            Step::More => {}
            Step::Bad(image) => {
                self.check = None;
                self.off(Note::FirmwareNotFound(wifi_images::Fault::Content(image)));
            }
            Step::Good => {
                self.check = None;
                let older = self.record.is_some_and(|record| record.boot == BOOT_BEFORE_PROGRAM);
                self.note(if older { Note::StartingOlderSetting } else { Note::Starting });
                let accepted = self.radio.driver.attach();
                self.asked(accepted, REQUEST_ATTACH, Stage::Attaching);
            }
        }
    }

    fn image(&self, place: (usize, usize)) -> &'b [u8] {
        let room: &'b [u8] = self.room;
        &room[place.0..place.0 + place.1]
    }

    /// The join asked of the driver, as WPA3 or as the record's other kind.
    fn join(&mut self, wpa3: bool) {
        let Some(record) = self.record else {
            self.park(Refusal::new(STAGE_REQUEST, REQUEST_JOIN));
            return;
        };
        self.wpa3 = wpa3;
        let credential = if record.kinds == KIND_OPEN {
            Credential::Open
        } else if wpa3 {
            if SaePassword::new(record.secret).is_none() {
                return self.stop(Note::SecretUnusable);
            }
            Credential::SaePassword(record.secret)
        } else {
            if Passphrase::new(record.secret).is_none() {
                return self.stop(Note::SecretUnusable);
            }
            Credential::Passphrase(record.secret)
        };
        let accepted = self.radio.driver.join_with(record.name, credential);
        self.asked(accepted, REQUEST_JOIN, Stage::Joining);
    }

    /// Whether a network that the driver gave up, found offering
    /// `advertised` and no suite the last join's kind can use, is joined
    /// with the record's other kind: the record accepts both kinds, and
    /// the network offers the other one -- WPA2 where WPA3 was asked, or
    /// WPA3 where WPA2 was asked on a firmware that runs the WPA3 exchange
    /// -- and the stored secret's length is one that the other kind takes.
    fn other_kind(&self, advertised: Advertised) -> bool {
        let Some(record) = self.record else {
            return false;
        };
        if record.kinds != KIND_WPA2_OR_WPA3 {
            return false;
        }
        if self.wpa3 {
            advertised.compatible(Security::Wpa2Psk) && Passphrase::new(record.secret).is_some()
        } else {
            self.sae && advertised.compatible(Security::Wpa3Sae) && SaePassword::new(record.secret).is_some()
        }
    }

    /// The radio stays up and holds no network.
    fn stop(&mut self, note: Note) {
        self.stage = Stage::Stopped;
        self.note(note);
    }

    /// One outcome of the driver; answers whether the service call ends.
    fn outcome(&mut self, outcome: Outcome) -> bool {
        match (self.stage, outcome) {
            (_, Outcome::Refused(refusal)) => {
                self.park(refusal);
                return true;
            }
            (_, Outcome::Frame { .. }) => {
                self.hold.offer(self.radio.driver.frame_payload());
                self.more = true;
                return true;
            }
            (Stage::Attaching, Outcome::Attached { .. }) => {
                let Some(places) = self.places else {
                    self.park(Refusal::new(STAGE_REQUEST, REQUEST_UPLOAD));
                    return true;
                };
                let (firmware, settings) = (self.image(places.firmware), self.image(places.settings));
                let accepted = self.radio.driver.upload(firmware, settings);
                self.asked(accepted, REQUEST_UPLOAD, Stage::Uploading);
            }
            (Stage::Uploading, Outcome::Uploaded { .. }) => {
                let Some(places) = self.places else {
                    self.park(Refusal::new(STAGE_REQUEST, REQUEST_OPEN));
                    return true;
                };
                let regulatory = self.image(places.regulatory);
                let version: &'b [u8] = self.pins.version;
                let accepted = self.radio.driver.open(regulatory, Some(version));
                self.asked(accepted, REQUEST_OPEN, Stage::Opening);
            }
            (Stage::Opening, Outcome::Ready { .. }) => {
                if self.record.is_some_and(|record| record.kinds == KIND_WPA2_OR_WPA3) {
                    let accepted = self.radio.driver.capabilities();
                    self.asked(accepted, REQUEST_CAPABILITIES, Stage::Asking);
                } else {
                    let accepted = self.radio.driver.up();
                    self.asked(accepted, REQUEST_UP, Stage::RadioUp);
                }
            }
            (Stage::Asking, Outcome::Capabilities(word)) => {
                self.sae = word.answered && word.sae;
                let accepted = self.radio.driver.up();
                self.asked(accepted, REQUEST_UP, Stage::RadioUp);
            }
            (Stage::RadioUp, Outcome::Up { .. }) => match self.record {
                Some(record) if record.kinds == KIND_WPA2_OR_WPA3 && self.sae => {
                    self.seen = false;
                    self.offered = false;
                    // The scan asks for the stored name when its first pass
                    // does not hear it: a network that does not broadcast
                    // its name answers that question alone.
                    let accepted = self.radio.driver.scan_for(record.name);
                    self.asked(accepted, REQUEST_SCAN, Stage::Scanning);
                }
                Some(record) => self.join(record.kinds == KIND_WPA3),
                None => self.join(false),
            },
            (Stage::Scanning, Outcome::ScanRecord(heard)) => {
                // Of what the scan hears, two answers about the stored name
                // are kept and nothing else.
                if self.record.is_some_and(|record| record.name == heard.ssid()) {
                    self.seen = true;
                    self.offered |= heard.security.sae;
                }
            }
            (Stage::Scanning, Outcome::ScanDone { .. }) => self.join(self.seen && self.offered),
            (Stage::Joining, Outcome::Joined { .. }) => {
                self.counts.joined = self.counts.joined.wrapping_add(1);
                self.kind_said = false;
                self.note(Note::Joined);
            }
            (Stage::Joining, Outcome::JoinFailed(failure)) => {
                self.counts.failed = failed_words(failure);
                // The driver tries again by itself, except where it gave
                // the network up: a network that offers no suite the
                // credential can use.
                if self.radio.driver.link() == LinkState::Detached {
                    match failure {
                        // The second choice: no line is said for it. The
                        // loss was said when it happened, and the join
                        // says its own line when it completes.
                        JoinFailure::Mismatch { advertised } if self.other_kind(advertised) => self.join(!self.wpa3),
                        // No kind that the record can use: the same join
                        // is asked again, and the driver looks for the
                        // network as it does for one it does not see. One
                        // line for the change of state.
                        _ => {
                            if !self.kind_said {
                                self.kind_said = true;
                                self.note(Note::NoAcceptedKind);
                            }
                            self.join(self.wpa3);
                        }
                    }
                } else if let Some(rest) = self.radio.driver.backoff() {
                    // The sign of a refused secret: one line at each, with
                    // when the next try comes -- the driver's rest with its
                    // spread, in whole seconds, rounded up.
                    let again_s = rest.div_ceil(1_000_000).min(u32::MAX as Micros) as u32;
                    self.note(if matches!(failure, JoinFailure::Supplicant { .. }) { Note::SecretRefused { again_s } } else { Note::AuthenticationFailed { again_s } });
                    self.counts.retried = self.counts.retried.wrapping_add(1);
                } else {
                    // One line for a change of state, never one for a try:
                    // the first failed try of a start says why there is no
                    // link yet. After a join the loss's line has said it.
                    if self.counts.retried == 0 && self.counts.joined == 0 {
                        self.note(Note::first_try(failure));
                    }
                    self.counts.retried = self.counts.retried.wrapping_add(1);
                }
            }
            (Stage::Joining, Outcome::LinkLost(_)) => {
                // A network that the board joined at a start is looked for
                // again by the driver, whatever the record says of
                // reconnection.
                self.counts.lost = self.counts.lost.wrapping_add(1);
                self.hold.clear();
                self.note(Note::LinkLost);
            }
            // Every other outcome is the driver's own business: an event it
            // hands on, a record of its own recovery scan, the end of a
            // disconnect.
            _ => {}
        }
        matches!(self.stage, Stage::Off | Stage::Parked | Stage::Down)
    }
}

impl<T: Transport, C: Clock> Medium for Wifi<'_, T, C> {
    /// The chip's own address, known once the radio is up.
    fn hardware_address(&self) -> Option<[u8; 6]> {
        self.radio.driver.address()
    }

    fn link(&self) -> Link {
        if self.radio.link_up() { Link::Up } else { Link::Down }
    }

    fn wake_word(&self) -> &AtomicU32 {
        self.wake
    }

    fn service(&mut self, _now_ms: u32) {
        self.more = false;
        // A line read high counts for this call's first poll and no other.
        let mut early = core::mem::take(&mut self.early);
        let began = self.clock.now_us();
        match self.stage {
            Stage::Off | Stage::Parked | Stage::Down => return,
            Stage::Corner => return self.read_corner(),
            Stage::Checking => return self.check_step(),
            _ => {}
        }
        let mut now = began;
        for _ in 0..POLLS {
            // A refusal behind a commit parks the driver with no outcome.
            if let Some(refusal) = self.radio.driver.refusal() {
                self.park(refusal);
                break;
            }
            // Not before the instant the driver named; and while the stack
            // can take the held frame, it takes it first.
            if now < self.next || (self.hold.is_full() && self.radio.room()) {
                break;
            }
            self.counts.polls = self.counts.polls.wrapping_add(1);
            let wake = self.radio.driver.poll(&mut self.radio.bus, &mut self.clock);
            // The driver waits for the chip's line where its answer names
            // it -- but not behind a poll that the line asked for and that
            // read nothing: the line is not asked again before the instant.
            self.irq = matches!(wake, Wake::Irq(_)) && !early;
            early = false;
            let ends = match wake {
                Wake::Again => {
                    self.next = 0;
                    false
                }
                Wake::At(instant) | Wake::Irq(instant) => {
                    self.next = instant;
                    false
                }
                Wake::Idle => {
                    self.next = now + IDLE_US;
                    false
                }
                Wake::Done(outcome) => {
                    self.next = 0;
                    self.outcome(outcome)
                }
            };
            now = self.clock.now_us();
            if ends || now.saturating_sub(began) >= BUDGET_US {
                break;
            }
        }
        let spent = now.saturating_sub(began).min(u32::MAX as Micros) as u32;
        self.counts.service_max_us = self.counts.service_max_us.max(spent);
        self.counts.service_us = self.counts.service_us.wrapping_add(spent);
    }

    fn split(&mut self) -> (&mut dyn RxSide, &mut dyn TxSide) {
        (&mut self.hold, &mut self.radio)
    }

    fn frame_unit(&self) -> usize {
        FRAME_UNIT
    }
}
