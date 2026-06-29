use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveVoteban {
    pub target_user_id: i64,
    pub target_message_id: i32,
    pub voteban_message_id: i32,
    pub initiator_id: i64,
    pub target_username: String,
    voters: HashMap<i64, bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoteCounts {
    pub for_ban: usize,
    pub against: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteResult {
    Recorded,
    AlreadyVoted,
    TargetCannotVote,
}

impl ActiveVoteban {
    pub fn new(
        target_user_id: i64,
        target_message_id: i32,
        voteban_message_id: i32,
        initiator_id: i64,
        target_username: String,
    ) -> Self {
        let mut voters = HashMap::new();
        voters.insert(initiator_id, true);
        Self {
            target_user_id,
            target_message_id,
            voteban_message_id,
            initiator_id,
            target_username,
            voters,
        }
    }

    pub fn record_vote(&mut self, user_id: i64, for_ban: bool) -> VoteResult {
        if user_id == self.target_user_id {
            return VoteResult::TargetCannotVote;
        }

        if self.voters.get(&user_id).copied() == Some(for_ban) {
            return VoteResult::AlreadyVoted;
        }

        self.voters.insert(user_id, for_ban);
        VoteResult::Recorded
    }

    pub fn counts(&self) -> VoteCounts {
        let for_ban = self.voters.values().filter(|vote| **vote).count();
        let against = self.voters.values().filter(|vote| !**vote).count();
        VoteCounts { for_ban, against }
    }

    pub fn for_voters(&self) -> Vec<i64> {
        self.voters
            .iter()
            .filter_map(|(user_id, vote)| (*vote).then_some(*user_id))
            .collect()
    }

    pub fn against_voters(&self) -> Vec<i64> {
        self.voters
            .iter()
            .filter_map(|(user_id, vote)| (!*vote).then_some(*user_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_vote_starts_with_initiator_for_ban() {
        let vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(
            vote.counts(),
            VoteCounts {
                for_ban: 1,
                against: 0
            }
        );
    }

    #[test]
    fn target_user_cannot_vote_on_own_ban() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(10, true), VoteResult::TargetCannotVote);
    }

    #[test]
    fn duplicate_same_direction_vote_is_reported() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(40, true), VoteResult::AlreadyVoted);
    }

    #[test]
    fn user_can_switch_vote_direction() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(50, false), VoteResult::Recorded);
        assert_eq!(
            vote.counts(),
            VoteCounts {
                for_ban: 1,
                against: 1
            }
        );
        assert_eq!(vote.record_vote(50, true), VoteResult::Recorded);
        assert_eq!(
            vote.counts(),
            VoteCounts {
                for_ban: 2,
                against: 0
            }
        );
    }
}
