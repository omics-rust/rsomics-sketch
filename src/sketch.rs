use std::collections::HashMap;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};

use rsomics_common::{Context, Result, RsomicsError, reject_output_alias};
use rsomics_kmer::CanonicalMurmur64;
use serde::Serialize;

use crate::signature::{Signature, max_hash_for_scaled, write_signatures};

#[derive(Clone, Debug)]
pub struct SketchOptions {
    pub ksize: NonZeroU32,
    pub scaled: NonZeroU32,
    pub seed: u64,
    pub abundance: bool,
    pub check_sequence: bool,
    pub name: Option<String>,
}

impl Default for SketchOptions {
    fn default() -> Self {
        Self {
            ksize: NonZeroU32::new(31).unwrap(),
            scaled: NonZeroU32::new(1_000).unwrap(),
            seed: 42,
            abundance: false,
            check_sequence: false,
            name: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SketchReport {
    pub inputs: usize,
    pub signatures: usize,
    pub records: u64,
    pub bases: u64,
    pub windows: u64,
    pub invalid_windows: u64,
    pub retained_hashes: usize,
    pub output: PathBuf,
}

struct BuiltSignature {
    signature: Signature,
    records: u64,
    bases: u64,
    windows: u64,
    invalid_windows: u64,
}

pub fn sketch(inputs: &[PathBuf], output: &Path, options: &SketchOptions) -> Result<SketchReport> {
    if inputs.is_empty() {
        return Err(RsomicsError::InvalidInput(
            "at least one sequence input is required".to_owned(),
        ));
    }
    reject_output_alias(output, inputs.iter().map(PathBuf::as_path))?;
    if options.name.is_some() && inputs.len() != 1 {
        return Err(RsomicsError::InvalidInput(
            "--name requires exactly one input".to_owned(),
        ));
    }
    if options.name.as_ref().is_some_and(String::is_empty) {
        return Err(RsomicsError::InvalidInput(
            "signature name cannot be empty".to_owned(),
        ));
    }
    let mut signatures = Vec::with_capacity(inputs.len());
    let mut records = 0_u64;
    let mut bases = 0_u64;
    let mut windows = 0_u64;
    let mut invalid_windows = 0_u64;
    let mut retained_hashes = 0_usize;
    for input in inputs {
        let built = build(input, options)?;
        records = records.checked_add(built.records).ok_or_else(|| {
            RsomicsError::InvalidInput("record count exceeds u64 range".to_owned())
        })?;
        bases = bases
            .checked_add(built.bases)
            .ok_or_else(|| RsomicsError::InvalidInput("base count exceeds u64 range".to_owned()))?;
        windows = windows.checked_add(built.windows).ok_or_else(|| {
            RsomicsError::InvalidInput("window count exceeds u64 range".to_owned())
        })?;
        invalid_windows = invalid_windows
            .checked_add(built.invalid_windows)
            .ok_or_else(|| {
                RsomicsError::InvalidInput("invalid-window count exceeds u64 range".to_owned())
            })?;
        retained_hashes = retained_hashes
            .checked_add(built.signature.sketch().hashes().len())
            .ok_or_else(|| {
                RsomicsError::InvalidInput("retained-hash count exceeds usize range".to_owned())
            })?;
        signatures.push(built.signature);
    }
    write_signatures(output, &signatures)?;
    Ok(SketchReport {
        inputs: inputs.len(),
        signatures: signatures.len(),
        records,
        bases,
        windows,
        invalid_windows,
        retained_hashes,
        output: output.to_owned(),
    })
}

fn build(path: &Path, options: &SketchOptions) -> Result<BuiltSignature> {
    let filename = path.to_str().ok_or_else(|| {
        RsomicsError::InvalidInput(format!("input path is not valid UTF-8: {}", path.display()))
    })?;
    let k = usize::try_from(options.ksize.get()).map_err(|_| {
        RsomicsError::InvalidInput(format!("k={} does not fit this platform", options.ksize))
    })?;
    let mut hasher = CanonicalMurmur64::try_new(
        NonZeroUsize::new(k).expect("nonzero u32 remains nonzero"),
        options.seed,
    )
    .map_err(|error| RsomicsError::ConfigError(error.to_string()))?;
    let max_hash = max_hash_for_scaled(options.scaled.get());
    let mut retained = HashMap::<u64, u64>::new();
    let mut reader = rsomics_seqio::open_path(path)?;
    let mut records = 0_u64;
    let mut bases = 0_u64;
    let mut windows = 0_u64;
    let mut invalid_windows = 0_u64;
    while let Some(record) = reader
        .read_record()
        .rs_with_context(|| format!("parsing {}", path.display()))?
    {
        records = records.checked_add(1).ok_or_else(|| {
            RsomicsError::InvalidInput("record count exceeds u64 range".to_owned())
        })?;
        bases = bases
            .checked_add(u64::try_from(record.seq.len()).map_err(|_| {
                RsomicsError::InvalidInput("sequence length exceeds u64 range".to_owned())
            })?)
            .ok_or_else(|| RsomicsError::InvalidInput("base count exceeds u64 range".to_owned()))?;
        if options.check_sequence
            && let Some((position, byte)) =
                record.seq.iter().copied().enumerate().find(|(_, base)| {
                    !matches!(base.to_ascii_uppercase(), b'A' | b'C' | b'G' | b'T')
                })
        {
            return Err(RsomicsError::InvalidInput(format!(
                "{} record {:?} has non-ACGT byte {byte:?} at position {position}",
                path.display(),
                String::from_utf8_lossy(record.id)
            )));
        }
        for hash in hasher.hashes(record.seq) {
            windows = windows.checked_add(1).ok_or_else(|| {
                RsomicsError::InvalidInput("window count exceeds u64 range".to_owned())
            })?;
            let Some(hash) = hash else {
                invalid_windows = invalid_windows.checked_add(1).ok_or_else(|| {
                    RsomicsError::InvalidInput("invalid-window count exceeds u64 range".to_owned())
                })?;
                continue;
            };
            if hash == 0 || hash > max_hash {
                continue;
            }
            let abundance = retained.entry(hash).or_insert(0);
            *abundance = abundance.checked_add(1).ok_or_else(|| {
                RsomicsError::InvalidInput(format!("abundance exceeds u64 range for hash {hash}"))
            })?;
        }
    }
    if records == 0 {
        return Err(RsomicsError::InvalidInput(format!(
            "{} contains no sequence records",
            path.display()
        )));
    }
    let mut entries: Vec<_> = retained.into_iter().collect();
    entries.sort_unstable_by_key(|entry| entry.0);
    let hashes: Vec<_> = entries.iter().map(|entry| entry.0).collect();
    let abundances = options
        .abundance
        .then(|| entries.into_iter().map(|entry| entry.1).collect());
    Ok(BuiltSignature {
        signature: Signature::new(
            filename.to_owned(),
            options.name.clone(),
            options.ksize.get(),
            options.seed,
            options.scaled.get(),
            hashes,
            abundances,
        ),
        records,
        bases,
        windows,
        invalid_windows,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn one_kmer_matches_sourmash_signature() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("acg.fa");
        let output = directory.path().join("acg.sig");
        fs::write(&input, b">acg\nACG\n").unwrap();
        let options = SketchOptions {
            ksize: NonZeroU32::new(3).unwrap(),
            scaled: NonZeroU32::new(1).unwrap(),
            ..SketchOptions::default()
        };
        let report = sketch(&[input], &output, &options).unwrap();
        assert_eq!(report.retained_hashes, 1);
        let signatures = crate::load_signatures(&output).unwrap();
        assert_eq!(
            signatures[0].sketch().hashes(),
            &[1_731_421_407_650_554_201]
        );
        assert_eq!(
            signatures[0].sketch().digest(),
            "08c05d56c36a44d38e708f93eb4f87b2"
        );
    }

    #[test]
    fn ambiguity_is_skipped_or_rejected_by_policy() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("invalid.fa");
        fs::write(&input, b">invalid\nACGNUTACG\n").unwrap();
        let mut options = SketchOptions {
            ksize: NonZeroU32::new(3).unwrap(),
            scaled: NonZeroU32::new(1).unwrap(),
            ..SketchOptions::default()
        };
        let output = directory.path().join("skip.sig");
        let report = sketch(std::slice::from_ref(&input), &output, &options).unwrap();
        assert_eq!(report.invalid_windows, 4);

        options.check_sequence = true;
        let error = sketch(&[input], &directory.path().join("check.sig"), &options).unwrap_err();
        assert!(error.to_string().contains("non-ACGT"));
    }

    #[test]
    fn failed_input_preserves_existing_output() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("empty.fa");
        let output = directory.path().join("result.sig");
        fs::write(&input, b"").unwrap();
        fs::write(&output, b"existing").unwrap();
        assert!(sketch(&[input], &output, &SketchOptions::default()).is_err());
        assert_eq!(fs::read(output).unwrap(), b"existing");
    }

    #[test]
    fn output_cannot_alias_an_input() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.fa");
        fs::write(&input, b">one\nACGT\n").unwrap();
        let error = sketch(
            std::slice::from_ref(&input),
            &input,
            &SketchOptions::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("also an input path"));
    }
}
