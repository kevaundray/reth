//! Benchmark comparing `std::sync::mpsc` and `crossbeam` channels for `StateRootTask`.

#![allow(missing_docs)]

use alloy_primitives::{B256, U256};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use proptest::test_runner::TestRunner;
use rand::Rng;
use revm_primitives::{Address, HashMap};
use revm_state::{Account, AccountInfo, AccountStatus, EvmState, EvmStorage, EvmStorageSlot};
use std::{hint::black_box, thread};

/// Creates a mock state with the specified number of accounts for benchmarking
fn create_bench_state(num_accounts: usize) -> EvmState {
    let mut runner = TestRunner::deterministic();
    let mut rng = runner.rng().clone();
    let mut state_changes = HashMap::default();

    for i in 0..num_accounts {
        let storage =
            EvmStorage::from_iter([(U256::from(i), EvmStorageSlot::new(U256::from(i + 1), 0))]);

        let account = Account {
            info: AccountInfo {
                balance: U256::from(100),
                nonce: 10,
                code_hash: B256::from_slice(&rng.random::<[u8; 32]>()),
                code: Default::default(),
            },
            storage,
            status: AccountStatus::empty(),
            transaction_id: 0,
        };

        let address = Address::with_last_byte(i as u8);
        state_changes.insert(address, account);
    }

    state_changes
}

/// Simulated `StateRootTask` with `std::sync::mpsc`
struct StdStateRootTask {
    rx: std::sync::mpsc::Receiver<EvmState>,
}

impl StdStateRootTask {
    const fn new(rx: std::sync::mpsc::Receiver<EvmState>) -> Self {
        Self { rx }
    }

    fn run(self) {
        while let Ok(state) = self.rx.recv() {
            black_box(state);
        }
    }
}

/// Simulated `StateRootTask` with `crossbeam-channel`
struct CrossbeamStateRootTask {
    rx: crossbeam_channel::Receiver<EvmState>,
}

impl CrossbeamStateRootTask {
    const fn new(rx: crossbeam_channel::Receiver<EvmState>) -> Self {
        Self { rx }
    }

    fn run(self) {
        while let Ok(state) = self.rx.recv() {
            black_box(state);
        }
    }
}

/// Benchmarks the performance of different channel implementations for state streaming
fn bench_state_stream(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_stream_channels");
    group.sample_size(10);

    for size in &[1, 10, 100] {
        let bench_setup = || {
            let states: Vec<_> = (0..100).map(|_| create_bench_state(*size)).collect();
            states
        };

        group.bench_with_input(BenchmarkId::new("std_channel", size), size, |b, _| {
            b.iter_batched(
                bench_setup,
                |states| {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let task = StdStateRootTask::new(rx);

                    let processor = thread::spawn(move || {
                        task.run();
                    });

                    for state in states {
                        tx.send(state).unwrap();
                    }
                    drop(tx);

                    processor.join().unwrap();
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("crossbeam_channel", size), size, |b, _| {
            b.iter_batched(
                bench_setup,
                |states| {
                    let (tx, rx) = crossbeam_channel::unbounded();
                    let task = CrossbeamStateRootTask::new(rx);

                    let processor = thread::spawn(move || {
                        task.run();
                    });

                    for state in states {
                        tx.send(state).unwrap();
                    }
                    drop(tx);

                    processor.join().unwrap();
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_state_stream);
criterion_main!(benches);
