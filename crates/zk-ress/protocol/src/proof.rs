use alloy_primitives::Bytes;
use alloy_rlp::{Decodable, Encodable};
use reth_ress_protocol::ExecutionWitness;

/// A trait for zk-ress execution proofs.
pub trait ExecutionProof:
    Encodable + Decodable + Default + Clone + Unpin + Send + Sync + 'static
{
    /// Returns `true` if the witness is empty.
    /// Used to identify default responses from peers.
    fn is_empty(&self) -> bool;
}

/// Blanket proof implementation for arbitrary bytes.
impl ExecutionProof for Bytes {
    fn is_empty(&self) -> bool {
        Bytes::is_empty(self)
    }
}

impl ExecutionProof for ExecutionWitness {
    fn is_empty(&self) -> bool {
        self.state.is_empty() && self.codes.is_empty()
    }
}
