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
committed transactionally; parse, validation and write failures before the
rename/commit leave an existing named destination unchanged. A directory-sync
error after commit can report failure after the destination has changed; no
post-commit rollback is promised. A `.gz` output suffix writes a gzip-compressed
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

## Corrected source/native delivery candidate

The 0.1.1 candidate retains the four commands and existing signature/API
contracts. It adopts `rsomics-kmer 0.2.3` from the exact public Git revision
`c79111f31651bad581920b7b5c2ddde9a57534e1`, including the short-input iterator
repair. Scale recovery and collection-wide comparison retain their existing
repairs. Records shorter than `k` contribute no windows. The strict parser
still rejects an empty FASTA sequence; valid empty FASTQ records can contribute
no windows, while a file with no sequence records remains an input error.
No producer or sequence-utility release is implied by this product adoption.

Registry 0.1.0 is not this corrected candidate; the required kmer 0.2.3 is not
registry-published. This product sets `publish = false`. GitHub release and
fresh public installation remain pending native/source/install qualification,
current-head construction measurements, original adoption and root's public
action. No available 0.1.1 release asset is implied here.

The delivery shape is an exact Git-preserving `*-source.tar` with `SOURCE.json`
and one binary archive per native Linux/macOS x86_64/aarch64 class, with
original terms, `BUILD.json` and checksums. The source TAR is not a normalized
Cargo registry crate and does not vendor dependencies. Building its unchanged
manifest/lock with `cargo install --path . --locked` needs access to the pinned
public Git source and registry downloads or corresponding caches. Local project
work uses `scripts/guarded-cargo.sh` and explicitly external installation/output
paths; its trusted `--exec` mode is not an arbitrary-tool sandbox.

Native host/toolchain and Git/lock identities are recorded, not minimum-OS,
signing or system-runtime clearance. Original dependency and Git-foundation
terms and scope are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
[licenses/provenance.json](licenses/provenance.json).

### Linux ARM CI oracle runtime

Only the Linux aarch64 CI reference uses the private `scripts/ci-sourmash`
launcher from the source archive. It retains the
official sourmash 4.9.4 wheel's `lib_lowlevel.so` with `RTLD_NODELETE`, then
forwards arguments and exit status through the original console entry. The
upstream entry, loader, library and scientific functions are not patched;
other native classes call sourmash directly. This is a CI oracle dependency,
not a component bundled into the native product binary.

The initial native run `41353788925` passed three classes but failed Linux
ARM at oracle process exit. Two-Python run `41354486584` and exit trace
`41355005538` retained that failure; focused lifetime contrast `41356083581`
placed the fatal address inside the previously loaded library and showed
successful version/sketch exits with library retention and unchanged upstream
bytes. These diagnostics support this bounded runtime workaround, not a
general Rust/CFFI cause, a new native-four pass, or performance acceptance.
Runtime records verify the official ARM loader/library hashes and retain
entry/launcher identities; measurements compare those records before/after.

## Performance

The following results describe the historical 0.1.0 build, not a new corrected
delivery binary. The final-head construction gate is pending. Its current
product profile uses only two real roles (candidate and sourmash 4.9.4), both
complete inputs, and all four native classes; it requires lower candidate
process RSS in every measured pair while retaining all throughput losses.
There is no guard-causal, universal speed or inspect/compare/search performance
claim. See the [current measurement protocol](.github/benchmarks/README.md).
The Linux ARM reference includes launcher startup and shared-library retention
in its measured wall time and process RSS; it is the configured CLI process,
not a pure-kernel or equal-thread comparison.

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
