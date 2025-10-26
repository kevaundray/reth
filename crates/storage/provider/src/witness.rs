//! Witness recording utilities for state providers.

use alloy_primitives::U256;
use reth_db::{
    cursor::{DbCursorRO, DbDupCursorRO},
    tables,
    transaction::DbTx,
};
use reth_revm::witness::{AccessedAccount, FlatWitnessRecord};
use reth_stateless::flat_witness::FlatPreState;
use revm_database::DbAccount;

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
    fn flat_witness(
        &self,
        witness: FlatWitnessRecord,
    ) -> Result<FlatPreState, FlatWitnessRecordError>;
}

impl<TX, N> RecordFlatWitness for DatabaseProvider<TX, N>
where
    TX: DbTx + 'static,
    N: NodeTypesForProvider,
{
    fn flat_witness(
        &self,
        record: FlatWitnessRecord,
    ) -> Result<FlatPreState, FlatWitnessRecordError> {
        // The provided statedb contains post-execution state, so we use the provider to pull the
        // pre-state of the accessed accounts, storage slots, and codes.
        let state_provider = self.latest();

        let mut prestate = FlatPreState::default();
        for (code_hash, code) in &record.contracts {
            match code {
                Some(bytecode) => {
                    prestate.contracts.insert(*code_hash, bytecode.clone());
                }
                None => {
                    // Fetch code from provider if not present in record
                    if let Some(code) = state_provider.bytecode_by_hash(code_hash)? {
                        prestate.contracts.insert(*code_hash, code.0);
                    }
                }
            }
        }

        for (address, account) in &record.accounts {
            let provider_account = state_provider.basic_account(address)?;

            let mut db_account = match provider_account {
                Some(account) => DbAccount { info: account.into(), ..Default::default() },
                None => DbAccount::new_not_existing(),
            };

            match account {
                AccessedAccount::Destroyed => {
                    // When an account is destroyed, statedb discards all storage slots, losing track of
                    // which slots were accessed pre-destruction. We fetch all storage slots for destroyed
                    // accounts (mirroring TrieWitness::get_proof_targets in trie/trie/src/witness.rs),
                    // requiring a lower-level db cursor. Only relevant pre-Cancun where SELFDESTRUCT clears
                    // storage.
                    let tx = self.tx_ref();
                    let mut storage_cursor = tx.cursor_dup_read::<tables::PlainStorageState>()?;
                    if let Some((_, first_entry)) = storage_cursor.seek_exact(*address)? {
                        db_account
                            .storage
                            .insert(U256::from_be_bytes(first_entry.key.0), first_entry.value);

                        while let Some((_, entry)) = storage_cursor.next_dup()? {
                            db_account
                                .storage
                                .insert(U256::from_be_bytes(entry.key.0), entry.value);
                        }
                    }
                    prestate.destructed_addresses.insert(*address);
                }
                AccessedAccount::StorageKeys(storage_keys) => {
                    // Fetch pre-state storage for accessed slots
                    for slot in storage_keys {
                        let val =
                            state_provider.storage(*address, (*slot).into())?.unwrap_or_default();
                        db_account.storage.insert(*slot, val);
                    }
                }
            }
            prestate.accounts.insert(*address, db_account);
        }

        Ok(prestate)
    }
}
