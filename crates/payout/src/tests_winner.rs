#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_winner_take_all_no_fee() {
        let (winner, fee) = super::super::utils::calculate_winner_take_all(200, 0);
        assert_eq!(winner, 200);
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_winner_take_all_with_fee() {
        let (winner, fee) = super::super::utils::calculate_winner_take_all(200, 50);
        assert_eq!(winner, 199);
        assert_eq!(fee, 1);
    }
}
