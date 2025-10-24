//! Flat execution witness for stateless block validation.
//!
//! This module provides a simplified witness structure containing the minimal state
//! data required for stateless execution of a block. The "flat" representation stores
//! state directly in a cache rather than as a Merkle proof, optimizing for execution
//! speed. This is useful if the flat execution witness is later cryptographically proven
//! correct in an independent proof.

use reth_primitives_traits::Header;
use reth_revm::db::Cache;

/// A flat execution witness containing the state and context needed for stateless block execution.
#[derive(Debug, Clone)]
pub struct FlatExecutionWitness {
    /// The state required for executing the block.
    pub state: Cache,
    /// The parent block header providing context for block validation.
    pub parent_header: Header,
}
