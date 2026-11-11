//! Monotonic request budget. A manual refresh cannot bypass server cooldowns.
#[derive(Clone, Debug)]
pub struct RefreshSchedule {
    due_at: u64,
    not_before: u64,
    retry_seconds: u64,
}

impl Default for RefreshSchedule {
    fn default() -> Self { Self { due_at: 0, not_before: 0, retry_seconds: 60 } }
}

impl RefreshSchedule {
    pub fn wait_seconds(&self, now: u64) -> u64 {
        self.due_at.max(self.not_before).saturating_sub(now)
    }
    pub fn request(&mut self, now: u64) { self.due_at = now.max(self.not_before); }
    pub fn succeeded(&mut self, now: u64, jitter: u64) {
        self.retry_seconds = 60;
        self.not_before = now.saturating_add(60);
        self.due_at = now.saturating_add(900 + jitter.min(60));
    }
    pub fn failed(&mut self, now: u64, server_delay: u64) {
        self.not_before = now.saturating_add(self.retry_seconds.max(server_delay.min(86_400)));
        self.due_at = self.not_before;
        self.retry_seconds = (self.retry_seconds * 2).min(3600);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_refresh_is_deferred_not_dropped() {
        let mut s = RefreshSchedule::default();
        s.succeeded(100, 20); s.request(110);
        assert_eq!(s.wait_seconds(110), 50);
        assert_eq!(s.wait_seconds(160), 0);
    }
    #[test]
    fn manual_refresh_cannot_skip_server_cooldown() {
        let mut s = RefreshSchedule::default();
        s.failed(10, 7200); s.request(11);
        assert_eq!(s.wait_seconds(11), 7199);
    }
    #[test]
    fn backoff_caps_and_resets_after_success() {
        let mut s = RefreshSchedule::default();
        for _ in 0..10 { s.failed(0, 0); }
        assert_eq!(s.wait_seconds(0), 3600);
        s.succeeded(0, 0); s.failed(0, 0);
        assert_eq!(s.wait_seconds(0), 60);
    }
}
