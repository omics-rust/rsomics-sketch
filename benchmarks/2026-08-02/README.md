# Release performance evidence

## Decision

Release 0.1.0 meets the replacement gate through both a strict throughput
advantage on the representative genome workflow and a strict resource-use
advantage on both measured workflows. The gzip FASTQ abundance case is not a
throughput win and is recorded as an optimization target rather than hidden.

Both comparisons emitted byte-identical sourmash 0.4 signature JSON. This
benchmark establishes performance for sketch construction only; it does not
make performance claims for inspect, compare, or search.

## Environment

- Date: 2026-08-02
- Machine: Apple M2, arm64
- Operating system: macOS 26.6 (25G72)
- Rust: 1.97.1, release profile
- rsomics-sketch: 0.1.0
- rsomics-kmer: 0.2.2
- sourmash: 4.9.4
- sourmash source tag: 4.9.4 (`f5881ac`)
- Timing: hyperfine 1.20.0 for the genome, `/usr/bin/time -lp` for RSS and
  the secondary FASTQ sample

All inputs, build output, command output, and scratch files resided on the
external disks described by the project operating manual.

## Genome construction

Input: NCBI RefSeq *Escherichia coli* K-12 MG1655 assembly ASM584v2 genomic
FASTA, 4,699,745 bytes.

- FASTA SHA-256: `53bb6a51b6e92139ced1e38f74b7938781027c52200922ff03718c2237d23bb4`
- Source gzip SHA-256: `a96d3cfa58c88d477013c768f90a11d2386d42a70d566abfb6d9013dcbd24255`
- Output SHA-256 from both tools: `79299a40184a8b5c1bb119c72eff6ca60aa3620d3dde796600c51b5d62ba569d`
- Parameters: DNA, `k=31`, `scaled=1000`, seed 42, no abundance
- Timing protocol: three warmups and 15 measured runs per command
- RSS protocol: five separate `/usr/bin/time -lp` runs per command

| Tool | Mean s | SD s | Median s | Range s | Mean peak RSS bytes |
|---|---:|---:|---:|---:|---:|
| sourmash 4.9.4 | 1.035143 | 0.036239 | 1.024030 | 1.001233-1.148116 | 134,955,008 |
| rsomics-sketch 0.1.0 | 0.813499 | 0.074179 | 0.780546 | 0.761589-0.995140 | 9,682,944 |

Commands:

```console
sourmash sketch dna -f -p k=31,scaled=1000,noabund -o sourmash.sig GCF_000005845.2_ASM584v2_genomic.fna
rsomics-sketch sketch GCF_000005845.2_ASM584v2_genomic.fna --ksize 31 --scaled 1000 -o rsomics.sig
```

The same commands were wrapped individually by hyperfine for elapsed time and
by `/usr/bin/time -lp` for resident memory. Output paths were removed before
each invocation.

## Gzip FASTQ abundance construction

Input: SRR341550 read 1, 6,282,141 reads, gzip-compressed.

- Input SHA-256: `d7a15c1762d64a5434ced0cc665d7f5d167ca81a71e239f8237b9cd490dd7683`
- Output SHA-256 from both tools: `804eda4e4dca0fd5d5636ce238fab4204a5219c56302cc873c0768be0659cd87`
- Parameters: DNA, `k=31`, `scaled=1000`, seed 42, abundance
- Protocol: one paired release-gate sample per command

| Tool | Elapsed s | User s | System s | Peak RSS bytes |
|---|---:|---:|---:|---:|
| sourmash 4.9.4 | 43.29 | 38.21 | 2.40 | 108,118,016 |
| rsomics-sketch 0.1.0 | 51.04 | 46.64 | 2.83 | 9,093,120 |

Commands:

```console
sourmash sketch dna -f -p k=31,scaled=1000,abund -o sourmash-fastq.sig SRR341550_1.fastq.gz
rsomics-sketch sketch SRR341550_1.fastq.gz --ksize 31 --scaled 1000 --abundance -o rsomics-fastq.sig
```

The FASTQ timing is deliberately labelled as a single sample. It is sufficient
to detect the current throughput regression, not to estimate a stable timing
distribution.
