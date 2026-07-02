#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_fee_calculation_zero() {
        assert_eq!(super::super::fees::calculate_fee(100, 0), 0);
    }

    #[test]
    fn test_fee_calculation_50bps() {
        assert_eq!(super::super::fees::calculate_fee(200, 50), 1);
    }

    #[test]
    fn test_fee_bps_valid() {
        assert!(super::super::fees::validate_fee_bps(50));
    }

    #[test]
    fn test_fee_bps_invalid() {
        assert!(!super::super::fees::validate_fee_bps(1001));
    }
}
