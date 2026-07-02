// Feature 3: Multi-player Match Support
// Support N-way matches beyond simple 1v1 scenarios

use soroban_sdk::{Address, Env, Map, Symbol, Vec};

#[derive(Clone)]
pub struct MultiplayerMatch {
    pub match_id: String,
    pub players: Vec<Address>,
    pub stakes: Map<Address, u64>,
    pub total_stake: u64,
    pub created_at: u64,
    pub game_type: String,
    pub game_id: String,
}

impl MultiplayerMatch {
    pub fn new(
        match_id: String,
        players: Vec<Address>,
        game_type: String,
        game_id: String,
    ) -> Result<Self, Symbol> {
        if players.len() < 2 {
            return Err(Symbol::short("min_2_players"));
        }

        if players.len() > 8 {
            return Err(Symbol::short("max_8_players"));
        }

        Ok(MultiplayerMatch {
            match_id,
            players,
            stakes: Map::new(&Env::default()),
            total_stake: 0,
            created_at: 0,
            game_type,
            game_id,
        })
    }

    pub fn add_stake(&mut self, player: Address, amount: u64) -> Result<(), Symbol> {
        if !self.players.iter().any(|p| p == player) {
            return Err(Symbol::short("player_not_invited"));
        }

        self.stakes.set(player.clone(), amount);
        self.total_stake = self.total_stake.saturating_add(amount);
        Ok(())
    }

    pub fn get_player_count(&self) -> usize {
        self.players.len()
    }

    pub fn validate_all_stakes_locked(&self) -> Result<bool, Symbol> {
        for player in self.players.iter() {
            if self.stakes.get(player.clone()).is_none() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn calculate_equal_payout(&self, winner_count: u32) -> u64 {
        if winner_count == 0 {
            return 0;
        }
        self.total_stake / (winner_count as u64)
    }

    pub fn calculate_ranked_payout(&self, rank: u32) -> Result<u64, Symbol> {
        // Payout based on ranking (decreasing stakes)
        if rank > self.players.len() as u32 {
            return Err(Symbol::short("invalid_rank"));
        }

        let rank_index = rank.saturating_sub(1) as usize;
        let percentage = (100u64).saturating_sub((rank_index as u64) * 10);
        Ok((self.total_stake * percentage) / 100)
    }
}

pub fn distribute_to_winners(
    env: &Env,
    match_id: &str,
    winners: Vec<Address>,
    amount_per_winner: u64,
) -> Result<(), Symbol> {
    if winners.is_empty() {
        return Err(Symbol::short("no_winners"));
    }

    for winner in winners.iter() {
        env.events().publish(
            ("match", "payout_distributed"),
            (match_id, winner, amount_per_winner),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_payout_split() {
        let mut m = MultiplayerMatch {
            match_id: "m1".to_string(),
            players: Vec::new(),
            stakes: Map::new(&Env::default()),
            total_stake: 1000,
            created_at: 0,
            game_type: "chess".to_string(),
            game_id: "g1".to_string(),
        };

        let payout = m.calculate_equal_payout(4);
        assert_eq!(payout, 250);
    }

    #[test]
    fn test_ranked_payout() {
        let m = MultiplayerMatch {
            match_id: "m1".to_string(),
            players: Vec::new(),
            stakes: Map::new(&Env::default()),
            total_stake: 1000,
            created_at: 0,
            game_type: "chess".to_string(),
            game_id: "g1".to_string(),
        };

        let rank1_payout = m.calculate_ranked_payout(1).unwrap();
        let rank2_payout = m.calculate_ranked_payout(2).unwrap();
        
        assert!(rank1_payout > rank2_payout);
    }
}
