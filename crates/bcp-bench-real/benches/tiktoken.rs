use std::path::Path;

use bcp_bench_real::fixture::encode_fixture;
use bcp_bench_real::markdown::{build_naive_markdown, build_realistic_markdown};
use bcp_bench_real::token_counter::TokenCounter;
use bcp_decoder::BcpDecoder;
use bcp_driver::{BcpDriver, DefaultDriver, DriverConfig, OutputMode};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn bench_token_counting(c: &mut Criterion) {
    let counter = TokenCounter::new().unwrap();
    let fixture_path = find_fixture("real_session_medium.json");
    let payload = encode_fixture(&fixture_path).unwrap();
    let decoded = BcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        ..Default::default()
    };
    let rendered = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    c.bench_function("tiktoken_count_medium", |b| {
        b.iter(|| counter.count(&rendered));
    });
}

fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");

    for fixture_name in [
        "real_session_small.json",
        "real_session_medium.json",
        "real_session_large.json",
    ] {
        let path = find_fixture(fixture_name);
        if !path.exists() {
            continue;
        }
        group.bench_with_input(
            BenchmarkId::from_parameter(fixture_name),
            &path,
            |b, path| {
                b.iter(|| {
                    let payload = encode_fixture(path).unwrap();
                    let decoded = BcpDecoder::decode(&payload).unwrap();
                    let config = DriverConfig {
                        mode: OutputMode::Minimal,
                        ..Default::default()
                    };
                    let rendered = DefaultDriver.render(&decoded.blocks, &config).unwrap();
                    let counter = TokenCounter::new().unwrap();
                    counter.count(&rendered)
                });
            },
        );
    }

    group.finish();
}

fn bench_savings_by_mode(c: &mut Criterion) {
    let counter = TokenCounter::new().unwrap();
    let fixture_path = find_fixture("real_session_medium.json");
    let payload = encode_fixture(&fixture_path).unwrap();
    let decoded = BcpDecoder::decode(&payload).unwrap();

    let mut group = c.benchmark_group("savings_by_mode");

    for (label, mode) in [
        ("xml", OutputMode::Xml),
        ("markdown", OutputMode::Markdown),
        ("minimal", OutputMode::Minimal),
    ] {
        let config = DriverConfig {
            mode,
            ..Default::default()
        };
        let rendered = DefaultDriver.render(&decoded.blocks, &config).unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(label), &rendered, |b, text| {
            b.iter(|| counter.count(text));
        });
    }

    // Also bench the baseline markdown construction
    let naive = build_naive_markdown(&decoded.blocks);
    group.bench_with_input(
        BenchmarkId::from_parameter("naive_md"),
        &naive,
        |b, text| {
            b.iter(|| counter.count(text));
        },
    );

    let realistic = build_realistic_markdown(&decoded.blocks);
    group.bench_with_input(
        BenchmarkId::from_parameter("agent_md"),
        &realistic,
        |b, text| {
            b.iter(|| counter.count(text));
        },
    );

    group.finish();
}

fn find_fixture(name: &str) -> std::path::PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("fixtures").join(name)
}

criterion_group!(
    benches,
    bench_token_counting,
    bench_full_pipeline,
    bench_savings_by_mode
);
criterion_main!(benches);
