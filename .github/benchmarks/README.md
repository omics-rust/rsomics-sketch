# Sketch construction measurements

The manual `Sketch construction measurements` workflow compares complete
product executables. It does not benchmark a rewritten inner-loop driver or
automatically approve publication. Production code and public APIs are not
changed by this measurement workflow.

## Controls

All three product executables use the workflow's exact sketch Git head and
Rust 1.91.0. Their foundation selections are:

- baseline: registry `rsomics-kmer 0.2.2`, with its published checksum asserted;
- reference: corrected foundation `23e42f37e9578e59d9935c0d529b07d1aac49eb9`;
- candidate: foundation `61de048cb1fce0c860beb22e78d2cdf9233aac39`.

The latter two currently have identical production source and serve as a
same-source product control. Runner-local Cargo configurations select their
Git dependencies; public product manifests retain registry dependencies.
Resolved external package identities and the external features reported by
Cargo's actual build artifacts must match the baseline before execution.
All three executables are archived from the same ordinary release-bin build,
before any test builds. Native release tests run with the candidate dependency;
the full live sourmash oracle suite explicitly executes the archived candidate
through `RSOMICS_SKETCH_BIN`, including the short-input regression. Its digest
is checked again after those tests.

The upstream is sourmash 4.9.4. Its installation report captures selected
download URLs and hashes, and `pip freeze` records the resolved environment.
When evaluating a future candidate, update the pinned revision explicitly;
the workflow does not silently select a newer foundation head.

## Complete inputs

| Workload | Source | Input SHA-256 | Warmup rounds | Measured rounds |
|---|---|---|---|---|
| Genome FASTA | [NCBI ASM584v2](https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/005/845/GCF_000005845.2_ASM584v2/GCF_000005845.2_ASM584v2_genomic.fna.gz) | `53bb6a51b6e92139ced1e38f74b7938781027c52200922ff03718c2237d23bb4` | 4 | 16 |
| Gzip FASTQ abundance | [ENA SRR341550 read 1](https://ftp.sra.ebi.ac.uk/vol1/fastq/SRR341/SRR341550/SRR341550_1.fastq.gz) | `d7a15c1762d64a5434ced0cc665d7f5d167ca81a71e239f8237b9cd490dd7683` | 1 | 8 |

The genome's source gzip is also checked against
`a96d3cfa58c88d477013c768f90a11d2386d42a70d566abfb6d9013dcbd24255`.
The 4,699,745-byte FASTA and full 6,282,141-read FASTQ are unchanged from the
historical workload identities, not downsampled replacements.

Each round runs baseline, reference, candidate and sourmash once. A four-order
balanced schedule distributes every tool equally across measurement positions
and within-round predecessor pairs. Commands use DNA `k=31`, `scaled=1000`, explicit seed 42,
and abundance only for the FASTQ workload. Every output must be nonempty and
byte-identical across tools and repeats. Absolute input paths are common to
all commands in one job; metadata paths can differ from old release outputs.

## Measurement and retention

Linux pins one allowed CPU. Both operating systems set common thread-limit
variables to one; macOS has no forced CPU affinity. These are single-worker,
warm-cache CLI measurements, not cold-start filesystem or multi-worker claims.
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
succeed. Its presence means collection completed; release approval still
requires paired time/RSS analysis, scoped upstream advantage, review of any
same-source bias or slowdown, current correctness gates and artifact retention
outside temporary runner storage. Historical August results are not reused
as a new release decision.
