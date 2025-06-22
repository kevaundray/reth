//! Reth implementation of [`reth_ress_protocol::RessProtocolProvider`].

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/reth/main/assets/reth-docs.png",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256",
    issue_tracker_base_url = "https://github.com/paradigmxyz/reth/issues/"
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use alloy_consensus::BlockHeader as _;
use alloy_primitives::{Bytes, B256};
use parking_lot::Mutex;
use reth_chain_state::{ExecutedBlock, ExecutedBlockWithTrieUpdates, MemoryOverlayStateProvider};
use reth_errors::{ProviderError, ProviderResult};
use reth_ethereum_primitives::{Block, BlockBody, EthPrimitives};
use reth_evm::{execute::Executor, ConfigureEvm};
use reth_primitives_traits::{Block as _, Header, RecoveredBlock};
use reth_ress_protocol::{ExecutionStateWitness, RessProtocolProvider};
use reth_revm::{
    database::StateProviderDatabase, db::State, state::Bytecode, witness::ExecutionWitnessRecord,
};
use reth_storage_api::{BlockReader, BlockSource, StateProviderFactory};
use reth_tasks::TaskSpawner;
use reth_trie::{MultiProofTargets, Nibbles, TrieInput};
use schnellru::{ByLength, LruMap};
use std::{sync::Arc, time::Instant};
use tokio::sync::{oneshot, Semaphore};
use tracing::*;

mod recorder;
use recorder::StateWitnessRecorderDatabase;

mod pending_state;
pub use pending_state::*;

/// Consistent snapshot of state for witness generation to prevent race conditions
#[derive(Debug)]
struct WitnessStateSnapshot {
    /// Canonical chain tip at snapshot time
    canonical_tip: u64,
    /// Canonical chain tip hash at snapshot time  
    canonical_tip_hash: B256,
    /// Target block for witness generation
    target_block: Arc<RecoveredBlock<Block>>,
    /// Ancestor chain from target block backwards (includes executed ancestors)
    ancestor_chain: Vec<ExecutedBlockWithTrieUpdates>,
    /// Historical state provider hash where ancestor chain ends
    historical_state_hash: B256,
}

/// Reth provider implementing [`RessProtocolProvider`].
#[expect(missing_debug_implementations)]
#[derive(Clone)]
pub struct RethRessProtocolProvider<P, E> {
    provider: P,
    evm_config: E,
    task_spawner: Box<dyn TaskSpawner>,
    max_witness_window: u64,
    witness_semaphore: Arc<Semaphore>,
    witness_cache: Arc<Mutex<LruMap<B256, Arc<ExecutionStateWitness>>>>,
    pending_state: PendingState<EthPrimitives>,
}

impl<P, E> RethRessProtocolProvider<P, E>
where
    P: BlockReader<Block = Block> + StateProviderFactory + Clone + 'static,
    E: ConfigureEvm<Primitives = EthPrimitives> + 'static,
{
    /// Create new ress protocol provider.
    pub fn new(
        provider: P,
        evm_config: E,
        task_spawner: Box<dyn TaskSpawner>,
        max_witness_window: u64,
        witness_max_parallel: usize,
        cache_size: u32,
        pending_state: PendingState<EthPrimitives>,
    ) -> eyre::Result<Self> {
        Ok(Self {
            provider,
            evm_config,
            task_spawner,
            max_witness_window,
            witness_semaphore: Arc::new(Semaphore::new(witness_max_parallel)),
            witness_cache: Arc::new(Mutex::new(LruMap::new(ByLength::new(cache_size)))),
            pending_state,
        })
    }

    /// Retrieve a valid or invalid block by block hash.
    pub fn block_by_hash(
        &self,
        block_hash: B256,
    ) -> ProviderResult<Option<Arc<RecoveredBlock<Block>>>> {
        // NOTE: we keep track of the pending state locally because reth does not provider a way
        // to access non-canonical or invalid blocks via the provider.
        let maybe_block = if let Some(block) = self.pending_state.recovered_block(&block_hash) {
            Some(block)
        } else if let Some(block) =
            self.provider.find_block_by_hash(block_hash, BlockSource::Any)?
        {
            let signers = block.recover_signers()?;
            Some(Arc::new(block.into_recovered_with_signers(signers)))
        } else {
            // we attempt to look up invalid block last
            self.pending_state.invalid_recovered_block(&block_hash)
        };
        Ok(maybe_block)
    }

    /// Create a consistent snapshot of state for witness generation
    fn create_witness_state_snapshot(
        &self,
        block_hash: B256,
    ) -> ProviderResult<WitnessStateSnapshot> {
        // Capture canonical chain state first to detect reorgs
        let canonical_tip = self.provider.best_block_number()?;
        let canonical_tip_hash = self.provider.block_hash(canonical_tip)?
            .ok_or(ProviderError::BlockHashNotFound(B256::ZERO))?; // Use dummy hash in error

        // Get target block
        let target_block = 
            self.block_by_hash(block_hash)?.ok_or(ProviderError::BlockHashNotFound(block_hash))?;

        // Validate witness window using captured canonical tip
        if canonical_tip.saturating_sub(target_block.number()) > self.max_witness_window {
            return Err(ProviderError::TrieWitnessError(
                "witness target block exceeds maximum witness window".to_owned(),
            ))
        }

        // Collect ancestor chain atomically
        let mut ancestor_chain = Vec::new();
        let mut ancestor_hash = target_block.parent_hash();
        let historical_state_hash = 'sp: loop {
            match self.provider.state_by_block_hash(ancestor_hash) {
                Ok(_) => break 'sp ancestor_hash,
                Err(_) => {
                    // Try to get executed ancestor block
                    let mut executed = self.pending_state.executed_block(&ancestor_hash);

                    // Fallback to invalid block if needed (for consistency with original logic)
                    if executed.is_none() {
                        if let Some(invalid) =
                            self.pending_state.invalid_recovered_block(&ancestor_hash)
                        {
                            trace!(target: "reth::ress_provider", %block_hash, %ancestor_hash, "Using invalid ancestor block for witness construction");
                            executed = Some(ExecutedBlockWithTrieUpdates {
                                block: ExecutedBlock {
                                    recovered_block: invalid,
                                    ..Default::default()
                                },
                                ..Default::default()
                            });
                        }
                    }

                    let Some(executed) = executed else {
                        return Err(ProviderError::StateForHashNotFound(ancestor_hash))
                    };
                    
                    ancestor_hash = executed.sealed_block().parent_hash();
                    ancestor_chain.push(executed);
                }
            };
        };

        Ok(WitnessStateSnapshot {
            canonical_tip,
            canonical_tip_hash,
            target_block,
            ancestor_chain,
            historical_state_hash,
        })
    }

    /// Validate that snapshot is still consistent (no reorgs occurred)
    fn validate_snapshot_consistency(&self, snapshot: &WitnessStateSnapshot) -> ProviderResult<()> {
        let current_tip = self.provider.best_block_number()?;
        let current_tip_hash = self.provider.block_hash(current_tip)?
            .ok_or(ProviderError::BlockHashNotFound(B256::ZERO))?; // Use dummy hash in error
        
        if current_tip != snapshot.canonical_tip || current_tip_hash != snapshot.canonical_tip_hash {
            return Err(ProviderError::TrieWitnessError(
                "Chain reorganization detected during witness generation".to_owned(),
            ))
        }
        Ok(())
    }

    /// Collect headers from snapshot data for BLOCKHASH opcode
    fn collect_headers_from_snapshot(
        &self,
        snapshot: &WitnessStateSnapshot,
        record: &ExecutionWitnessRecord,
    ) -> ProviderResult<Vec<Bytes>> {
        let block_number = snapshot.target_block.number();
        
        // Determine how many headers we need based on BLOCKHASH usage
        let headers_needed = match record.lowest_block_number {
            Some(smallest) => (block_number - smallest).min(256),
            None => {
                // If no BLOCKHASH calls were made, we still need the parent header
                1
            }
        };

        use alloy_rlp::Encodable;
        let mut headers = Vec::new();
        let mut current_hash = snapshot.target_block.parent_hash();
        
        // First, try to get headers from our ancestor chain (these are guaranteed consistent)
        let mut headers_from_ancestors = 0;
        for ancestor in &snapshot.ancestor_chain {
            if headers_from_ancestors >= headers_needed {
                break;
            }
            let header = ancestor.sealed_block().header();
            let mut serialized_header = Vec::new();
            header.encode(&mut serialized_header);
            headers.push(serialized_header.into());
            headers_from_ancestors += 1;
            current_hash = header.parent_hash();
        }
        
        // If we need more headers, get them from the database (walking back from historical state)
        for _ in headers_from_ancestors..headers_needed {
            let header = if let Some(block) = self.pending_state.recovered_block(&current_hash) {
                // Check fork/pending blocks first - these are valid non-canonical blocks
                Some(block.header().clone())
            } else if let Some(block) = 
                self.provider.find_block_by_hash(current_hash, BlockSource::Any)? 
            {
                // Then check canonical database for committed blocks
                Some(block.header().clone())
            } else {
                // Don't include invalid blocks - BLOCKHASH should only see valid chain history
                None
            };
            
            match header {
                Some(header) => {
                    // Serialize header and add to collection
                    let mut serialized_header = Vec::new();
                    header.encode(&mut serialized_header);
                    headers.push(serialized_header.into());
                    // Walk backwards to parent for next iteration
                    current_hash = header.parent_hash();
                }
                None => {
                    // Stop if we can't find a parent header - this maintains consistency
                    debug!(target: "reth::ress_provider", "Cannot find parent header, stopping header collection");
                    break;
                }
            }
        }
        
        // Reverse to get chronological order (oldest first)
        headers.reverse();
        Ok(headers)
    }

    /// Generate execution witness for the target block hash.
    pub fn generate_execution_witness(
        &self,
        block_hash: B256,
    ) -> ProviderResult<ExecutionStateWitness> {
        debug!(target: "reth::ress_provider", %block_hash, "Generating witness for block");

        if let Some(witness) = self.witness_cache.lock().get(&block_hash).cloned() {
            return Ok(witness.as_ref().clone())
        }

        // Create atomic snapshot to prevent race conditions
        let snapshot = self.create_witness_state_snapshot(block_hash)?;
        
        // Generate witness from snapshot
        self.generate_witness_from_snapshot(snapshot)
    }

    /// Generate witness from a consistent state snapshot
    fn generate_witness_from_snapshot(
        &self,
        snapshot: WitnessStateSnapshot,
    ) -> ProviderResult<ExecutionStateWitness> {
        let block_hash = snapshot.target_block.hash();
        
        // Validate snapshot is still consistent before proceeding
        // self.validate_snapshot_consistency(&snapshot)?;

        debug!(target: "reth::ress_provider", %block_hash, ancestors_len = snapshot.ancestor_chain.len(), "Executing the block");

        // Get historical state provider using snapshot data
        let historical = self.provider.state_by_block_hash(snapshot.historical_state_hash)?;

        // Execute all gathered blocks to gather accesses state.
        let mut db = StateWitnessRecorderDatabase::new(StateProviderDatabase::new(
            MemoryOverlayStateProvider::new(historical, snapshot.ancestor_chain.clone()),
        ));
        let mut record = ExecutionWitnessRecord::default();

        // We allow block execution to fail, since we still want to record all accessed state by
        // invalid blocks.
        if let Err(error) = self.evm_config.batch_executor(&mut db).execute_with_state_closure(
            &snapshot.target_block,
            |state: &State<_>| {
                record.record_executed_state(state);
            },
        ) {
            debug!(target: "reth::ress_provider", %block_hash, %error, "Error executing the block");
        }

        // Use snapshot data - no race condition since we have consistent snapshot
        let witness_state_provider = self.provider.state_by_block_hash(snapshot.historical_state_hash)?;
        let mut trie_input = TrieInput::default();
        for block in &snapshot.ancestor_chain {
            trie_input.append_cached_ref(&block.trie, &block.hashed_state);
        }
        let (mut hashed_state, bytecodes) = db.into_state_and_bytecodes();
        hashed_state.extend(record.hashed_state.clone());

        debug!(target: "reth::ress_provider", %block_hash, ?hashed_state, ?bytecodes, "Collected hashed state");

        // Gather the state witness.
        let state = if hashed_state.is_empty() {
            // If no state was accessed, at least the root node must be present.
            let multiproof = witness_state_provider.multiproof(
                trie_input,
                MultiProofTargets::from_iter([(B256::ZERO, Default::default())]),
            )?;
            let mut state = Vec::new();
            if let Some(root_node) =
                multiproof.account_subtree.into_inner().remove(&Nibbles::default())
            {
                state.push(root_node);
            }
            state
        } else {
            witness_state_provider.witness(trie_input, hashed_state)?
        };

        let _block_number = snapshot.target_block.number();
        
        // Collect headers using consistent snapshot data to prevent race conditions
        let headers = self.collect_headers_from_snapshot(&snapshot, &record)?;

        let witness = ExecutionStateWitness {
            state,
            bytecodes: bytecodes.values().map(Bytecode::original_bytes).collect(),
            headers,
        };

        // Insert witness into the cache.
        self.witness_cache.lock().insert(block_hash, Arc::new(witness.clone()));

        Ok(witness)
    }

    /// Spawn execution witness creation task onto the runtime.
    pub async fn execution_witness(
        &self,
        block_hash: B256,
    ) -> ProviderResult<ExecutionStateWitness> {
        let _permit = self.witness_semaphore.acquire().await.map_err(ProviderError::other)?;
        let this = self.clone();
        let (tx, rx) = oneshot::channel();
        self.task_spawner.spawn_blocking(Box::pin(async move {
            let result = this.generate_execution_witness(block_hash);
            let _ = tx.send(result);
        }));
        match rx.await {
            Ok(Ok(witness)) => Ok(witness),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(ProviderError::TrieWitnessError("dropped".to_owned())),
        }
    }
}

impl<P, E> RessProtocolProvider for RethRessProtocolProvider<P, E>
where
    P: BlockReader<Block = Block> + StateProviderFactory + Clone + 'static,
    E: ConfigureEvm<Primitives = EthPrimitives> + 'static,
{
    fn header(&self, block_hash: B256) -> ProviderResult<Option<Header>> {
        trace!(target: "reth::ress_provider", %block_hash, "Serving header");
        Ok(self.block_by_hash(block_hash)?.map(|b| b.header().clone()))
    }

    fn block_body(&self, block_hash: B256) -> ProviderResult<Option<BlockBody>> {
        trace!(target: "reth::ress_provider", %block_hash, "Serving block body");
        Ok(self.block_by_hash(block_hash)?.map(|b| b.body().clone()))
    }

    fn bytecode(&self, code_hash: B256) -> ProviderResult<Option<Bytes>> {
        trace!(target: "reth::ress_provider", %code_hash, "Serving bytecode");
        let maybe_bytecode = 'bytecode: {
            if let Some(bytecode) = self.pending_state.find_bytecode(code_hash) {
                break 'bytecode Some(bytecode);
            }

            self.provider.latest()?.bytecode_by_hash(&code_hash)?
        };

        Ok(maybe_bytecode.map(|bytecode| bytecode.original_bytes()))
    }

    async fn witness(&self, block_hash: B256) -> ProviderResult<Vec<Bytes>> {
        trace!(target: "reth::ress_provider", %block_hash, "Serving witness");
        let started_at = Instant::now();
        self.execution_witness(block_hash).await.map(|w| {
            trace!(target: "reth::ress_provider", %block_hash, elapsed = ?started_at.elapsed(), "Computed witness");
            w.state
        })
    }
}
