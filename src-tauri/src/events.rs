use crate::model::{EventMetric, EventProcess, EventStatus, LoadEvent, ProcessSnapshot};
use std::time::{Duration, Instant};

const MAX_EVENTS: usize = 20;
const MAX_GAP: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
struct Sample<'a> {
    at: u64,
    now: Instant,
    value: Option<f64>,
    processes: Option<&'a ProcessSnapshot>,
    interrupted: bool,
}

#[derive(Default)]
struct Watch {
    high_since: Option<(Instant, u64)>,
    peak: f64,
    active: bool,
    event_id: Option<u64>,
    recovery_since: Option<Instant>,
    last_sample: Option<Instant>,
}

impl Watch {
    fn interrupt(&mut self, events: &mut [LoadEvent]) {
        if let Some(event) = events
            .iter_mut()
            .find(|event| Some(event.id) == self.event_id)
        {
            event.status = EventStatus::Interrupted;
        }
        *self = Self::default();
    }

    fn observe(
        &mut self,
        events: &mut Vec<LoadEvent>,
        next_id: &mut u64,
        metric: EventMetric,
        sample: Sample<'_>,
    ) {
        let Sample {
            at,
            now,
            value,
            processes,
            interrupted,
        } = sample;
        if interrupted
            || self
                .last_sample
                .is_some_and(|last| now.saturating_duration_since(last) > MAX_GAP)
        {
            self.interrupt(events);
        }
        let Some(value) = value.filter(|v| v.is_finite()) else {
            self.interrupt(events);
            return;
        };
        self.last_sample = Some(now);
        if self.active {
            self.peak = self.peak.max(value);
            let recovered = if value < 80.0 {
                let since = *self.recovery_since.get_or_insert(now);
                now.saturating_duration_since(since) >= Duration::from_secs(60)
            } else {
                self.recovery_since = None;
                false
            };
            if let Some(event) = events
                .iter_mut()
                .find(|event| Some(event.id) == self.event_id)
            {
                event.last_at_ms = at;
                event.duration_secs = now
                    .saturating_duration_since(self.high_since.unwrap().0)
                    .as_secs();
                event.peak_percent = self.peak;
                if recovered {
                    event.status = EventStatus::Recovered;
                }
            }
            if recovered {
                *self = Self::default();
            }
            return;
        }
        if value < 90.0 {
            self.high_since = None;
            self.peak = 0.0;
            return;
        }
        let (start, started_at_ms) = *self.high_since.get_or_insert((now, at));
        self.peak = self.peak.max(value);
        if now.saturating_duration_since(start) < Duration::from_secs(30) {
            return;
        }
        let rows = processes.map(|p| match metric {
            EventMetric::Cpu => &p.top_cpu,
            EventMetric::Memory => &p.top_memory,
        });
        let processes = rows
            .into_iter()
            .flatten()
            .take(3)
            .map(|p| EventProcess {
                name: p.name.clone(),
                cpu_usage_percent: p.cpu_usage_percent,
                resident_memory_bytes: p.resident_memory_bytes,
            })
            .collect();
        *next_id += 1;
        let id = *next_id;
        events.push(LoadEvent {
            id,
            metric,
            status: EventStatus::Active,
            started_at_ms,
            confirmed_at_ms: at,
            last_at_ms: at,
            duration_secs: now.saturating_duration_since(start).as_secs(),
            peak_percent: self.peak,
            processes,
        });
        while events.len() > MAX_EVENTS {
            if let Some(index) = events
                .iter()
                .position(|event| event.status != EventStatus::Active)
            {
                events.remove(index);
            } else {
                break;
            }
        }
        self.active = true;
        self.event_id = Some(id);
    }
}

#[derive(Default)]
pub struct EventTracker {
    cpu: Watch,
    memory: Watch,
    next_id: u64,
}

impl EventTracker {
    pub fn observe(
        &mut self,
        events: &mut Vec<LoadEvent>,
        at: u64,
        now: Instant,
        values: [Option<f64>; 2],
        processes: Option<&ProcessSnapshot>,
        interrupted: bool,
    ) {
        let sample = Sample {
            at,
            now,
            value: values[0],
            processes,
            interrupted,
        };
        self.cpu
            .observe(events, &mut self.next_id, EventMetric::Cpu, sample);
        self.memory.observe(
            events,
            &mut self.next_id,
            EventMetric::Memory,
            Sample {
                value: values[1],
                ..sample
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn tracks_episodes_independently_and_interrupts_gaps() {
        let mut tracker = EventTracker::default();
        let mut events = vec![];
        let start = Instant::now();
        for s in 0..=30 {
            tracker.observe(
                &mut events,
                s * 1000,
                start + Duration::from_secs(s),
                [Some(95.0), Some(92.0)],
                None,
                false,
            );
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].status, EventStatus::Active);
        assert_eq!(events[0].duration_secs, 30);
        tracker.observe(
            &mut events,
            45000,
            start + Duration::from_secs(45),
            [Some(99.0), None],
            None,
            false,
        );
        assert!(events
            .iter()
            .all(|event| event.status == EventStatus::Interrupted));
        assert_eq!(events[0].duration_secs, 30);
    }

    #[test]
    fn recovery_and_clear_do_not_repeat_the_same_episode() {
        let mut tracker = EventTracker::default();
        let mut events = vec![];
        let start = Instant::now();
        for s in 0..=30 {
            tracker.observe(
                &mut events,
                s * 1000,
                start + Duration::from_secs(s),
                [Some(90.0), None],
                None,
                false,
            );
        }
        events.clear();
        for s in 31..=40 {
            tracker.observe(
                &mut events,
                s * 1000,
                start + Duration::from_secs(s),
                [Some(99.0), None],
                None,
                false,
            );
        }
        assert!(events.is_empty());
        for s in 41..=101 {
            tracker.observe(
                &mut events,
                s * 1000,
                start + Duration::from_secs(s),
                [Some(79.0), None],
                None,
                false,
            );
        }
        for s in 102..=132 {
            tracker.observe(
                &mut events,
                s * 1000,
                start + Duration::from_secs(s),
                [Some(90.0), None],
                None,
                false,
            );
        }
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn retains_bounded_events_and_only_trigger_snapshot_processes() {
        use crate::model::ProcessInfo;
        let processes = ProcessSnapshot {
            sampled_at_ms: 0,
            top_cpu: (0..8)
                .map(|pid| ProcessInfo {
                    pid,
                    name: format!("worker-{pid}"),
                    cpu_usage_percent: Some(10.0),
                    resident_memory_bytes: 42,
                })
                .collect(),
            top_memory: vec![],
        };
        let mut tracker = EventTracker::default();
        let mut events = vec![];
        let start = Instant::now();
        let mut s = 0;
        for _ in 0..21 {
            for step in 0..=30 {
                tracker.observe(
                    &mut events,
                    s * 1000,
                    start + Duration::from_secs(s),
                    [Some(if step == 5 { 99.0 } else { 90.0 }), None],
                    Some(&processes),
                    false,
                );
                s += 1;
            }
            for _ in 0..=60 {
                tracker.observe(
                    &mut events,
                    s * 1000,
                    start + Duration::from_secs(s),
                    [Some(79.0), None],
                    None,
                    false,
                );
                s += 1;
            }
        }
        assert_eq!(events.len(), 20);
        assert!(events
            .iter()
            .all(|event| event.status == EventStatus::Recovered
                && event.peak_percent == 99.0
                && event.processes.len() == 3));
        assert_eq!(events[0].id, 2);
    }

    #[test]
    fn short_high_period_and_missing_value_do_not_form_an_event() {
        let mut tracker = EventTracker::default();
        let mut events = vec![];
        let start = Instant::now();
        for s in 0..=29 {
            tracker.observe(
                &mut events,
                s * 1000,
                start + Duration::from_secs(s),
                [Some(90.0), None],
                None,
                false,
            );
        }
        tracker.observe(
            &mut events,
            30000,
            start + Duration::from_secs(30),
            [None, None],
            None,
            false,
        );
        assert!(events.is_empty());
    }
}
