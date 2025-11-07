//! Witness recording utilities for state providers.

use reth_execution_types::{FlatPreState, FlatWitnessRecord};

/// Errors that can occur during flat witness record generation.
#[derive(Debug, thiserror::Error)]
pub enum FlatWitnessRecordError {
    /// Failed to retrieve account from state provider
    #[error("Failed to get prestate account: {0}")]
    GetAccountError(#[from] crate::ProviderError),
    /// Failed to access database cursor
    #[error("Failed to create database cursor: {0}")]
    DatabaseCursorError(#[from] reth_db::DatabaseError),
}

/// Extension trait for recording flat witness data from state providers.
pub trait RecordFlatWitness {
    /// Records pre-state from database for all accounts touched during execution.
    ///
    /// The provided `statedb` contains post-execution state, so this method uses the provider
    /// to pull the pre-state of the accessed accounts, storage slots, and codes.
    fn flat_witness(
        &self,
        witness: FlatWitnessRecord,
    ) -> Result<FlatPreState, FlatWitnessRecordError>;
}
