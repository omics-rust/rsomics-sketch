# rsomics-sketch

`rsomics-sketch` builds persistent DNA FracMinHash signatures, validates and
inspects them, compares compatible signatures, and performs deterministic
linear search. Its initial file profile is semantically interoperable with
sourmash 4.9.4 signature version 0.4.

The product is intentionally narrower than sourmash or Mash. It currently
supports canonical DNA Murmur64 FracMinHash signatures, optional abundance,
Jaccard and directional containment, containment-root ANI estimates, pair and
matrix output, and linear search. Indexes, gather, protein sketches, fixed-size
Mash sketches, and `.msh` files are not advertised yet.

```console
rsomics-sketch sketch genome.fa -o genome.sig
rsomics-sketch inspect genome.sig -o genome.tsv
rsomics-sketch compare genome.sig related.sig -o pairs.tsv
rsomics-sketch compare genome.sig related.sig -o matrix.tsv --shape matrix
rsomics-sketch search genome.sig references.sig -o matches.tsv --threshold 0.1
```

FASTA and FASTQ inputs may be plain, gzip, or BGZF. Signature output is
committed transactionally; a failed parse or write leaves an existing named
destination unchanged. A `.gz` output suffix writes a gzip-compressed
signature. Sequence bytes are normalized case-insensitively to A/C/G/T.
Ambiguous windows are skipped, matching the default sourmash DNA sketch
profile; `--check-sequence` instead rejects the first invalid byte.

Loading preserves the serialized hash threshold and recovers the integer
`scaled` value that created it. This corrects a sourmash 4.9.4 metadata edge
case: its floating-point inverse can report 92 for a signature made with
`scaled=93`. Signature bytes remain interoperable; exact metadata or mixed-scale
comparison equivalence is not claimed where that upstream truncation occurs.

Comparison uses the coarsest `scaled` value across the whole input collection;
search downsamples each query/target pair to their coarser value.
Both reject mismatched `k`, seed, or abundance profiles before writing
the destination. ANI columns are explicitly named as the directional
containment-root estimator and are not Mash distance estimates.
Containment matrices follow sourmash orientation: each cell is the fraction of
the column signature contained in the row signature.

## Performance

On the 4.70 Mbp *E. coli* K-12 MG1655 reference, `sketch` at `k=31` and
`scaled=1000` produced a byte-identical signature in 0.813 s versus 1.035 s
for sourmash 4.9.4 (15 measured runs, 1.27x throughput). Mean peak RSS from
five separate runs was 9.68 MB versus 134.96 MB (92.8% lower).

The tradeoff is workload-dependent. On 6,282,141 gzip-compressed FASTQ reads
with abundance tracking, one release-gate run took 51.04 s versus 43.29 s
(17.9% slower), while peak RSS was 9.09 MB versus 108.12 MB (91.6% lower).
The two tools produced byte-identical signatures in both cases. Commands,
input digests, per-run values, and the performance decision are recorded in
[`benchmarks/2026-08-02`](benchmarks/2026-08-02/README.md).

## License

MIT OR Apache-2.0.

Compatibility sources and upstream attribution are recorded in
[`ATTRIBUTION.md`](ATTRIBUTION.md).
