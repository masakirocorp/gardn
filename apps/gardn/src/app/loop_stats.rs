use std::time::Duration;

const SLOW_LOOP: Duration = Duration::from_millis(32);

#[derive(Debug, Clone)]
pub(crate) struct LoopStats {
    enabled: bool,
    overlay: String,
    max_loop: Duration,
}

impl LoopStats {
    pub(crate) fn from_env() -> Self {
        let enabled = env_enabled();
        if enabled {
            tracing::warn!("GARDN_DEBUG_LOOP overlay enabled");
        }
        Self {
            enabled,
            overlay: String::new(),
            max_loop: Duration::ZERO,
        }
    }

    pub(crate) fn overlay_line(&self) -> Option<&str> {
        if self.enabled && !self.overlay.is_empty() {
            Some(self.overlay.as_str())
        } else {
            None
        }
    }

    pub(crate) fn finish_frame(
        &mut self,
        drain: Duration,
        schedule: Duration,
        draw: Duration,
        input: Duration,
        event: &str,
        _loop_total: Duration,
    ) {
        if !self.enabled {
            return;
        }
        let work = drain + schedule + draw + input;
        if work > self.max_loop {
            self.max_loop = work;
        }
        self.overlay = format!(
            " loop {:>5.1}ms drain {:>4.1} sch {:>4.1} draw {:>5.1} in {:>4.1} max {:>5.1} {event} ",
            ms(work),
            ms(drain),
            ms(schedule),
            ms(draw),
            ms(input),
            ms(self.max_loop),
        );
        if work >= SLOW_LOOP {
            tracing::warn!(
                loop_ms = ms(work),
                drain_ms = ms(drain),
                schedule_ms = ms(schedule),
                draw_ms = ms(draw),
                input_ms = ms(input),
                event,
                "slow ui loop"
            );
        }
    }
}

fn env_enabled() -> bool {
    match std::env::var("GARDN_DEBUG_LOOP") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_stats_do_not_build_an_overlay() {
        let mut stats = LoopStats {
            enabled: false,
            overlay: String::new(),
            max_loop: Duration::ZERO,
        };
        stats.finish_frame(
            Duration::from_millis(40),
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
            "draw",
            Duration::from_millis(40),
        );
        assert!(stats.overlay_line().is_none());
    }

    #[test]
    fn enabled_stats_track_max_and_format_the_overlay() {
        let mut stats = LoopStats {
            enabled: true,
            overlay: String::new(),
            max_loop: Duration::ZERO,
        };
        stats.finish_frame(
            Duration::from_millis(2),
            Duration::from_millis(3),
            Duration::from_millis(10),
            Duration::ZERO,
            "draw",
            Duration::from_millis(16),
        );
        stats.finish_frame(
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(40),
            Duration::from_millis(2),
            "input",
            Duration::from_millis(50),
        );
        let line = stats.overlay_line().expect("overlay");
        assert!(line.contains("draw"), "{line}");
        assert!(line.contains("input"), "{line}");
        assert!(line.contains("max"), "{line}");
        assert_eq!(stats.max_loop, Duration::from_millis(44));
    }
}
