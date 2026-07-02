pub const DISPUTE_WINDOW_SECS: u64 = 86400;

pub fn is_dispute_window_open(submitted_at: u64, now: u64) -> bool {
    now <= submitted_at + DISPUTE_WINDOW_SECS
}

pub fn dispute_window_remaining(submitted_at: u64, now: u64) -> u64 {
    let window_end = submitted_at + DISPUTE_WINDOW_SECS;
    if now >= window_end {
        0
    } else {
        window_end - now
    }
}
