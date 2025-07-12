use alloy_genesis::GenesisAccount;
use alloy_primitives::{Address, B256, U256};
use alloy_trie::KECCAK_EMPTY;
use itertools::Itertools;
use reth_db_common::init::{insert_genesis_hashes, insert_genesis_history, insert_genesis_state};
use reth_evm::{block::SystemCaller, ConfigureEvm, Evm};
use reth_evm_ethereum::EthEvmConfig;
use reth_provider::{
    test_utils::create_test_provider_factory_with_chain_spec, BlockWriter, DatabaseProviderFactory,
    StorageLocation,
};
use reth_revm::{database::StateProviderDatabase, db::CacheDB, Database, DatabaseCommit};
use revm_inspectors::tracing::{TracingInspector, TracingInspectorConfig};
use thiserror::Error;

use alloc::{
    collections::btree_map::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use alloy_consensus::{BlockHeader, Header, Transaction};
use alloy_rlp::Decodable;
use alloy_rpc_types_debug::ExecutionWitness;
use alloy_rpc_types_trace::geth::GethTrace;
use reth_chainspec::{ChainSpec, MAINNET};
use reth_ethereum_primitives::Block;
use reth_primitives_traits::{RecoveredBlock, SealedBlock, SealedHeader};

use crate::{
    trie::StatelessSparseTrie, validation::compute_ancestor_hashes, witness_db::WitnessDatabase,
};

/// Error type for tracing operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Missing ancestor header
    #[error("missing ancestor header")]
    MissingAncestor,

    /// Decoding headers failed
    #[error("decoding headers failed")]
    DecodingHeaders,

    /// Compute ancestor hashes failed
    #[error("compute ancestor hashes failed")]
    ComputeAncestorHashes,

    /// Generate sparse trie failed
    #[error("generate sparse trie failed")]
    GenerateSparseTrie,

    /// Missing account data for address
    #[error("missing account data for address {addr}")]
    MissingAccountInTrie {
        /// The address for which account data is missing.
        addr: Address,
    },

    /// Missing storage data for address and storage key
    #[error("missing storage data for address {addr} and storage key {storage_key}")]
    MissingStorage {
        /// The address for which storage data is missing.
        addr: Address,
        /// The storage key for which data is missing.
        storage_key: U256,
    },

    /// Missing trace for the block
    #[error("missing trace for the block")]
    MissingTrace,

    /// Error creating the execution environment
    #[error("error creating the execution environment: {0}")]
    CreatingExecutionEnvironment(String),
}

/// Traces a block and its transactions from the provided witness.
pub fn trace_from_witness(
    block: RecoveredBlock<Block>,
    witness: ExecutionWitness,
) -> Result<Vec<GethTrace>, Error> {
    // TODO: consider extracting the ancestor processing, since it is also used in the validation module.
    let mut ancestor_headers: Vec<Header> = witness
        .headers
        .iter()
        .map(|serialized_header| {
            let bytes = serialized_header.as_ref();
            Header::decode(&mut &bytes[..])
        })
        .collect::<Result<_, _>>()
        .map_err(|_| Error::DecodingHeaders)?;
    ancestor_headers.sort_by_key(|header| header.number());
    // Check that the ancestor headers form a contiguous chain and are not just random headers.
    let ancestor_hashes = compute_ancestor_hashes(&block, &ancestor_headers)
        .map_err(|_| Error::ComputeAncestorHashes)?;
    // Get the last ancestor header and retrieve its state root.
    //
    // There should be at least one ancestor header, this is because we need the parent header to
    // retrieve the previous state root.
    // The edge case here would be the genesis block, but we do not create proofs for the genesis
    // block.
    let pre_state_root = match ancestor_headers.last() {
        Some(prev_header) => prev_header.state_root,
        None => panic!("There should be at least one ancestor header"),
    };

    let (trie, bytecode) = StatelessSparseTrie::new(&witness, pre_state_root)
        .map_err(|_| Error::GenerateSparseTrie)?;

    let mut db = WitnessDatabase::new(&trie, bytecode, ancestor_hashes);

    // Generate a map of the pre-state from the witness.
    // Recall that the witness `keys` are a flat list of 20-byte or 32-byte keys. 20-byte keys refer to addresses,
    // and the next group of contiguous 32-byte keys refer to storage slots for the address.
    let mut pre_state: BTreeMap<Address, GenesisAccount> = Default::default();
    let mut iter = witness.keys.iter();
    loop {
        let Some(key) = iter.next() else {
            break;
        };

        let addr = Address::from_slice(key);
        let basic = db.basic(addr).map_err(|_| Error::MissingAccountInTrie { addr })?;

        let Some(basic) = basic else {
            // For non-existent accounts, we skip adding it to the pre-state. Note that we know this account doesn't exist
            // since the previous `db.basic(...)` call would fail if the account didn't have a correct proof of absence.
            // We also skip any attached storage keys for the account too.
            for _ in iter.peeking_take_while(|k| k.len() == 32) {}
            continue;
        };
        let code = (basic.code_hash != KECCAK_EMPTY).then(|| {
            // The `unwrap_or_default` is correct since, even if an account has bytecode, it might not be provided
            // in the witness since wasn't required. For example, if the account was accessed via EXTCODEHASH.
            db.code_by_hash(basic.code_hash).unwrap_or_default().bytes()
        });

        let mut storage: BTreeMap<B256, B256> = Default::default();
        let storage_keys = iter.peeking_take_while(|k| k.len() == 32).collect::<Vec<_>>();
        for storage_key in storage_keys {
            let storage_key = U256::from_be_slice(storage_key.as_ref());
            let storage_value = db
                .storage(addr, storage_key)
                .map_err(|_| Error::MissingStorage { addr, storage_key })?;
            storage.insert(storage_key.into(), storage_value.into());
        }

        pre_state.insert(
            addr,
            GenesisAccount {
                balance: basic.balance,
                nonce: Some(basic.nonce),
                code,
                storage: Some(storage),
                private_key: None,
            },
        );
    }

    let trace = trace_block(
        MAINNET.clone(),
        SealedHeader::new_unhashed(MAINNET.genesis_header().clone()),
        pre_state,
        &[block],
    )?
    .into_iter()
    .next()
    .ok_or(Error::MissingTrace)?;

    Ok(trace)
}

/// Traces a block and its transactions using the provided chain spec and genesis state.
pub fn trace_block(
    chain_spec: Arc<ChainSpec>,
    prev_block_header: SealedHeader,
    pre_state: BTreeMap<Address, GenesisAccount>,
    blocks: &[RecoveredBlock<Block>],
) -> Result<Vec<Vec<GethTrace>>, Error> {
    // Create a new test database and initialize a provider for the test case.
    let factory = create_test_provider_factory_with_chain_spec(chain_spec.clone());
    let provider = factory.database_provider_rw().map_err(|e| {
        Error::CreatingExecutionEnvironment(format!("failed to create database provider: {e}"))
    })?;

    // Insert the previous block and pre-state into the database.
    let prev_block = SealedBlock::<Block>::from_sealed_parts(prev_block_header, Default::default())
        .try_recover()
        .map_err(|e| {
            Error::CreatingExecutionEnvironment(format!("failed to recover previous block: {e}"))
        })?;
    provider.insert_block(prev_block, StorageLocation::Database).map_err(|e| {
        Error::CreatingExecutionEnvironment(format!("failed to insert previous block: {e}"))
    })?;
    insert_genesis_state(&provider, pre_state.iter()).map_err(|e| {
        Error::CreatingExecutionEnvironment(format!("failed to insert genesis state: {e}"))
    })?;
    insert_genesis_hashes(&provider, pre_state.iter()).map_err(|e| {
        Error::CreatingExecutionEnvironment(format!("failed to insert genesis hashes: {e}"))
    })?;
    insert_genesis_history(&provider, pre_state.iter()).map_err(|e| {
        Error::CreatingExecutionEnvironment(format!("failed to insert genesis history: {e}"))
    })?;

    let executor_provider = EthEvmConfig::ethereum(chain_spec);
    let mut blocks_traces: Vec<Vec<GethTrace>> = Vec::new();
    for block in blocks {
        provider.insert_block(block.clone(), StorageLocation::Database).map_err(|e| {
            Error::CreatingExecutionEnvironment(format!("failed to insert block: {e}"))
        })?;

        let evm_env = executor_provider.evm_env(block.header());
        let mut db = CacheDB::new(StateProviderDatabase::new(provider.latest()));
        let mut evm = executor_provider.evm_with_env(&mut db, evm_env);

        let mut system_caller = SystemCaller::new(provider.chain_spec());
        system_caller.apply_pre_execution_changes(block.header(), &mut evm).map_err(|e| {
            Error::CreatingExecutionEnvironment(format!(
                "failed to apply pre-execution changes: {e}"
            ))
        })?;

        let mut inspector = TracingInspector::new(TracingInspectorConfig::default_geth());
        let evm_env = executor_provider.evm_env(block.header());

        let mut tx_traces: Vec<GethTrace> = Vec::new();
        for tx in block.transactions_recovered() {
            let mut evm = executor_provider.evm_with_env_and_inspector(
                &mut db,
                evm_env.clone(),
                &mut inspector,
            );
            let res = evm.transact(tx).map_err(|e| {
                Error::CreatingExecutionEnvironment(format!("transaction execution failed: {e}"))
            })?;
            db.commit(res.state);

            let gas_used = res.result.gas_used();
            let return_value = res.result.into_output().unwrap_or_default();
            inspector.set_transaction_gas_limit(tx.gas_limit());
            let get_trace: GethTrace = inspector
                .geth_builder()
                .geth_traces(
                    gas_used,
                    return_value.clone(),
                    alloy_rpc_types_trace::geth::GethDefaultTracingOptions::default(),
                )
                .into();
            tx_traces.push(get_trace);
        }
        blocks_traces.push(tx_traces);
    }

    Ok(blocks_traces)
}
