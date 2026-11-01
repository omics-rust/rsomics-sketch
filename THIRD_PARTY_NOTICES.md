# Enabled dependency and Git-foundation terms

Native archives retain original texts for the actual locked normal/build
graphs of Linux/macOS x86_64/aarch64. The union contains 54 packages, including
build/procedural-macro dependencies and the exact Git foundation. Original
texts are in [licenses/registry](licenses/registry/) and
[licenses/git](licenses/git/); no generic replacement templates are used.

[licenses/provenance.json](licenses/provenance.json) binds per-target package
IDs, declared expressions/license files, 112 original registry term bodies and
their checksum-verified crate archives, plus the two original kmer Git license
bodies. The latter bind version 0.2.3 and commit
`c79111f31651bad581920b7b5c2ddde9a57534e1`, not an invented registry checksum.
Unicode/data, copyright, LLVM-exception and applicable nested terms remain
included. The provenance distinguishes 31 source/dev/other-target-only
packages from the native shipping graph.

This is not a linked-system-library inventory, legal-clearance assertion,
signing guarantee, or offline-vendor claim. Git-preserving source archives
retain the real manifest/lock; fresh source builds need public Git/registry
access or existing matching caches. Native bundles do not include the sourmash
Python package, an upstream runtime, build scratch or test executable.

The existing [ATTRIBUTION.md](ATTRIBUTION.md) and original product license
bodies are unchanged. Historical measurements remain historical; delivery
metadata does not attribute their timings to a new executable.
