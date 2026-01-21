//! Example: Running a gRPC witness server alongside a reth node.
//!
//! This example shows how to integrate the gRPC WitnessService with an Ethereum node.
//!
//! # Running the example
//!
//! ```bash
//! cargo run --example witness_server -- node --http
//! ```
//!
//! # Calling the gRPC service
//!
//! Using grpcurl:
//! ```bash
//! # Get witness for block 1000
//! grpcurl -plaintext -d '{"block_number": 1000}' \
//!     localhost:9000 reth.witness.WitnessService/GetExecutionWitness
//!
//! # Get witness for latest block (-1)
//! grpcurl -plaintext -d '{"block_number": -1}' \
//!     localhost:9000 reth.witness.WitnessService/GetExecutionWitness
//!
//! # Get witness by block hash
//! grpcurl -plaintext \
//!     -d '{"block_hash": "BASE64_ENCODED_32_BYTE_HASH"}' \
//!     localhost:9000 reth.witness.WitnessService/GetExecutionWitnessByHash
//! ```
//!
//! Using a Rust client:
//! ```rust,ignore
//! use reth_rpc_grpc::proto::witness_service_client::WitnessServiceClient;
//! use reth_rpc_grpc::proto::ExecutionWitnessRequest;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut client = WitnessServiceClient::connect("http://[::1]:9000").await?;
//!
//!     let response = client.get_execution_witness(ExecutionWitnessRequest {
//!         block_number: 1000,
//!     }).await?;
//!
//!     let witness = response.into_inner();
//!     println!("State entries: {}", witness.state.len());
//!     println!("Code entries: {}", witness.codes.len());
//!     println!("Key entries: {}", witness.keys.len());
//!     println!("Header entries: {}", witness.headers.len());
//!
//!     Ok(())
//! }
//! ```

fn main() {
    // This is a documentation-only example.
    // For a complete implementation, see the integration pattern below:
    //
    // ```rust
    // use reth::cli::Cli;
    // use reth_node_ethereum::EthereumNode;
    // use reth_rpc_grpc::{WitnessServer, WitnessServiceServer};
    // use tonic::transport::Server;
    //
    // fn main() -> eyre::Result<()> {
    //     Cli::parse_args().run(|builder, _| async move {
    //         let handle = builder
    //             .node(EthereumNode::default())
    //             .launch()
    //             .await?;
    //
    //         // Get the EthApi from the node
    //         let eth_api = handle.node.rpc_registry.eth_api().clone();
    //
    //         // Create and start gRPC server
    //         let witness_service = WitnessServer::new(eth_api);
    //         let grpc_server = Server::builder()
    //             .add_service(WitnessServiceServer::new(witness_service))
    //             .serve("[::1]:9000".parse().unwrap());
    //
    //         handle.node.task_executor.spawn_critical("gRPC server", async move {
    //             grpc_server.await.expect("gRPC server failed");
    //         });
    //
    //         handle.wait_for_node_exit().await
    //     })
    // }
    // ```

    println!("See the source code for integration examples.");
}
