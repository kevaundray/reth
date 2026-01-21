//! gRPC API for high-performance binary data transfer.
//!
//! This crate provides a gRPC alternative to JSON-RPC for endpoints that transfer
//! large amounts of binary data, such as execution witnesses. Binary encoding
//! avoids the ~2x overhead of hex-encoding in JSON.
//!
//! # Example
//!
//! ```ignore
//! use reth_rpc_grpc::{WitnessServer, WitnessServiceServer};
//! use tonic::transport::Server;
//!
//! // Create the witness service with your eth API
//! let witness_service = WitnessServer::new(eth_api);
//!
//! // Start the gRPC server
//! Server::builder()
//!     .add_service(WitnessServiceServer::new(witness_service))
//!     .serve("[::1]:9000".parse().unwrap())
//!     .await?;
//! ```

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod witness;

#[cfg(test)]
mod bench;

pub use witness::WitnessServer;

/// Generated protobuf types and service traits.
pub mod proto {
    tonic::include_proto!("reth.witness");
}

pub use proto::witness_service_server::WitnessServiceServer;
