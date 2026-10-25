//! Seat counting: active named users against the license's seat limit.
//!
//! A seat is a named user who started an edit session within the window
//! (30 days). Watching is free. The ledger holds at most `seats` names, so
//! its memory is bounded by the license (R4). It is plain state with the
//! clock passed in, so it is tested without waiting.

use std::collections::HashMap;

use crate::license::DAY;

/// The answer to "may this user take a seat?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// The user has a seat (new or already counted).
    Admitted,
    /// Every seat is taken by other users within the window.
    Full,
}

/// Counts named users within a rolling window.
#[derive(Debug)]
pub struct SeatLedger {
    seats: usize,
    window: u64,
    /// Subject to the last time it used a seat, in seconds.
    last_seen: HashMap<String, u64>,
}

impl SeatLedger {
    /// The usual window: 30 days.
    pub const WINDOW_DAYS: u64 = 30;

    /// A ledger for `seats` users and the usual window.
    #[must_use]
    pub fn new(seats: u32) -> Self {
        Self::with_window(seats, Self::WINDOW_DAYS * DAY)
    }

    /// A ledger with a window in seconds (tests).
    #[must_use]
    pub fn with_window(seats: u32, window_secs: u64) -> Self {
        SeatLedger {
            seats: seats as usize,
            window: window_secs,
            last_seen: HashMap::new(),
        }
    }

    /// Records use by `subject` at `now`. A user already counted always
    /// gets in and has the window renewed. Time O(1), or O(n) when the
    /// ledger is full and expired names are dropped, where n <= seats.
    pub fn admit(&mut self, subject: &str, now: u64) -> Admission {
        if let Some(seen) = self.last_seen.get_mut(subject) {
            // Their slot is still held, even if the window has passed, until
            // someone else needs it (the prune below).
            *seen = now;
            return Admission::Admitted;
        }
        if self.last_seen.len() >= self.seats {
            let window = self.window;
            self.last_seen
                .retain(|_, seen| now.saturating_sub(*seen) < window);
        }
        if self.last_seen.len() >= self.seats {
            return Admission::Full;
        }
        self.last_seen.insert(subject.to_owned(), now);
        Admission::Admitted
    }

    /// Users holding a seat at `now`. Time O(n).
    #[must_use]
    pub fn used(&self, now: u64) -> usize {
        self.last_seen
            .values()
            .filter(|seen| now.saturating_sub(**seen) < self.window)
            .count()
    }

    /// The seat limit.
    #[must_use]
    pub fn seats(&self) -> usize {
        self.seats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_ledger_refuses_new_users_but_not_known_ones() {
        let mut ledger = SeatLedger::with_window(2, 100);
        assert_eq!(ledger.admit("a", 0), Admission::Admitted);
        assert_eq!(ledger.admit("b", 1), Admission::Admitted);
        assert_eq!(ledger.admit("c", 2), Admission::Full);
        assert_eq!(ledger.admit("a", 3), Admission::Admitted);
        assert_eq!(ledger.used(3), 2);
    }

    #[test]
    fn a_seat_frees_after_the_window() {
        let mut ledger = SeatLedger::with_window(1, 100);
        assert_eq!(ledger.admit("a", 0), Admission::Admitted);
        assert_eq!(ledger.admit("b", 99), Admission::Full);
        assert_eq!(ledger.admit("b", 100), Admission::Admitted);
        assert_eq!(ledger.used(100), 1);
        // "a" left; coming back now finds the seat taken.
        assert_eq!(ledger.admit("a", 101), Admission::Full);
    }

    #[test]
    fn renewing_keeps_a_seat_alive() {
        let mut ledger = SeatLedger::with_window(1, 100);
        assert_eq!(ledger.admit("a", 0), Admission::Admitted);
        assert_eq!(ledger.admit("a", 90), Admission::Admitted);
        assert_eq!(ledger.admit("b", 150), Admission::Full);
    }

    #[test]
    fn memory_never_exceeds_the_seat_count() {
        let mut ledger = SeatLedger::with_window(3, 10);
        for i in 0..1000 {
            let _ = ledger.admit(&format!("user{i}"), i);
        }
        assert!(ledger.last_seen.len() <= 3);
    }
}
