use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(program: impl AsRef<OsStr>, arguments: &[&OsStr]) {
    let output = Command::new(program).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sourmash() -> String {
    std::env::var("SOURMASH").expect("set SOURMASH to the pinned sourmash 4.9.4 executable")
}

fn rsomics() -> String {
    std::env::var_os("RSOMICS_SKETCH_BIN")
        .map(|path| {
            path.into_string()
                .expect("RSOMICS_SKETCH_BIN must be UTF-8")
        })
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_rsomics-sketch").to_owned())
}

fn sketch_with(program: &str, input: &Path, output: &Path, parameter: &str) {
    if program.ends_with("sourmash") {
        run(
            program,
            &[
                OsStr::new("sketch"),
                OsStr::new("dna"),
                OsStr::new("-p"),
                OsStr::new(parameter),
                OsStr::new("-o"),
                output.as_os_str(),
                input.as_os_str(),
            ],
        );
    } else {
        let mut arguments: Vec<&OsStr> = vec![
            OsStr::new("sketch"),
            input.as_os_str(),
            OsStr::new("-o"),
            output.as_os_str(),
        ];
        for component in parameter.split(',') {
            if let Some(value) = component.strip_prefix("k=") {
                arguments.extend([OsStr::new("--ksize"), OsStr::new(value)]);
            } else if let Some(value) = component.strip_prefix("scaled=") {
                arguments.extend([OsStr::new("--scaled"), OsStr::new(value)]);
            } else if let Some(value) = component.strip_prefix("seed=") {
                arguments.extend([OsStr::new("--seed"), OsStr::new(value)]);
            } else if component == "abund" {
                arguments.push(OsStr::new("--abundance"));
            }
        }
        run(program, &arguments);
    }
}

#[test]
#[ignore = "requires the pinned live sourmash oracle"]
fn generated_signatures_are_byte_identical() {
    let sourmash = sourmash();
    let rsomics = &rsomics();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("random.fa");
    let mut state = 0x9e37_79b9_u64;
    let alphabet = b"ACGTacgtN";
    let mut sequence = Vec::with_capacity(20_000);
    for _ in 0..20_000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        sequence.push(alphabet[(state as usize) % alphabet.len()]);
    }
    let mut fasta = b">generated\n".to_vec();
    fasta.extend_from_slice(&sequence);
    fasta.push(b'\n');
    fs::write(&input, fasta).unwrap();

    for (index, (parameter, scaled)) in [
        ("k=1,scaled=1,noabund", 1),
        ("k=31,scaled=10,noabund", 10),
        ("k=51,scaled=100,abund", 100),
        ("k=17,scaled=7,noabund,seed=4294967297", 7),
        ("k=17,scaled=93,abund", 93),
        ("k=17,scaled=3583599928,noabund", 3_583_599_928),
    ]
    .into_iter()
    .enumerate()
    {
        let expected = directory.path().join(format!("expected-{index}.sig"));
        let actual = directory.path().join(format!("actual-{index}.sig"));
        sketch_with(&sourmash, &input, &expected, parameter);
        sketch_with(rsomics, &input, &actual, parameter);
        assert_eq!(fs::read(&actual).unwrap(), fs::read(expected).unwrap());
        let loaded = rsomics_sketch::load_signatures(&actual).unwrap();
        assert_eq!(loaded[0].sketch().scaled(), scaled);
    }

    let second = directory.path().join("second.fa");
    fs::write(&second, b">second\nTTTTACGTACGTACGTACGTACGTACGTACGT\n").unwrap();
    let expected = directory.path().join("expected-collection.sig");
    run(
        &sourmash,
        &[
            OsStr::new("sketch"),
            OsStr::new("dna"),
            OsStr::new("-p"),
            OsStr::new("k=17,scaled=7,noabund"),
            OsStr::new("-o"),
            expected.as_os_str(),
            input.as_os_str(),
            second.as_os_str(),
        ],
    );
    let actual = directory.path().join("actual-collection.sig");
    run(
        rsomics,
        &[
            OsStr::new("sketch"),
            input.as_os_str(),
            second.as_os_str(),
            OsStr::new("--ksize"),
            OsStr::new("17"),
            OsStr::new("--scaled"),
            OsStr::new("7"),
            OsStr::new("-o"),
            actual.as_os_str(),
        ],
    );
    assert_eq!(fs::read(actual).unwrap(), fs::read(expected).unwrap());
}

#[test]
#[ignore = "requires the pinned live sourmash oracle"]
fn short_sequences_match_sourmash() {
    let sourmash = sourmash();
    let rsomics = &rsomics();
    let directory = tempfile::tempdir().unwrap();
    for k in [31, 51] {
        for extension in ["fa", "fq"] {
            for (name, lengths) in [
                ("mixed", vec![3, k, k + 1, 2]),
                ("all-short", vec![3, k - 1, 2]),
            ] {
                let input = directory.path().join(format!("{name}-{k}.{extension}"));
                let mut records = String::new();
                for (index, length) in lengths.into_iter().enumerate() {
                    let sequence = ["A", "C", "G", "N"][index].repeat(length);
                    if extension == "fa" {
                        records.push_str(&format!(">record-{index}\n{sequence}\n"));
                    } else {
                        let quality = "I".repeat(length);
                        records.push_str(&format!("@record-{index}\n{sequence}\n+\n{quality}\n"));
                    }
                }
                fs::write(&input, records).unwrap();
                let expected = directory.path().join("expected.sig");
                let actual = directory.path().join("actual.sig");
                let parameter = format!("k={k},scaled=1,abund");
                sketch_with(&sourmash, &input, &expected, &parameter);
                sketch_with(rsomics, &input, &actual, &parameter);
                assert_eq!(fs::read(actual).unwrap(), fs::read(expected).unwrap());
            }
        }
    }
}

#[test]
#[ignore = "requires the pinned live sourmash oracle"]
fn comparison_and_search_scores_match() {
    let sourmash = sourmash();
    let rsomics = &rsomics();
    let directory = tempfile::tempdir().unwrap();
    let sequences = [
        ("query", b"ACGTTGCATGTCGCATGATGCATGAGAGCT".as_slice()),
        ("related", b"ACGTTGCATGTCGCATGATGCATGAAAAA".as_slice()),
        ("other", b"TTTTTTTTTTTTTTTTTTTTTTTTTTTTTT".as_slice()),
    ];
    let mut signatures = Vec::<PathBuf>::new();
    for (name, sequence) in sequences {
        let input = directory.path().join(format!("{name}.fa"));
        let mut fasta = format!(">{name}\n").into_bytes();
        fasta.extend_from_slice(sequence);
        fasta.push(b'\n');
        fs::write(&input, fasta).unwrap();
        let signature = directory.path().join(format!("{name}.sig"));
        sketch_with(&sourmash, &input, &signature, "k=3,scaled=1,noabund");
        signatures.push(signature);
    }

    let expected_matrix = directory.path().join("expected.csv");
    run(
        &sourmash,
        &[
            OsStr::new("compare"),
            OsStr::new("--containment"),
            OsStr::new("--csv"),
            expected_matrix.as_os_str(),
            signatures[0].as_os_str(),
            signatures[1].as_os_str(),
            signatures[2].as_os_str(),
        ],
    );
    let actual_matrix = directory.path().join("actual.tsv");
    run(
        rsomics,
        &[
            OsStr::new("compare"),
            signatures[0].as_os_str(),
            signatures[1].as_os_str(),
            signatures[2].as_os_str(),
            OsStr::new("--shape"),
            OsStr::new("matrix"),
            OsStr::new("--matrix-measure"),
            OsStr::new("containment"),
            OsStr::new("-o"),
            actual_matrix.as_os_str(),
        ],
    );
    let expected = numeric_matrix(&expected_matrix, b',', false);
    let actual = numeric_matrix(&actual_matrix, b'\t', true);
    assert_eq!(actual, expected);

    let expected_search = directory.path().join("expected-search.csv");
    run(
        &sourmash,
        &[
            OsStr::new("search"),
            OsStr::new("-t"),
            OsStr::new("0"),
            OsStr::new("-n"),
            OsStr::new("0"),
            OsStr::new("-o"),
            expected_search.as_os_str(),
            signatures[0].as_os_str(),
            signatures[1].as_os_str(),
            signatures[2].as_os_str(),
        ],
    );
    let actual_search = directory.path().join("actual-search.tsv");
    run(
        rsomics,
        &[
            OsStr::new("search"),
            signatures[0].as_os_str(),
            signatures[1].as_os_str(),
            signatures[2].as_os_str(),
            OsStr::new("-t"),
            OsStr::new("0"),
            OsStr::new("-o"),
            actual_search.as_os_str(),
        ],
    );
    assert_eq!(
        search_scores(&actual_search, b'\t'),
        search_scores(&expected_search, b',')
    );
}

fn numeric_matrix(path: &Path, delimiter: u8, row_labels: bool) -> Vec<Vec<f64>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .from_path(path)
        .unwrap();
    reader
        .records()
        .map(|record| {
            record
                .unwrap()
                .iter()
                .skip(usize::from(row_labels))
                .map(|value| value.parse().unwrap())
                .collect()
        })
        .collect()
}

fn search_scores(path: &Path, delimiter: u8) -> Vec<(String, String)> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .from_path(path)
        .unwrap();
    let headers = reader.headers().unwrap().clone();
    let score = headers
        .iter()
        .position(|value| value == "score")
        .unwrap_or(0);
    let digest = headers
        .iter()
        .position(|value| value == "target_md5" || value == "md5")
        .unwrap();
    reader
        .records()
        .map(|record| {
            let record = record.unwrap();
            (record[score].to_owned(), record[digest].to_owned())
        })
        .collect()
}
