//! Witness recording utilities for state providers.

use alloy_primitives::{keccak256, U256};
use reth_db::{
    cursor::{DbCursorRO, DbDupCursorRO},
    tables,
    transaction::DbTx,
};
use reth_revm::{witness::FlatWitnessRecord, State};
use revm_database::{AccountStatus, DbAccount};
use revm_state::Bytecode;

use crate::{providers::NodeTypesForProvider, DatabaseProvider, StateProvider};

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
    fn record_flat_witness<DB>(
        &self,
        statedb: &State<DB>,
        witness: &mut FlatWitnessRecord,
    ) -> Result<(), FlatWitnessRecordError>;
}

impl<TX, N> RecordFlatWitness for DatabaseProvider<TX, N>
where
    TX: DbTx + 'static,
    N: NodeTypesForProvider,
{
    fn record_flat_witness<DB>(
        &self,
        statedb: &State<DB>,
        witness: &mut FlatWitnessRecord,
    ) -> Result<(), FlatWitnessRecordError> {
        // The provided statedb contains post-execution state, so we use the provider to pull the
        // pre-state of the accessed accounts, storage slots, and codes.
        let state_provider = self.latest();

        witness.contracts = statedb
            .cache
            .contracts
            .values()
            .map(|code| code.original_bytes())
            .map(|code_bytes| (keccak256(&code_bytes), Bytecode::new_legacy(code_bytes)))
            .collect();

        for (address, account) in &statedb.cache.accounts {
            let provider_account = state_provider.basic_account(address)?;

            let mut db_account = match provider_account {
                Some(account) => DbAccount { info: account.into(), ..Default::default() },
                None => DbAccount::new_not_existing(),
            };

            // Fetch pre-state storage for accessed slots
            if let Some(account) = &account.account {
                for slot in account.storage.keys() {
                    let val = state_provider.storage(*address, (*slot).into())?.unwrap_or_default();
                    db_account.storage.insert(*slot, val);
                }
            }

            // When an account is destroyed, statedb discards all storage slots, losing track of
            // which slots were accessed pre-destruction. We fetch all storage slots for destroyed
            // accounts (mirroring TrieWitness::get_proof_targets in trie/trie/src/witness.rs),
            // requiring a lower-level db cursor. Only relevant pre-Cancun where SELFDESTRUCT clears
            // storage.
            if account.status == AccountStatus::Destroyed {
                let tx = self.tx_ref();
                let mut storage_cursor = tx.cursor_dup_read::<tables::PlainStorageState>()?;
                if let Some((_, first_entry)) = storage_cursor.seek_exact(*address)? {
                    db_account
                        .storage
                        .insert(U256::from_be_bytes(first_entry.key.0), first_entry.value);

                    while let Some((_, entry)) = storage_cursor.next_dup()? {
                        db_account.storage.insert(U256::from_be_bytes(entry.key.0), entry.value);
                    }
                }
                witness.destructed_addresses.insert(*address);
            }

            witness.accounts.insert(*address, db_account);
        }

        Ok(())
    }
}
