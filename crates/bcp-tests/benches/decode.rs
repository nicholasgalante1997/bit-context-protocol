use bcp_decoder::BcpDecoder;
use bcp_encoder::BcpEncoder;
use bcp_types::enums::{Lang, Role, Status};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

fn bench_decode_small(c: &mut Criterion) {
    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
        .encode()
        .unwrap();

    c.bench_function("decode_small", |b| {
        b.iter(|| BcpDecoder::decode(&payload).unwrap());
    });
}

fn bench_decode_medium(c: &mut Criterion) {
    let content = b"fn placeholder() {}\n".repeat(50);
    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .add_code(Lang::TypeScript, "b.ts", &content)
        .add_conversation(Role::User, b"Review this code.")
        .add_tool_result("clippy", Status::Ok, b"warning: unused variable")
        .encode()
        .unwrap();

    c.bench_function("decode_medium", |b| {
        b.iter(|| BcpDecoder::decode(&payload).unwrap());
    });
}

fn bench_decode_compressed(c: &mut Criterion) {
    let content = b"fn placeholder() {}\n".repeat(50);

    let uncompressed = BcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .add_code(Lang::Rust, "b.rs", &content)
        .encode()
        .unwrap();

    let per_block = BcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .with_compression()
        .add_code(Lang::Rust, "b.rs", &content)
        .with_compression()
        .encode()
        .unwrap();

    let whole_payload = BcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .add_code(Lang::Rust, "b.rs", &content)
        .compress_payload()
        .encode()
        .unwrap();

    let mut group = c.benchmark_group("decode_compression");

    group.bench_function("uncompressed", |b| {
        b.iter(|| BcpDecoder::decode(&uncompressed).unwrap());
    });
    group.bench_function("per_block", |b| {
        b.iter(|| BcpDecoder::decode(&per_block).unwrap());
    });
    group.bench_function("whole_payload", |b| {
        b.iter(|| BcpDecoder::decode(&whole_payload).unwrap());
    });

    group.finish();
}

fn bench_decode_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_throughput");

    for size_kb in [1, 10, 100] {
        let content = vec![b'x'; size_kb * 1024];
        let payload = BcpEncoder::new()
            .add_code(Lang::Rust, "large.rs", &content)
            .encode()
            .unwrap();

        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("decode", format!("{size_kb}kb")),
            &payload,
            |b, p| b.iter(|| BcpDecoder::decode(p).unwrap()),
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_decode_small,
    bench_decode_medium,
    bench_decode_compressed,
    bench_decode_throughput
);
criterion_main!(benches);
