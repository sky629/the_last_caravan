//! 프레임 속도를 재는 곳입니다. 화면 없이 부하를 시험할 때 로그 한 줄로 프레임 속도를 확인하는 데 씁니다.

/// 프레임을 셀 때마다 `frame`을 부르면, 1초가 지날 때마다 그동안의 평균 프레임 속도를 돌려줍니다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FpsMeter {
    window_start_secs: f64,
    frames: u32,
}

impl FpsMeter {
    /// `now_secs`는 앱이 시작된 뒤 지난 시간(초)입니다.
    pub fn frame(&mut self, now_secs: f64) -> Option<f64> {
        self.frames += 1;
        let elapsed = now_secs - self.window_start_secs;
        if elapsed < 1.0 {
            return None;
        }
        let fps = f64::from(self.frames) / elapsed;
        self.window_start_secs = now_secs;
        self.frames = 0;
        Some(fps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reports_the_average_once_per_second() {
        let mut meter = FpsMeter::default();
        let mut reports = Vec::new();
        for frame in 1..=180 {
            if let Some(fps) = meter.frame(f64::from(frame) / 60.0) {
                reports.push(fps);
            }
        }
        assert_eq!(reports.len(), 3);
        assert!(
            reports.iter().all(|fps| (fps - 60.0).abs() < 1.0),
            "{reports:?}"
        );
    }

    #[test]
    fn it_stays_quiet_inside_the_first_second() {
        let mut meter = FpsMeter::default();
        assert_eq!(meter.frame(0.2), None);
        assert_eq!(meter.frame(0.9), None);
    }
}
