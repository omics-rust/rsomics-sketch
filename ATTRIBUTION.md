# Attribution and compatibility sources

The initial compatibility profile was validated against sourmash 4.9.4,
copyright the sourmash authors and distributed under the BSD 3-Clause
License. The relevant public contracts are its DNA FracMinHash construction,
signature format version 0.4, signature digest, comparison matrix, and linear
search behavior. See <https://github.com/sourmash-bio/sourmash> and
<https://sourmash.readthedocs.io/en/stable/>.

`rsomics-sketch` is an independent Rust implementation. It does not bundle the
sourmash Python package or copy sourmash source files. Live compatibility tests
install the pinned upstream release as an external oracle.

Mash 2.3 documentation informs the later fixed-size sketch dossier, but the
0.1 release does not implement or claim `.msh` interoperability.
