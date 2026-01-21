//! Benchmark comparing JSON-RPC vs gRPC serialization for execution witnesses.
//!
//! Run with: `cargo test -p reth-rpc-grpc --release -- bench_ --nocapture`

use alloy_primitives::Bytes;
use alloy_rpc_types_debug::ExecutionWitness;
use prost::Message;
use std::time::Instant;

use crate::proto::ExecutionWitnessResponse;

/// Generate a realistic mock execution witness with configurable size.
fn generate_mock_witness(
    num_state_nodes: usize,
    num_codes: usize,
    num_keys: usize,
    num_headers: usize,
) -> ExecutionWitness {
    // State nodes are typically 32-500 bytes (trie nodes)
    let state: Vec<Bytes> = (0..num_state_nodes)
        .map(|i| {
            let size = 32 + (i % 468); // Variable sizes 32-500 bytes
            Bytes::from(vec![((i * 7) % 256) as u8; size])
        })
        .collect();

    // Contract codes can be large (up to 24KB)
    let codes: Vec<Bytes> = (0..num_codes)
        .map(|i| {
            let size = 100 + (i * 1000) % 24000; // 100 bytes to 24KB
            Bytes::from(vec![((i * 13) % 256) as u8; size])
        })
        .collect();

    // Keys are typically 20 bytes (addresses) or 32 bytes (storage slots)
    let keys: Vec<Bytes> = (0..num_keys)
        .map(|i| {
            let size = if i % 2 == 0 { 20 } else { 32 };
            Bytes::from(vec![((i * 11) % 256) as u8; size])
        })
        .collect();

    // Headers are RLP-encoded, typically 500-600 bytes
    let headers: Vec<Bytes> = (0..num_headers)
        .map(|i| Bytes::from(vec![((i * 17) % 256) as u8; 550]))
        .collect();

    ExecutionWitness { state, codes, keys, headers }
}

/// Convert to proto format (what gRPC sends)
fn to_proto(witness: &ExecutionWitness) -> ExecutionWitnessResponse {
    ExecutionWitnessResponse {
        state: witness.state.iter().map(|b| b.to_vec()).collect(),
        codes: witness.codes.iter().map(|b| b.to_vec()).collect(),
        keys: witness.keys.iter().map(|b| b.to_vec()).collect(),
        headers: witness.headers.iter().map(|b| b.to_vec()).collect(),
    }
}

/// Benchmark result
#[derive(Debug)]
pub struct BenchResult {
    pub json_size: usize,
    pub proto_size: usize,
    pub json_serialize_us: u128,
    pub proto_serialize_us: u128,
    pub json_deserialize_us: u128,
    pub proto_deserialize_us: u128,
    pub size_reduction_percent: f64,
    pub serialize_speedup: f64,
    pub deserialize_speedup: f64,
}

/// Run benchmark comparing JSON vs Protobuf serialization
pub fn run_benchmark(
    num_state_nodes: usize,
    num_codes: usize,
    num_keys: usize,
    num_headers: usize,
    iterations: usize,
) -> BenchResult {
    let witness = generate_mock_witness(num_state_nodes, num_codes, num_keys, num_headers);
    let proto_witness = to_proto(&witness);

    // Warm up
    let _ = serde_json::to_vec(&witness);
    let _ = proto_witness.encode_to_vec();

    // Benchmark JSON serialization
    let start = Instant::now();
    let mut json_bytes = Vec::new();
    for _ in 0..iterations {
        json_bytes = serde_json::to_vec(&witness).unwrap();
    }
    let json_serialize_us = start.elapsed().as_micros() / iterations as u128;

    // Benchmark Protobuf serialization
    let start = Instant::now();
    let mut proto_bytes = Vec::new();
    for _ in 0..iterations {
        proto_bytes = proto_witness.encode_to_vec();
    }
    let proto_serialize_us = start.elapsed().as_micros() / iterations as u128;

    // Benchmark JSON deserialization
    let start = Instant::now();
    for _ in 0..iterations {
        let _: ExecutionWitness = serde_json::from_slice(&json_bytes).unwrap();
    }
    let json_deserialize_us = start.elapsed().as_micros() / iterations as u128;

    // Benchmark Protobuf deserialization
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = ExecutionWitnessResponse::decode(proto_bytes.as_slice()).unwrap();
    }
    let proto_deserialize_us = start.elapsed().as_micros() / iterations as u128;

    let json_size = json_bytes.len();
    let proto_size = proto_bytes.len();

    BenchResult {
        json_size,
        proto_size,
        json_serialize_us,
        proto_serialize_us,
        json_deserialize_us,
        proto_deserialize_us,
        size_reduction_percent: (1.0 - (proto_size as f64 / json_size as f64)) * 100.0,
        serialize_speedup: json_serialize_us as f64 / proto_serialize_us as f64,
        deserialize_speedup: json_deserialize_us as f64 / proto_deserialize_us as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_size(bytes: usize) -> String {
        if bytes >= 1_000_000 {
            format!("{:.2} MB", bytes as f64 / 1_000_000.0)
        } else if bytes >= 1_000 {
            format!("{:.2} KB", bytes as f64 / 1_000.0)
        } else {
            format!("{} B", bytes)
        }
    }

    #[test]
    fn bench_small_witness() {
        println!("\n=== Small Witness (simple transaction) ===");
        println!("  50 state nodes, 2 contracts, 20 keys, 1 header\n");

        let result = run_benchmark(50, 2, 20, 1, 100);

        println!("  Size:");
        println!("    JSON:     {}", format_size(result.json_size));
        println!("    Protobuf: {}", format_size(result.proto_size));
        println!("    Reduction: {:.1}%", result.size_reduction_percent);
        println!();
        println!("  Serialization (avg over 100 iterations):");
        println!("    JSON:     {} µs", result.json_serialize_us);
        println!("    Protobuf: {} µs", result.proto_serialize_us);
        println!("    Speedup:  {:.1}x", result.serialize_speedup);
        println!();
        println!("  Deserialization (avg over 100 iterations):");
        println!("    JSON:     {} µs", result.json_deserialize_us);
        println!("    Protobuf: {} µs", result.proto_deserialize_us);
        println!("    Speedup:  {:.1}x", result.deserialize_speedup);
    }

    #[test]
    fn bench_medium_witness() {
        println!("\n=== Medium Witness (contract interaction) ===");
        println!("  500 state nodes, 10 contracts, 200 keys, 5 headers\n");

        let result = run_benchmark(500, 10, 200, 5, 50);

        println!("  Size:");
        println!("    JSON:     {}", format_size(result.json_size));
        println!("    Protobuf: {}", format_size(result.proto_size));
        println!("    Reduction: {:.1}%", result.size_reduction_percent);
        println!();
        println!("  Serialization (avg over 50 iterations):");
        println!("    JSON:     {} µs", result.json_serialize_us);
        println!("    Protobuf: {} µs", result.proto_serialize_us);
        println!("    Speedup:  {:.1}x", result.serialize_speedup);
        println!();
        println!("  Deserialization (avg over 50 iterations):");
        println!("    JSON:     {} µs", result.json_deserialize_us);
        println!("    Protobuf: {} µs", result.proto_deserialize_us);
        println!("    Speedup:  {:.1}x", result.deserialize_speedup);
    }

    #[test]
    fn bench_large_witness() {
        println!("\n=== Large Witness (complex DeFi transaction) ===");
        println!("  2000 state nodes, 50 contracts, 1000 keys, 10 headers\n");

        let result = run_benchmark(2000, 50, 1000, 10, 20);

        println!("  Size:");
        println!("    JSON:     {}", format_size(result.json_size));
        println!("    Protobuf: {}", format_size(result.proto_size));
        println!("    Reduction: {:.1}%", result.size_reduction_percent);
        println!();
        println!("  Serialization (avg over 20 iterations):");
        println!("    JSON:     {} µs", result.json_serialize_us);
        println!("    Protobuf: {} µs", result.proto_serialize_us);
        println!("    Speedup:  {:.1}x", result.serialize_speedup);
        println!();
        println!("  Deserialization (avg over 20 iterations):");
        println!("    JSON:     {} µs", result.json_deserialize_us);
        println!("    Protobuf: {} µs", result.proto_deserialize_us);
        println!("    Speedup:  {:.1}x", result.deserialize_speedup);
    }

    #[test]
    fn bench_huge_payload() {
        use std::io::Write;
        use std::time::Instant;

        println!("\n=== Huge Payload (~300MB) Benchmark ===\n");

        // Generate ~300MB witness
        println!("  Generating 300MB test data...");
        let start = Instant::now();
        let witness = generate_mock_witness(500_000, 1000, 250_000, 200);
        println!("  Generated in {:?}\n", start.elapsed());

        let proto_witness = to_proto(&witness);

        // JSON serialization
        println!("  Serializing JSON...");
        let start = Instant::now();
        let json_bytes = serde_json::to_vec(&witness).unwrap();
        let json_ser_time = start.elapsed();

        // Protobuf serialization
        println!("  Serializing Protobuf...");
        let start = Instant::now();
        let proto_bytes = proto_witness.encode_to_vec();
        let proto_ser_time = start.elapsed();

        // Compress both
        println!("  Compressing JSON with gzip...");
        let start = Instant::now();
        let mut json_gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        json_gzip.write_all(&json_bytes).unwrap();
        let json_gzip_bytes = json_gzip.finish().unwrap();
        let json_compress_time = start.elapsed();

        println!("  Compressing Protobuf with gzip...");
        let start = Instant::now();
        let mut proto_gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        proto_gzip.write_all(&proto_bytes).unwrap();
        let proto_gzip_bytes = proto_gzip.finish().unwrap();
        let proto_compress_time = start.elapsed();

        // JSON deserialization
        println!("  Deserializing JSON...");
        let start = Instant::now();
        let _: ExecutionWitness = serde_json::from_slice(&json_bytes).unwrap();
        let json_deser_time = start.elapsed();

        // Protobuf deserialization
        println!("  Deserializing Protobuf...");
        let start = Instant::now();
        let _ = ExecutionWitnessResponse::decode(proto_bytes.as_slice()).unwrap();
        let proto_deser_time = start.elapsed();

        println!();
        println!("  ╔════════════════════════════════════════════════════════════════╗");
        println!("  ║              300MB Payload Benchmark Results                   ║");
        println!("  ╠════════════════════════════════════════════════════════════════╣");
        println!("  ║                    │      JSON      │    Protobuf   │ Speedup  ║");
        println!("  ╠════════════════════════════════════════════════════════════════╣");
        println!(
            "  ║ Uncompressed Size  │ {:>13} │ {:>13} │ {:>5.1}x   ║",
            format_size(json_bytes.len()),
            format_size(proto_bytes.len()),
            json_bytes.len() as f64 / proto_bytes.len() as f64
        );
        println!(
            "  ║ Gzip Size          │ {:>13} │ {:>13} │ {:>5.1}x   ║",
            format_size(json_gzip_bytes.len()),
            format_size(proto_gzip_bytes.len()),
            json_gzip_bytes.len() as f64 / proto_gzip_bytes.len() as f64
        );
        println!("  ╠════════════════════════════════════════════════════════════════╣");
        println!(
            "  ║ Serialize Time     │ {:>13} │ {:>13} │ {:>5.1}x   ║",
            format!("{:.2?}", json_ser_time),
            format!("{:.2?}", proto_ser_time),
            json_ser_time.as_secs_f64() / proto_ser_time.as_secs_f64()
        );
        println!(
            "  ║ Deserialize Time   │ {:>13} │ {:>13} │ {:>5.1}x   ║",
            format!("{:.2?}", json_deser_time),
            format!("{:.2?}", proto_deser_time),
            json_deser_time.as_secs_f64() / proto_deser_time.as_secs_f64()
        );
        println!(
            "  ║ Gzip Compress Time │ {:>13} │ {:>13} │ {:>5.1}x   ║",
            format!("{:.2?}", json_compress_time),
            format!("{:.2?}", proto_compress_time),
            json_compress_time.as_secs_f64() / proto_compress_time.as_secs_f64()
        );
        println!("  ╠════════════════════════════════════════════════════════════════╣");
        let json_total = json_ser_time + json_compress_time;
        let proto_total = proto_ser_time + proto_compress_time;
        println!(
            "  ║ Total (ser+gzip)   │ {:>13} │ {:>13} │ {:>5.1}x   ║",
            format!("{:.2?}", json_total),
            format!("{:.2?}", proto_total),
            json_total.as_secs_f64() / proto_total.as_secs_f64()
        );
        println!("  ╚════════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Memory/bandwidth savings with Proto+gzip vs JSON:");
        println!(
            "    - vs JSON (raw):  {:.1}% smaller ({} saved)",
            (1.0 - proto_gzip_bytes.len() as f64 / json_bytes.len() as f64) * 100.0,
            format_size(json_bytes.len() - proto_gzip_bytes.len())
        );
        println!(
            "    - vs JSON+gzip:   {:.1}% smaller ({} saved)",
            (1.0 - proto_gzip_bytes.len() as f64 / json_gzip_bytes.len() as f64) * 100.0,
            format_size(json_gzip_bytes.len() - proto_gzip_bytes.len())
        );
    }

    #[test]
    fn bench_with_compression() {
        use std::io::Write;

        println!("\n=== Compression Comparison (Large Witness) ===\n");

        let witness = generate_mock_witness(2000, 50, 1000, 10);
        let proto_witness = to_proto(&witness);

        let json_bytes = serde_json::to_vec(&witness).unwrap();
        let proto_bytes = proto_witness.encode_to_vec();

        // Compress with gzip
        let mut json_gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        json_gzip.write_all(&json_bytes).unwrap();
        let json_gzip_bytes = json_gzip.finish().unwrap();

        let mut proto_gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        proto_gzip.write_all(&proto_bytes).unwrap();
        let proto_gzip_bytes = proto_gzip.finish().unwrap();

        println!("  Format          │ Uncompressed │   Gzip     │ Gzip Ratio");
        println!("  ────────────────┼──────────────┼────────────┼───────────");
        println!(
            "  JSON            │ {:>12} │ {:>10} │ {:>8.1}%",
            format_size(json_bytes.len()),
            format_size(json_gzip_bytes.len()),
            (1.0 - json_gzip_bytes.len() as f64 / json_bytes.len() as f64) * 100.0
        );
        println!(
            "  Protobuf        │ {:>12} │ {:>10} │ {:>8.1}%",
            format_size(proto_bytes.len()),
            format_size(proto_gzip_bytes.len()),
            (1.0 - proto_gzip_bytes.len() as f64 / proto_bytes.len() as f64) * 100.0
        );
        println!();
        println!(
            "  Proto vs JSON (uncompressed): {:.1}% smaller",
            (1.0 - proto_bytes.len() as f64 / json_bytes.len() as f64) * 100.0
        );
        println!(
            "  Proto+gzip vs JSON+gzip:      {:.1}% smaller",
            (1.0 - proto_gzip_bytes.len() as f64 / json_gzip_bytes.len() as f64) * 100.0
        );
        println!(
            "  Proto+gzip vs JSON (raw):     {:.1}% smaller",
            (1.0 - proto_gzip_bytes.len() as f64 / json_bytes.len() as f64) * 100.0
        );
    }

    #[test]
    fn bench_summary_table() {
        println!("\n");
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║            JSON-RPC vs gRPC Execution Witness Benchmark                      ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        println!("║ Scenario          │ JSON Size  │ Proto Size │ Reduction │ Ser. Speed │ Deser ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");

        let scenarios = [
            ("Small (simple tx)", 50, 2, 20, 1),
            ("Medium (contract)", 500, 10, 200, 5),
            ("Large (DeFi)", 2000, 50, 1000, 10),
            ("XL (batch)", 5000, 100, 3000, 50),
        ];

        for (name, nodes, codes, keys, headers) in scenarios {
            let r = run_benchmark(nodes, codes, keys, headers, 20);
            println!(
                "║ {:17} │ {:>10} │ {:>10} │ {:>8.1}% │ {:>9.1}x │ {:>4.1}x ║",
                name,
                format_size(r.json_size),
                format_size(r.proto_size),
                r.size_reduction_percent,
                r.serialize_speedup,
                r.deserialize_speedup
            );
        }

        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
        println!();
    }
}
