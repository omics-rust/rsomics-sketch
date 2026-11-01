# Sketch construction measurements

The manual `Sketch construction measurements` workflow compares complete
product executables. It does not benchmark a rewritten inner-loop driver or
automatically approve publication. Production code and public APIs are not
changed by this measurement workflow.

## Current product profile

The workflow explicitly selects `--profile product`: two real executables,
the current-head candidate and sourmash. It does not manufacture baseline or
reference roles, change the product dependency selection, or attribute a
resource difference to a particular guard or foundation change.

The candidate uses the exact workflow Git head, Rust 1.91.0 and the ordinary
locked manifest: product 0.1.1 with Git `rsomics-kmer 0.2.3` at
`c79111f31651bad581920b7b5c2ddde9a57534e1`. One normal release-bin build is
archived before any test-feature build. Cargo metadata, exact source TAR and
lockfile, actual compiled external dependency features, compiler/native host
and executable digests bind that binary. Native release tests and all four
live sourmash oracles execute the archived candidate through
`RSOMICS_SKETCH_BIN`, including short input. Its digest is checked after tests
and after measurements. No test-feature binary is substituted.

The upstream is sourmash 4.9.4. Its installation report captures selected
download URLs and hashes, and `pip freeze` records the resolved environment.
The manifest supplies the immutable foundation revision; no runner-local
Cargo patch or registry fallback silently selects a different source.

The recorder's default `legacy` profile retains the historical four-role
protocol and balanced schedule for interpreting old experiments. Existing
raw controls/results are not rewritten, reclassified as this product gate,
or evidence that this final binary was measured.

## Complete inputs

| Workload | Source | Input bytes | Input SHA-256 | Warmup rounds | Measured rounds |
|---|---|---|---|---|---|
| Genome FASTA | [NCBI ASM584v2](https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/005/845/GCF_000005845.2_ASM584v2/GCF_000005845.2_ASM584v2_genomic.fna.gz) | 4,699,745 | `53bb6a51b6e92139ced1e38f74b7938781027c52200922ff03718c2237d23bb4` | 4 | 16 |
| Gzip FASTQ abundance | [ENA SRR341550 read 1](https://ftp.sra.ebi.ac.uk/vol1/fastq/SRR341/SRR341550/SRR341550_1.fastq.gz) | 87,439,836 | `d7a15c1762d64a5434ced0cc665d7f5d167ca81a71e239f8237b9cd490dd7683` | 1 | 8 |

The genome's source gzip is also checked against
`a96d3cfa58c88d477013c768f90a11d2386d42a70d566abfb6d9013dcbd24255`.
The 4,699,745-byte FASTA and full 6,282,141-read FASTQ are unchanged from the
historical workload identities, not downsampled replacements.

Each product round runs candidate and sourmash once, alternating their order.
Even measured round counts balance the two positions. Commands use DNA
`k=31`, `scaled=1000`, explicit seed 42,
and abundance only for the FASTQ workload. Every output must be nonempty and
byte-identical across tools and repeats. Absolute input paths are common to
all commands in one job; metadata paths can differ from old release outputs.

## Measurement and retention

The eight-job matrix covers both complete workloads on native Linux/macOS,
x86_64/aarch64. Each command measures one CLI process. Linux pins all of its
threads to one actual allowed CPU and records that CPU; macOS has no forced
CPU affinity and is not an equivalent one-CPU budget.

OMP/OpenBLAS/MKL/Rayon environment limits are one, not a cap on total process
threads. The locked `rsomics-seqio 0.4.0` gzip path starts a separate
`seqio-gzip` decoder thread independently of those limits. It shares the one
allowed CPU on Linux but may overlap sketching across CPUs on macOS. Total
thread counts and matching cross-tool thread counts are not measured or
claimed. Wall/user/system time and process RSS describe the actual CLI
configuration, not a single-thread kernel comparison. These are warm-cache
measurements, not exclusive-host, cold-start filesystem or scaling claims.
Every command writes to a fresh directory. No input or previous output is
deleted, and an existing evidence directory is rejected even when empty.

The recorder uses native `/usr/bin/time` for wall time, user/system CPU time
and peak RSS. Linux reports RSS in KiB, converted to bytes; macOS reports bytes.
Raw resource output is retained so normalization remains auditable. Apple's
[time implementation](https://github.com/apple-oss-distributions/shell_cmds/blob/main/time/time.c)
documents its separate statistics file and child resource collection. Timing
resolution is that of the native command, not a nanosecond precision claim.

Artifacts contain binaries, Cargo manifests/lockfiles/resolved metadata,
compiler artifact messages and normalized compiled dependency features,
source and host provenance, input URL/HTTP headers, recorder source, per-trial
commands/stdout/stderr/resource logs, every signature, and incremental JSONL
observations. The input files themselves remain reproducible from their
pinned sources and hashes. Observations are also printed in CI logs.

`complete.json` is written only after all commands and byte comparisons
succeed; it is a collection receipt, not release approval. The workflow then
writes `paired-summary.json` with each measured pair and each tool's wall,
user/system CPU, process peak RSS and input-byte throughput distributions
(count, mean, median, sample standard deviation, minimum and maximum).
Input-byte throughput uses compressed bytes for the gzip workload.

The resource gate requires strictly lower candidate process peak RSS in
every measured pair in every one of the eight jobs. Equality or one losing
pair fails that job; the paired summary and raw outcomes remain available.
Wall-time losses stay explicit rather than being hidden behind the memory
advantage. Final acceptance still requires the exact-head native/source/install
gate, original artifact adoption, review and retention outside temporary
runner storage. Historical August results are not reused as a new release
decision; inspect/compare/search performance is not measured here.
