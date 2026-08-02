use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rsomics_sketch::{SketchOptions, sketch};

fn construction(c: &mut Criterion) {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("sequence.fa");
    let output = directory.path().join("sequence.sig");
    let mut writer = BufWriter::new(File::create(&input).unwrap());
    writer.write_all(b">sequence\n").unwrap();
    for block in 0..262_144 {
        let bases = if block % 4_093 == 0 { b"ACGN" } else { b"ACGT" };
        writer.write_all(bases).unwrap();
    }
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    let mut group = c.benchmark_group("construction");
    group.throughput(Throughput::Bytes(1_048_576));
    group.bench_function("dna_k31_scaled1000", |benchmark| {
        benchmark.iter(|| {
            sketch(
                black_box(std::slice::from_ref(&input)),
                black_box(&output),
                black_box(&SketchOptions::default()),
            )
            .unwrap()
        });
    });
    group.finish();
}

criterion_group!(benches, construction);
criterion_main!(benches);
