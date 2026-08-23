//! Bounded automation capture and MIDI publication helpers.

use crate::sequencer::{AutomationPoint, PageTarget, MAX_AUTOMATION_POINTS_PER_LANE};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub const GLOBAL_CC_RATE: u32 = 250;
pub const DESTINATION_CC_RATE: u32 = 100;
const MAX_PENDING_CC: usize = 2_048;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CcKey {
    target: PageTarget,
    channel: u8,
    controller: u8,
}

#[derive(Debug)]
pub struct CcPublisher {
    next_global: Instant,
    next_destination: BTreeMap<PageTarget, Instant>,
    sent: BTreeMap<CcKey, u8>,
    pending: BTreeMap<CcKey, u8>,
}

impl CcPublisher {
    pub fn new(now: Instant) -> Self {
        Self {
            next_global: now,
            next_destination: BTreeMap::new(),
            sent: BTreeMap::new(),
            pending: BTreeMap::new(),
        }
    }

    pub fn clear(&mut self, now: Instant) {
        self.next_global = now;
        self.next_destination.clear();
        self.sent.clear();
        self.pending.clear();
    }

    pub fn offer(&mut self, target: &PageTarget, bytes: &[u8], now: Instant) -> Option<Vec<u8>> {
        let [status, controller, value, ..] = bytes else {
            return Some(bytes.to_vec());
        };
        if status & 0xf0 != 0xb0 {
            return Some(bytes.to_vec());
        }
        let key = CcKey {
            target: target.clone(),
            channel: status & 0x0f,
            controller: *controller,
        };
        if self.pending.get(&key) == Some(value) {
            return None;
        }
        if self.sent.get(&key) == Some(value) {
            // The destination has returned to the last published value before
            // a throttled intermediate value was flushed.  That intermediate
            // value is now stale and must not be emitted later.
            self.pending.remove(&key);
            return None;
        }
        if self.ready(target, now) {
            self.record_sent(key, *value, now);
            return Some(vec![*status, *controller, *value]);
        }
        if self.pending.len() < MAX_PENDING_CC || self.pending.contains_key(&key) {
            self.pending.insert(key, *value);
        }
        None
    }

    pub fn flush(&mut self, now: Instant) -> Option<(PageTarget, Vec<u8>)> {
        let key = self
            .pending
            .keys()
            .find(|key| self.ready(&key.target, now))?
            .clone();
        let value = self.pending.remove(&key)?;
        let bytes = vec![0xb0 | key.channel, key.controller, value];
        let target = key.target.clone();
        self.record_sent(key, value, now);
        Some((target, bytes))
    }

    fn ready(&self, target: &PageTarget, now: Instant) -> bool {
        now >= self.next_global
            && self
                .next_destination
                .get(target)
                .is_none_or(|deadline| now >= *deadline)
    }

    fn record_sent(&mut self, key: CcKey, value: u8, now: Instant) {
        self.next_global = now + Duration::from_secs_f64(1.0 / f64::from(GLOBAL_CC_RATE));
        self.next_destination.insert(
            key.target.clone(),
            now + Duration::from_secs_f64(1.0 / f64::from(DESTINATION_CC_RATE)),
        );
        self.sent.insert(key, value);
    }
}

/// Append or replace a captured point, then remove the middle of nearly
/// collinear triples. The tolerance is in the stored 16-bit normalized domain.
pub fn capture_thinned(points: &mut Vec<AutomationPoint>, point: AutomationPoint, tolerance: u16) {
    match points.binary_search_by_key(&point.tick, |candidate| candidate.tick) {
        Ok(index) => points[index] = point,
        Err(index) if points.len() < MAX_AUTOMATION_POINTS_PER_LANE => points.insert(index, point),
        Err(_) => return,
    }
    if points.len() < 3 {
        return;
    }
    let mut index = 1;
    while index + 1 < points.len() {
        let a = points[index - 1];
        let b = points[index];
        let c = points[index + 1];
        let span = u64::from(c.tick.saturating_sub(a.tick));
        if span == 0 {
            index += 1;
            continue;
        }
        let offset = u64::from(b.tick.saturating_sub(a.tick));
        let expected = i64::from(a.value)
            + (i64::from(c.value) - i64::from(a.value)) * offset as i64 / span as i64;
        if (i64::from(b.value) - expected).unsigned_abs() <= u64::from(tolerance) {
            points.remove(index);
        } else {
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_thins_redundant_linear_events() {
        let mut points = Vec::new();
        capture_thinned(&mut points, AutomationPoint { tick: 0, value: 0 }, 2);
        capture_thinned(
            &mut points,
            AutomationPoint {
                tick: 10,
                value: 1_000,
            },
            2,
        );
        capture_thinned(
            &mut points,
            AutomationPoint {
                tick: 20,
                value: 2_000,
            },
            2,
        );
        assert_eq!(
            points,
            vec![
                AutomationPoint { tick: 0, value: 0 },
                AutomationPoint {
                    tick: 20,
                    value: 2_000
                }
            ]
        );
    }

    #[test]
    fn publisher_coalesces_and_bounds_each_destination() {
        let now = Instant::now();
        let target = PageTarget::ConfiguredExternal;
        let mut publisher = CcPublisher::new(now);
        assert_eq!(
            publisher.offer(&target, &[0xb0, 74, 1], now),
            Some(vec![0xb0, 74, 1])
        );
        assert_eq!(publisher.offer(&target, &[0xb0, 74, 1], now), None);
        assert_eq!(publisher.offer(&target, &[0xb0, 74, 2], now), None);
        assert!(publisher.flush(now + Duration::from_millis(9)).is_none());
        assert_eq!(
            publisher.flush(now + Duration::from_millis(10)),
            Some((target, vec![0xb0, 74, 2]))
        );
    }

    #[test]
    fn publisher_enforces_the_global_rate_across_destinations() {
        let now = Instant::now();
        let first = PageTarget::Midi("DIN A".into());
        let second = PageTarget::Midi("DIN B".into());
        let mut publisher = CcPublisher::new(now);
        assert!(publisher.offer(&first, &[0xb0, 1, 1], now).is_some());
        assert!(publisher.offer(&second, &[0xb0, 1, 1], now).is_none());
        assert!(publisher.flush(now + Duration::from_millis(3)).is_none());
        assert!(publisher.flush(now + Duration::from_millis(4)).is_some());
    }

    #[test]
    fn publisher_cancels_a_superseded_pending_value() {
        let now = Instant::now();
        let target = PageTarget::ConfiguredExternal;
        let mut publisher = CcPublisher::new(now);
        assert_eq!(
            publisher.offer(&target, &[0xb0, 74, 10], now),
            Some(vec![0xb0, 74, 10])
        );
        assert!(publisher.offer(&target, &[0xb0, 74, 20], now).is_none());
        assert!(publisher.offer(&target, &[0xb0, 74, 10], now).is_none());
        assert!(publisher.flush(now + Duration::from_millis(10)).is_none());
    }
}
