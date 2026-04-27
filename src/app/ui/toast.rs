use std::time::{Duration, Instant};

const DEFAULT_TTL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    shown_at: Instant,
    ttl: Duration,
}

impl Toast {
    pub fn new(message: String) -> Self {
        Self {
            message,
            shown_at: Instant::now(),
            ttl: DEFAULT_TTL,
        }
    }

    pub fn is_active(&self) -> bool {
        self.shown_at.elapsed() < self.ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn fresh_toast_is_active() {
        let toast = Toast::new("hello".to_string());
        assert!(toast.is_active());
        assert_eq!(toast.message, "hello");
    }

    #[test]
    fn toast_expires_after_ttl() {
        let mut toast = Toast::new("expires".to_string());
        toast.ttl = Duration::from_millis(10);
        toast.shown_at = Instant::now();
        sleep(Duration::from_millis(20));
        assert!(!toast.is_active());
    }
}
