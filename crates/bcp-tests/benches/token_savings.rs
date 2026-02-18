use bcp_decoder::LcpDecoder;
use bcp_driver::{
    CodeAwareEstimator, DefaultDriver, DriverConfig, HeuristicEstimator, LcpDriver, OutputMode,
    TokenEstimator,
};
use bcp_encoder::LcpEncoder;
use bcp_types::enums::{Lang, Role, Status};
use criterion::{Criterion, criterion_group, criterion_main};

fn representative_payload() -> Vec<u8> {
    let code = b"fn process(input: &[u8]) -> Vec<u8> {\n    input.iter().map(|b| b.wrapping_add(1)).collect()\n}\n";
    LcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", code)
        .add_code(Lang::Rust, "src/lib.rs", code)
        .add_code(
            Lang::TypeScript,
            "src/index.ts",
            b"export function process(input: Uint8Array): Uint8Array {\n    return input.map(b => (b + 1) & 0xFF);\n}\n",
        )
        .add_conversation(Role::User, b"Add error handling for empty input.")
        .add_tool_result(
            "cargo_test",
            Status::Ok,
            b"test result: ok. 5 passed; 0 failed",
        )
        .encode()
        .unwrap()
}

fn bench_estimate_heuristic(c: &mut Criterion) {
    let payload = representative_payload();
    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();
    let estimator = HeuristicEstimator;

    c.bench_function("estimate_heuristic", |b| {
        b.iter(|| estimator.estimate(&output));
    });
}

fn bench_estimate_code_aware(c: &mut Criterion) {
    let payload = representative_payload();
    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();
    let estimator = CodeAwareEstimator;

    c.bench_function("estimate_code_aware", |b| {
        b.iter(|| estimator.estimate(&output));
    });
}

fn bench_full_pipeline(c: &mut Criterion) {
    let code = b"fn main() { println!(\"hello\"); }\n";

    c.bench_function("full_pipeline", |b| {
        b.iter(|| {
            let payload = LcpEncoder::new()
                .add_code(Lang::Rust, "main.rs", code)
                .add_conversation(Role::User, b"What does this do?")
                .encode()
                .unwrap();

            let decoded = LcpDecoder::decode(&payload).unwrap();
            let config = DriverConfig {
                mode: OutputMode::Minimal,
                ..Default::default()
            };
            let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();
            HeuristicEstimator.estimate(&output)
        });
    });
}

criterion_group!(
    benches,
    bench_estimate_heuristic,
    bench_estimate_code_aware,
    bench_full_pipeline
);
criterion_main!(benches);
