use std::time::Duration;

const JITTER_MS_PER_UNIT: u64 = 4;

fn jitter() -> Duration {
    let mut byte = [0u8; 1];
    let _ = orion::util::secure_rand_bytes(&mut byte);
    Duration::from_millis(byte[0] as u64 * JITTER_MS_PER_UNIT)
}

pub fn with_jitter(base: Duration) -> Duration {
    base + jitter()
}

pub fn exponential_with_jitter(attempts: i32, cap: Duration) -> Duration {
    let capped_attempts = attempts.clamp(0, 8);
    let base_secs = (2u64 << capped_attempts).min(cap.as_secs());
    with_jitter(Duration::from_secs(base_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_jitter_stays_within_expected_range() {
        let base = Duration::from_secs(15);
        for _ in 0..200 {
            let result = with_jitter(base);
            assert!(result >= base);
            assert!(result <= base + Duration::from_millis(1020));
        }
    }

    #[test]
    fn exponential_with_jitter_respects_cap_at_high_attempts() {
        let cap = Duration::from_secs(300);
        for _ in 0..200 {
            let result = exponential_with_jitter(100, cap);
            assert!(result >= cap);
            assert!(result <= cap + Duration::from_millis(1020));
        }
    }

    #[test]
    fn exponential_with_jitter_grows_with_attempts() {
        let cap = Duration::from_secs(300);
        let low = exponential_with_jitter(0, cap);
        let high = exponential_with_jitter(4, cap);
        assert!(high > low);
    }
}
