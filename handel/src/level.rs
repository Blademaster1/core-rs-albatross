use std::{cmp::min, sync::Arc};

use parking_lot::RwLock;

use crate::partitioner::{Partitioner, PartitioningError};

/// Struct that defines the state of a level
#[derive(Clone, Debug)]
pub struct LevelState {
    /// Flag indicating that we have started sending updates on this level.
    pub started: bool,
    /// Flag indicating that this level is complete, i.e. this is true if we have aggregated all
    /// contributions for this level.
    pub complete: bool,
    /// The index of the next peer to send an update to.
    pub next_peer_index: usize,
}

/// Struct that defines an Aggregation Level
#[derive(Debug)]
pub struct Level {
    /// The ID of this level
    pub id: usize,
    /// The Peer IDs on this level
    pub peer_ids: Vec<usize>,
    /// The state of this level
    pub state: RwLock<LevelState>,
}

impl Level {
    /// Creates a new level given its id, the set of peers and the expected
    /// number of peers to consider this level send complete
    pub fn new(id: usize, peer_ids: Vec<usize>) -> Level {
        Level {
            id,
            peer_ids,
            state: RwLock::new(LevelState {
                started: false,
                complete: false,
                next_peer_index: 0,
            }),
        }
    }

    /// Returns the number of peers on this level
    pub fn num_peers(&self) -> usize {
        self.peer_ids.len()
    }

    /// Returns whether this level is empty
    pub fn is_empty(&self) -> bool {
        self.peer_ids.len() == 0
    }

    /// Creates a set of levels given a partitioner
    pub fn create_levels<P: Partitioner, TId: std::fmt::Debug>(
        partitioner: Arc<P>,
        id: TId,
        node_id: usize,
    ) -> Vec<Level> {
        let mut levels: Vec<Level> = Vec::new();
        // Begin with an empty range, as this side of the tree begins without any node on it.
        let mut tree_lhs = 0..0;

        for i in 0..partitioner.levels() {
            match partitioner.range(i) {
                Ok(tree_rhs) => {
                    let ids = tree_rhs.clone().collect::<Vec<usize>>();

                    trace!(
                        ?id,
                        level = i,
                        peers = ?ids,
                        "Peers on level",
                    );
                    let level = Level::new(i, ids);

                    if i == 0 {
                        // The first level is always started.
                        level.state.write().started = true;
                        // The first level sets its own id as the range.
                        tree_lhs = node_id..node_id + 1;
                        // Add the level
                        levels.push(level);
                        // Move to the next level
                        continue;
                    }

                    // Get start and end of our side to avoid having to clone the range
                    let tree_lhs_start = tree_lhs.start;
                    let tree_lhs_end = tree_lhs.end;

                    // Find this nodes index on its side of the tree
                    let index = tree_lhs
                        .position(|id| id == node_id)
                        .expect("The node must always be present on its side of the tree");

                    // Index of the next peer is symmetric, so set it to where this nodes position would be
                    // on its side of the sub tree. Levels may not be full, so it needs to be adjusted for that.
                    level.state.write().next_peer_index = index % level.peer_ids.len();

                    // All levels but the first must update their side of the tree
                    if *tree_rhs.end() + 1 == tree_lhs_start {
                        tree_lhs = *tree_rhs.start()..tree_lhs_end;
                    } else if *tree_rhs.start() == tree_lhs_end {
                        tree_lhs = tree_lhs_start..*tree_rhs.end() + 1;
                    } else {
                        log::info!(
                            "{}: {:?} - {:?}",
                            node_id,
                            tree_lhs_start..tree_lhs_end,
                            tree_rhs
                        );
                        panic!("ranges must be consecutive");
                    }

                    levels.push(level);
                }
                Err(PartitioningError::EmptyLevel { .. }) => {
                    let level = Level::new(i, vec![]);
                    levels.push(level);
                }
                Err(e) => panic!("{}", e),
            }
        }

        levels
    }

    /// Returns whether this level has been started.
    pub fn is_started(&self) -> bool {
        let state = self.state.read();
        state.started
    }

    /// Returns whether this level is complete.
    pub fn is_complete(&self) -> bool {
        let state = self.state.read();
        state.complete
    }

    /// Selects the set of next peers to send an update to for this level given a count of them
    pub fn select_next_peers(&self, count: usize) -> Vec<usize> {
        if self.id == 0 || self.is_empty() {
            vec![]
        } else {
            let num_peers = min(count, self.peer_ids.len());
            let mut selected: Vec<usize> = Vec::new();

            let mut state = self.state.write();
            for _ in 0..num_peers {
                selected.push(self.peer_ids[state.next_peer_index]);
                state.next_peer_index = (state.next_peer_index + 1) % self.peer_ids.len();
            }

            selected
        }
    }

    /// Starts the level if not already started.
    /// If the level was started before returns false, otherwise returns true.
    pub fn start(&self) -> bool {
        let mut state = self.state.write();
        let already_started = state.started;
        state.started = true;
        !already_started
    }
}

#[cfg(test)]
mod test {
    use nimiq_collections::bitset::BitSet;
    use nimiq_test_log::test;
    use rand::{thread_rng, Rng};
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::contribution::{AggregatableContribution, ContributionError};

    /// Dump Aggregate adding numbers.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct Contribution {
        contributors: BitSet,
    }

    impl AggregatableContribution for Contribution {
        fn contributors(&self) -> BitSet {
            self.contributors.clone()
        }

        fn combine(&mut self, _other_contribution: &Self) -> Result<(), ContributionError> {
            unimplemented!()
        }
    }

    #[test]
    fn it_can_handle_empty_level() {
        let mut rng = thread_rng();
        let id: usize = rng.gen_range(0..10);
        let level = Level::new(id, [].to_vec());

        // Check that the level is actually empty
        assert!(level.is_empty());
        assert_eq!(level.num_peers(), 0);

        // Start the level
        level.start();
        assert!(level.state.read().started);

        // Check that it can properly select next peers (return empty vector)
        assert!(level.select_next_peers(rng.gen_range(0..512)).is_empty());
    }

    #[test]
    fn it_can_handle_non_empty_level() {
        let mut rng = thread_rng();
        let id: usize = rng.gen_range(0..10);
        let num_ids = rng.gen_range(1..512);
        let mut ids: Vec<usize> = (0..num_ids).map(|_| rng.gen_range(0..=10)).collect();
        let level = Level::new(id, ids.clone());

        // Check that the level is not empty
        assert!(!level.is_empty());
        assert_eq!(level.num_peers(), num_ids);

        // Start the level
        level.start();

        // Check that it can properly select next peers (return empty vector)
        let select_size = rng.gen_range(0..num_ids) + 1;
        let iterations = num_ids / select_size;
        if id != 0 {
            for _ in 0..iterations {
                let exp_next_peers: Vec<usize> = ids.drain(0..select_size).collect();
                let next_peers = level.select_next_peers(select_size);
                log::debug!(select_size, ?exp_next_peers, ?next_peers);
                assert_eq!(next_peers, exp_next_peers);
            }
        } else {
            // For level 0 the next peers is always empty since it should be our own ID
            assert!(level.select_next_peers(select_size).is_empty());
        }
    }
}
