//! Command-line interface for running tests.
use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use ef_tests::{cases::blockchain_test::BlockchainTests, Suite};

/// Command-line arguments for the test runner.
#[derive(Debug, Parser)]
pub struct TestRunnerCommand {
    /// Path to the test suite
    suite_path: PathBuf,
}

fn main() {
    // let mut cmd = TestRunnerCommand::parse();
    let suite_path =
        // PathBuf::from_str("/data/code-data/kev-reth/testing/ef-tests/reth-debug").unwrap();
        PathBuf::from_str("/data/code-data/kev-reth/testing/ef-tests/execution-spec-tests").unwrap();
    BlockchainTests::new(suite_path.join("blockchain_tests")).run();
}
