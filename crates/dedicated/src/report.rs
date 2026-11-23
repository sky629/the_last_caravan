//! 1초에 한 줄씩 서버의 규모와 Tick 시간을 기록합니다. 시계를 읽는 곳은 실행 파일이므로 시각은 호출하는 쪽이 넘깁니다.

use std::time::{Duration, Instant};

use caravan_server::ServerStats;

/// 기록하는 구간의 길이입니다.
const WINDOW: Duration = Duration::from_secs(1);

/// 한 구간 동안 Tick 하나를 실행하는 데 걸린 시간을 모읍니다.
pub struct TickTimer {
    window_started: Instant,
    ticks: u32,
    total: Duration,
    worst: Duration,
}

impl TickTimer {
    pub fn new(now: Instant) -> TickTimer {
        TickTimer {
            window_started: now,
            ticks: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
        }
    }

    /// `pump` 한 번이 `ticks`개의 Tick을 실행하는 데 `spent`가 걸렸다는 기록입니다.
    /// Tick이 없었던 `pump`는 입장과 퇴장만 처리한 것이므로 기록하지 않습니다.
    /// 여러 Tick을 한꺼번에 밀려서 실행했다면 Tick 하나가 걸린 시간을 똑같이 나눕니다.
    pub fn record(&mut self, ticks: u64, spent: Duration) {
        if ticks == 0 {
            return;
        }
        let each = spent / ticks as u32;
        self.ticks += ticks as u32;
        self.total += spent;
        self.worst = self.worst.max(each);
    }

    /// 구간이 끝났으면 기록할 한 줄을 돌려주고 다음 구간을 시작합니다.
    pub fn line_if_due(&mut self, now: Instant, stats: ServerStats) -> Option<String> {
        if now.duration_since(self.window_started) < WINDOW {
            return None;
        }
        let average = self.total.checked_div(self.ticks).unwrap_or(Duration::ZERO);
        let line = format!(
            "stats rooms={} members={} monsters={} ticks={} tick_avg_ms={:.2} tick_max_ms={:.2}",
            stats.rooms,
            stats.members,
            stats.monsters,
            self.ticks,
            average.as_secs_f64() * 1000.0,
            self.worst.as_secs_f64() * 1000.0,
        );
        *self = TickTimer::new(now);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> ServerStats {
        ServerStats {
            rooms: 1,
            members: 2,
            monsters: 87,
            ticks: 0,
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_line_reports_the_average_and_worst_tick_of_the_second_and_starts_over() {
        let start = Instant::now();
        let mut timer = TickTimer::new(start);
        timer.record(1, ms(1));
        timer.record(0, ms(40));
        timer.record(1, ms(2));
        timer.record(1, ms(6));
        assert_eq!(timer.line_if_due(start + ms(999), stats()), None);

        assert_eq!(
            timer.line_if_due(start + ms(1000), stats()).as_deref(),
            Some("stats rooms=1 members=2 monsters=87 ticks=3 tick_avg_ms=3.00 tick_max_ms=6.00")
        );
        assert_eq!(timer.line_if_due(start + ms(1500), stats()), None);
        assert_eq!(
            timer.line_if_due(start + ms(2000), stats()).as_deref(),
            Some("stats rooms=1 members=2 monsters=87 ticks=0 tick_avg_ms=0.00 tick_max_ms=0.00"),
            "a second without ticks reports zeros instead of dividing by zero"
        );
    }

    #[test]
    fn ticks_run_together_after_a_stall_share_the_time_they_took() {
        let start = Instant::now();
        let mut timer = TickTimer::new(start);
        timer.record(5, ms(20));
        assert_eq!(
            timer.line_if_due(start + ms(1000), stats()).as_deref(),
            Some("stats rooms=1 members=2 monsters=87 ticks=5 tick_avg_ms=4.00 tick_max_ms=4.00")
        );
    }
}
