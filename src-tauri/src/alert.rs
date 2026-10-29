use std::time::{Duration, Instant};

const HIGH_AFTER: Duration = Duration::from_secs(30);
const REARM_AFTER: Duration = Duration::from_secs(60);
const MAX_SAMPLE_GAP: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadMetric {
    Cpu,
    Memory,
}

#[derive(Default)]
struct MetricWatch {
    high_since: Option<(LoadMetric, Instant)>,
    last_sample_at: Option<Instant>,
    active: bool,
    recovery_since: Option<Instant>,
}

impl MetricWatch {
    fn observe(&mut self, metric: LoadMetric, value: Option<f64>, now: Instant) -> bool {
        let Some(value) = value.filter(|value| value.is_finite()) else {
            self.high_since = None;
            self.recovery_since = None;
            self.last_sample_at = None;
            return false;
        };
        if self
            .last_sample_at
            .is_some_and(|previous| now.duration_since(previous) > MAX_SAMPLE_GAP)
        {
            self.high_since = None;
            self.recovery_since = None;
        }
        self.last_sample_at = Some(now);
        if self.active {
            if value < 80.0 {
                let since = *self.recovery_since.get_or_insert(now);
                if now.duration_since(since) >= REARM_AFTER {
                    self.active = false;
                    self.recovery_since = None;
                }
            } else {
                self.recovery_since = None;
            }
            return false;
        }
        if value < 90.0 {
            self.high_since = None;
            return false;
        }
        let since = match self.high_since {
            Some((previous, since)) if previous == metric => since,
            _ => {
                self.high_since = Some((metric, now));
                return false;
            }
        };
        if now.duration_since(since) >= HIGH_AFTER {
            self.active = true;
            self.high_since = None;
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
pub struct HighLoadAlert {
    cpu: MetricWatch,
    memory: MetricWatch,
}

impl HighLoadAlert {
    pub fn observe(
        &mut self,
        enabled: bool,
        cpu: Option<f32>,
        memory: Option<f64>,
        now: Instant,
    ) -> Vec<LoadMetric> {
        if !enabled {
            *self = Self::default();
            return vec![];
        }
        let mut triggered = Vec::with_capacity(2);
        if self.cpu.observe(LoadMetric::Cpu, cpu.map(f64::from), now) {
            triggered.push(LoadMetric::Cpu);
        }
        if self.memory.observe(LoadMetric::Memory, memory, now) {
            triggered.push(LoadMetric::Memory);
        }
        triggered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observe_for(
        alert: &mut HighLoadAlert,
        start: Instant,
        start_sec: u64,
        end_sec: u64,
        cpu: Option<f32>,
        memory: Option<f64>,
    ) -> Vec<LoadMetric> {
        let mut fired = vec![];
        for sec in start_sec..=end_sec {
            fired = alert.observe(true, cpu, memory, start + Duration::from_secs(sec));
        }
        fired
    }

    #[test]
    fn sustained_high_load_fires_once_and_rearms_after_sixty_low_seconds() {
        let start = Instant::now();
        let mut alert = HighLoadAlert::default();
        assert_eq!(
            observe_for(&mut alert, start, 0, 29, Some(90.0), Some(20.0)),
            vec![]
        );
        assert_eq!(
            alert.observe(
                true,
                Some(90.0),
                Some(20.0),
                start + Duration::from_secs(30)
            ),
            vec![LoadMetric::Cpu]
        );
        assert_eq!(
            alert.observe(
                true,
                Some(95.0),
                Some(20.0),
                start + Duration::from_secs(31)
            ),
            vec![]
        );
        assert_eq!(
            observe_for(&mut alert, start, 32, 92, Some(79.0), Some(20.0)),
            vec![]
        );
        assert_eq!(
            alert.observe(
                true,
                Some(91.0),
                Some(20.0),
                start + Duration::from_secs(93)
            ),
            vec![]
        );
        assert_eq!(
            observe_for(&mut alert, start, 94, 123, Some(91.0), Some(20.0)),
            vec![LoadMetric::Cpu]
        );
        assert_eq!(
            alert.observe(
                true,
                Some(91.0),
                Some(20.0),
                start + Duration::from_secs(124)
            ),
            vec![]
        );
    }

    #[test]
    fn missing_samples_break_duration_but_do_not_clear_an_active_episode() {
        let start = Instant::now();
        let mut alert = HighLoadAlert::default();
        observe_for(&mut alert, start, 0, 30, Some(95.0), Some(20.0));
        assert_eq!(
            alert.observe(true, None, None, start + Duration::from_secs(31)),
            vec![]
        );
        assert_eq!(
            observe_for(&mut alert, start, 32, 100, Some(95.0), Some(20.0)),
            vec![]
        );
        assert_eq!(
            alert.observe(
                true,
                Some(79.0),
                Some(20.0),
                start + Duration::from_secs(101)
            ),
            vec![]
        );
    }

    #[test]
    fn memory_can_trigger_and_long_sample_gaps_do_not_count_as_continuous_load() {
        let start = Instant::now();
        let mut alert = HighLoadAlert::default();
        assert_eq!(
            observe_for(&mut alert, start, 0, 30, Some(10.0), Some(90.0)),
            vec![LoadMetric::Memory]
        );
        let mut fresh = HighLoadAlert::default();
        fresh.observe(true, Some(90.0), Some(20.0), start);
        assert_eq!(
            fresh.observe(
                true,
                Some(90.0),
                Some(20.0),
                start + Duration::from_secs(31)
            ),
            vec![]
        );
    }

    #[test]
    fn disabling_notifications_clears_unconfirmed_load() {
        let start = Instant::now();
        let mut alert = HighLoadAlert::default();
        observe_for(&mut alert, start, 0, 10, Some(95.0), Some(20.0));
        alert.observe(
            false,
            Some(95.0),
            Some(20.0),
            start + Duration::from_secs(11),
        );
        assert_eq!(
            observe_for(&mut alert, start, 12, 41, Some(95.0), Some(20.0)),
            vec![]
        );
    }

    #[test]
    fn cpu_and_memory_episodes_are_tracked_independently() {
        let start = Instant::now();
        let mut alert = HighLoadAlert::default();
        assert_eq!(
            observe_for(&mut alert, start, 0, 30, Some(95.0), Some(20.0)),
            vec![LoadMetric::Cpu]
        );
        assert_eq!(
            observe_for(&mut alert, start, 31, 61, Some(95.0), Some(90.0)),
            vec![LoadMetric::Memory]
        );
    }
}
