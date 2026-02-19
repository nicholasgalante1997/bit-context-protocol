use bcp_encoder::BcpEncoder;
use bcp_types::enums::{Lang, Role, Status};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

fn bench_encode_small(c: &mut Criterion) {
    let content = b"fn main() {\n    println!(\"Hello, world!\");\n}";

    c.bench_function("encode_small", |b| {
        b.iter(|| {
            BcpEncoder::new()
                .add_code(Lang::Rust, "src/main.rs", content)
                .encode()
                .unwrap()
        });
    });
}

fn bench_encode_medium(c: &mut Criterion) {
    let content = b"fn placeholder() {}\n".repeat(50);

    c.bench_function("encode_medium", |b| {
        b.iter(|| {
            BcpEncoder::new()
                .add_code(Lang::Rust, "a.rs", &content)
                .add_code(Lang::TypeScript, "b.ts", &content)
                .add_conversation(Role::User, b"Fix the bug.")
                .add_tool_result("test", Status::Ok, &content)
                .encode()
                .unwrap()
        });
    });
}

fn bench_encode_with_compression(c: &mut Criterion) {
    let content = b"fn placeholder() {}\n".repeat(50);

    let mut group = c.benchmark_group("encode_compression");

    group.bench_function("no_compression", |b| {
        b.iter(|| {
            BcpEncoder::new()
                .add_code(Lang::Rust, "a.rs", &content)
                .add_code(Lang::Rust, "b.rs", &content)
                .encode()
                .unwrap()
        });
    });

    group.bench_function("per_block", |b| {
        b.iter(|| {
            BcpEncoder::new()
                .add_code(Lang::Rust, "a.rs", &content)
                .with_compression()
                .add_code(Lang::Rust, "b.rs", &content)
                .with_compression()
                .encode()
                .unwrap()
        });
    });

    group.bench_function("whole_payload", |b| {
        b.iter(|| {
            BcpEncoder::new()
                .add_code(Lang::Rust, "a.rs", &content)
                .add_code(Lang::Rust, "b.rs", &content)
                .compress_payload()
                .encode()
                .unwrap()
        });
    });

    group.finish();
}

fn bench_encode_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_throughput");

    for size_kb in [1, 10, 100] {
        let content = vec![b'x'; size_kb * 1024];
        #[allow(clippy::cast_possible_truncation)]
        group.throughput(Throughput::Bytes((size_kb * 1024) as u64));
        group.bench_with_input(
            BenchmarkId::new("encode", format!("{size_kb}kb")),
            &content,
            |b, content| {
                b.iter(|| {
                    BcpEncoder::new()
                        .add_code(Lang::Rust, "large.rs", content)
                        .encode()
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_encode_small,
    bench_encode_medium,
    bench_encode_with_compression,
    bench_encode_throughput
);
criterion_main!(benches);
