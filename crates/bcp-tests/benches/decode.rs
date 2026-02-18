use bcp_decoder::LcpDecoder;
use bcp_encoder::LcpEncoder;
use bcp_types::enums::{Lang, Role, Status};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

fn bench_decode_small(c: &mut Criterion) {
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
        .encode()
        .unwrap();

    c.bench_function("decode_small", |b| {
        b.iter(|| LcpDecoder::decode(&payload).unwrap());
    });
}

fn bench_decode_medium(c: &mut Criterion) {
    let content = b"fn placeholder() {}\n".repeat(50);
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .add_code(Lang::TypeScript, "b.ts", &content)
        .add_conversation(Role::User, b"Review this code.")
        .add_tool_result("clippy", Status::Ok, b"warning: unused variable")
        .encode()
        .unwrap();

    c.bench_function("decode_medium", |b| {
        b.iter(|| LcpDecoder::decode(&payload).unwrap());
    });
}

fn bench_decode_compressed(c: &mut Criterion) {
    let content = b"fn placeholder() {}\n".repeat(50);

    let uncompressed = LcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .add_code(Lang::Rust, "b.rs", &content)
        .encode()
        .unwrap();

    let per_block = LcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .with_compression()
        .add_code(Lang::Rust, "b.rs", &content)
        .with_compression()
        .encode()
        .unwrap();

    let whole_payload = LcpEncoder::new()
        .add_code(Lang::Rust, "a.rs", &content)
        .add_code(Lang::Rust, "b.rs", &content)
        .compress_payload()
        .encode()
        .unwrap();

    let mut group = c.benchmark_group("decode_compression");

    group.bench_function("uncompressed", |b| {
        b.iter(|| LcpDecoder::decode(&uncompressed).unwrap());
    });
    group.bench_function("per_block", |b| {
        b.iter(|| LcpDecoder::decode(&per_block).unwrap());
    });
    group.bench_function("whole_payload", |b| {
        b.iter(|| LcpDecoder::decode(&whole_payload).unwrap());
    });

    group.finish();
}

fn bench_decode_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_throughput");

    for size_kb in [1, 10, 100] {
        let content = vec![b'x'; size_kb * 1024];
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "large.rs", &content)
            .encode()
            .unwrap();

        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("decode", format!("{size_kb}kb")),
            &payload,
            |b, p| b.iter(|| LcpDecoder::decode(p).unwrap()),
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
