#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_dispute_window_24h() {
        let submitted = 0u64;
        let now = 86400u64;
        assert!(!super::super::utils::is_dispute_window_open(submitted, now + 1));
    }

    #[test]
    fn test_dispute_window_open() {
        let submitted = 0u64;
        let now = 43200u64;
        assert!(super::super::utils::is_dispute_window_open(submitted, now));
    }

    #[test]
    fn test_dispute_remaining() {
        let submitted = 0u64;
        let now = 43200u64;
        assert_eq!(super::super::utils::dispute_window_remaining(submitted, now), 43200);
    }
}
