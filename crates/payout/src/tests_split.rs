#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_split_payout_70_30() {
        let (winner, loser, fee) = super::super::utils::calculate_split_payout(200, 50, 7000);
        assert_eq!(fee, 1);
        assert_eq!(winner + loser + fee, 200);
    }

    #[test]
    fn test_split_payout_50_50() {
        let (winner, loser, fee) = super::super::utils::calculate_split_payout(200, 0, 5000);
        assert_eq!(winner, 100);
        assert_eq!(loser, 100);
        assert_eq!(fee, 0);
    }
}
