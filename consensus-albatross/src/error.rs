use thiserror::Error;

use blockchain_albatross::BlockchainError;


#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    BlockchainError(#[from] BlockchainError),
}


#[derive(Debug, Error)]
pub enum SyncError {
    #[error("Other")]
    Other,
    #[error("No valid sync target found")]
    NoValidSyncTarget,
}
