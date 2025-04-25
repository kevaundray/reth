//! Miscellaneous test utilities.

use crate::{RLPExecutionWitness, RessProtocolProvider};
use alloy_consensus::Header;
use alloy_primitives::{map::B256HashMap, B256};
use reth_ethereum_primitives::BlockBody;
use reth_storage_errors::provider::ProviderResult;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// Noop implementation of [`RessProtocolProvider`].
#[derive(Clone, Copy, Default, Debug)]
pub struct NoopRessProtocolProvider;

impl RessProtocolProvider for NoopRessProtocolProvider {
    fn header(&self, _block_hash: B256) -> ProviderResult<Option<Header>> {
        Ok(None)
    }

    fn block_body(&self, _block_hash: B256) -> ProviderResult<Option<BlockBody>> {
        Ok(None)
    }

    async fn witness(&self, _block_hash: B256) -> ProviderResult<RLPExecutionWitness> {
        Ok(Default::default())
    }
}

/// Mock implementation of [`RessProtocolProvider`].
#[derive(Clone, Default, Debug)]
pub struct MockRessProtocolProvider {
    headers: Arc<Mutex<B256HashMap<Header>>>,
    block_bodies: Arc<Mutex<B256HashMap<BlockBody>>>,
    witnesses: Arc<Mutex<B256HashMap<RLPExecutionWitness>>>,
    witness_delay: Option<Duration>,
}

impl MockRessProtocolProvider {
    /// Configure witness response delay.
    pub const fn with_witness_delay(mut self, delay: Duration) -> Self {
        self.witness_delay = Some(delay);
        self
    }

    /// Insert header.
    pub fn add_header(&self, block_hash: B256, header: Header) {
        self.headers.lock().unwrap().insert(block_hash, header);
    }

    /// Extend headers from iterator.
    pub fn extend_headers(&self, headers: impl IntoIterator<Item = (B256, Header)>) {
        self.headers.lock().unwrap().extend(headers);
    }

    /// Insert block body.
    pub fn add_block_body(&self, block_hash: B256, body: BlockBody) {
        self.block_bodies.lock().unwrap().insert(block_hash, body);
    }

    /// Extend block bodies from iterator.
    pub fn extend_block_bodies(&self, bodies: impl IntoIterator<Item = (B256, BlockBody)>) {
        self.block_bodies.lock().unwrap().extend(bodies);
    }

    /// Insert witness.
    pub fn add_witness(&self, block_hash: B256, witness: RLPExecutionWitness) {
        self.witnesses.lock().unwrap().insert(block_hash, witness);
    }

    /// Extend witnesses from iterator.
    pub fn extend_witnesses(
        &self,
        witnesses: impl IntoIterator<Item = (B256, RLPExecutionWitness)>,
    ) {
        self.witnesses.lock().unwrap().extend(witnesses);
    }
}

impl RessProtocolProvider for MockRessProtocolProvider {
    fn header(&self, block_hash: B256) -> ProviderResult<Option<Header>> {
        Ok(self.headers.lock().unwrap().get(&block_hash).cloned())
    }

    fn block_body(&self, block_hash: B256) -> ProviderResult<Option<BlockBody>> {
        Ok(self.block_bodies.lock().unwrap().get(&block_hash).cloned())
    }

    async fn witness(&self, block_hash: B256) -> ProviderResult<RLPExecutionWitness> {
        if let Some(delay) = self.witness_delay {
            tokio::time::sleep(delay).await;
        }
        Ok(self.witnesses.lock().unwrap().get(&block_hash).cloned().unwrap_or_default())
    }
}
