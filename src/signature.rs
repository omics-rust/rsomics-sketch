use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use rsomics_common::{Context, Result, RsomicsError, write_atomic};
use serde::{Deserialize, Serialize};

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

#[derive(Clone, Debug)]
pub struct Signature {
    email: String,
    filename: Option<String>,
    name: Option<String>,
    license: String,
    sketch: FracMinHash,
}

#[derive(Clone, Debug)]
pub struct FracMinHash {
    ksize: u32,
    seed: u64,
    scaled: u32,
    max_hash: u64,
    hashes: Vec<u64>,
    abundances: Option<Vec<u64>>,
    digest: String,
}

#[derive(Serialize, Deserialize)]
struct RawSignature {
    class: String,
    #[serde(default)]
    email: String,
    hash_function: String,
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    license: String,
    signatures: Vec<RawMinHash>,
    version: f64,
}

#[derive(Serialize, Deserialize)]
struct RawMinHash {
    num: u32,
    ksize: u32,
    seed: u64,
    max_hash: u64,
    mins: Vec<u64>,
    md5sum: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    abundances: Option<Vec<u64>>,
    molecule: String,
}

impl Signature {
    pub(crate) fn new(
        filename: String,
        name: Option<String>,
        ksize: u32,
        seed: u64,
        scaled: u32,
        hashes: Vec<u64>,
        abundances: Option<Vec<u64>>,
    ) -> Self {
        let max_hash = max_hash_for_scaled(scaled);
        let digest = digest(ksize, &hashes);
        Self {
            email: String::new(),
            filename: Some(filename),
            name,
            license: "CC0".to_owned(),
            sketch: FracMinHash {
                ksize,
                seed,
                scaled,
                max_hash,
                hashes,
                abundances,
                digest,
            },
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn license(&self) -> &str {
        &self.license
    }

    pub fn sketch(&self) -> &FracMinHash {
        &self.sketch
    }

    pub fn label(&self) -> &str {
        self.name()
            .or_else(|| self.filename())
            .unwrap_or_else(|| self.sketch.digest())
    }
}

impl FracMinHash {
    pub fn ksize(&self) -> u32 {
        self.ksize
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn scaled(&self) -> u32 {
        self.scaled
    }

    pub fn max_hash(&self) -> u64 {
        self.max_hash
    }

    pub fn hashes(&self) -> &[u64] {
        &self.hashes
    }

    pub fn abundances(&self) -> Option<&[u64]> {
        self.abundances.as_deref()
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

pub fn load_signatures(path: &Path) -> Result<Vec<Signature>> {
    let file =
        File::open(path).rs_with_context(|| format!("opening signature {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let prefix = reader
        .fill_buf()
        .rs_with_context(|| format!("reading signature header {}", path.display()))?;
    let raw: Vec<RawSignature> = if prefix.starts_with(&GZIP_MAGIC) {
        serde_json::from_reader(MultiGzDecoder::new(reader))
    } else {
        serde_json::from_reader(reader)
    }
    .map_err(|error| invalid(path, error))?;
    if raw.is_empty() {
        return Err(RsomicsError::InvalidInput(format!(
            "{} contains no signatures",
            path.display()
        )));
    }
    raw.into_iter()
        .enumerate()
        .map(|(index, signature)| validate(signature, path, index))
        .collect()
}

pub(crate) fn write_signatures(path: &Path, signatures: &[Signature]) -> Result<()> {
    let raw: Vec<_> = signatures.iter().map(RawSignature::from).collect();
    write_atomic(path, |output| {
        if path.extension().is_some_and(|extension| extension == "gz") {
            let mut encoder = GzEncoder::new(output, Compression::default());
            serde_json::to_writer(&mut encoder, &raw).map_err(json_output_error)?;
            encoder.try_finish().map_err(RsomicsError::Io)?;
        } else {
            serde_json::to_writer(output, &raw).map_err(json_output_error)?;
        }
        Ok(())
    })
}

pub(crate) fn max_hash_for_scaled(scaled: u32) -> u64 {
    match scaled {
        1 => u64::MAX,
        _ => (u64::MAX as f64 / f64::from(scaled)) as u64,
    }
}

fn validate(raw: RawSignature, path: &Path, index: usize) -> Result<Signature> {
    let location = || format!("{} signature {}", path.display(), index + 1);
    if raw.class != "sourmash_signature" {
        return Err(profile_error(location(), "class", &raw.class));
    }
    if raw.hash_function != "0.murmur64" {
        return Err(profile_error(
            location(),
            "hash_function",
            &raw.hash_function,
        ));
    }
    if raw.version != 0.4 {
        return Err(profile_error(
            location(),
            "version",
            &raw.version.to_string(),
        ));
    }
    if raw.license != "CC0" {
        return Err(profile_error(location(), "license", &raw.license));
    }
    let [sketch] = <[RawMinHash; 1]>::try_from(raw.signatures).map_err(|signatures| {
        RsomicsError::InvalidInput(format!(
            "{} must contain exactly one DNA FracMinHash sketch, found {}",
            location(),
            signatures.len()
        ))
    })?;
    if sketch.num != 0 {
        return Err(profile_error(location(), "num", &sketch.num.to_string()));
    }
    if sketch.ksize == 0 {
        return Err(profile_error(location(), "ksize", "0"));
    }
    if sketch.molecule != "DNA" {
        return Err(profile_error(location(), "molecule", &sketch.molecule));
    }
    if sketch.max_hash == 0 {
        return Err(profile_error(location(), "max_hash", "0"));
    }
    let scaled = scaled_for_max_hash(sketch.max_hash);
    if max_hash_for_scaled(scaled) != sketch.max_hash {
        return Err(RsomicsError::InvalidInput(format!(
            "{} has a max_hash outside the pinned scaled profile",
            location()
        )));
    }
    if sketch.mins.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(RsomicsError::InvalidInput(format!(
            "{} hashes are not strictly increasing",
            location()
        )));
    }
    if sketch
        .mins
        .last()
        .is_some_and(|hash| *hash > sketch.max_hash)
    {
        return Err(RsomicsError::InvalidInput(format!(
            "{} contains a hash above max_hash",
            location()
        )));
    }
    if let Some(abundances) = sketch.abundances.as_deref() {
        if abundances.len() != sketch.mins.len() {
            return Err(RsomicsError::InvalidInput(format!(
                "{} abundance and hash counts differ",
                location()
            )));
        }
        if abundances.contains(&0) {
            return Err(RsomicsError::InvalidInput(format!(
                "{} contains a zero abundance",
                location()
            )));
        }
    }
    let expected = digest(sketch.ksize, &sketch.mins);
    if sketch.md5sum != expected {
        return Err(RsomicsError::InvalidInput(format!(
            "{} digest mismatch: recorded {}, computed {expected}",
            location(),
            sketch.md5sum
        )));
    }
    Ok(Signature {
        email: raw.email,
        filename: raw.filename,
        name: raw.name,
        license: raw.license,
        sketch: FracMinHash {
            ksize: sketch.ksize,
            seed: sketch.seed,
            scaled,
            max_hash: sketch.max_hash,
            hashes: sketch.mins,
            abundances: sketch.abundances,
            digest: sketch.md5sum,
        },
    })
}

fn scaled_for_max_hash(max_hash: u64) -> u32 {
    match max_hash {
        u64::MAX => 1,
        _ => (u64::MAX as f64 / max_hash as f64) as u32,
    }
}

fn digest(ksize: u32, hashes: &[u64]) -> String {
    let mut context = md5::Context::new();
    context.consume(ksize.to_string());
    for hash in hashes {
        context.consume(hash.to_string());
    }
    format!("{:x}", context.finalize())
}

fn profile_error(location: String, field: &str, value: &str) -> RsomicsError {
    RsomicsError::InvalidInput(format!(
        "{location} has unsupported {field} value {value:?}"
    ))
}

fn invalid(path: &Path, error: serde_json::Error) -> RsomicsError {
    RsomicsError::InvalidInput(format!("parsing signature {}: {error}", path.display()))
}

fn json_output_error(error: serde_json::Error) -> RsomicsError {
    match error.io_error_kind() {
        Some(kind) => RsomicsError::Io(std::io::Error::new(kind, error)),
        None => RsomicsError::ConfigError(format!("serializing signature failed: {error}")),
    }
}

impl From<&Signature> for RawSignature {
    fn from(signature: &Signature) -> Self {
        Self {
            class: "sourmash_signature".to_owned(),
            email: signature.email.clone(),
            hash_function: "0.murmur64".to_owned(),
            filename: signature.filename.clone(),
            name: signature.name.clone(),
            license: signature.license.clone(),
            signatures: vec![RawMinHash {
                num: 0,
                ksize: signature.sketch.ksize,
                seed: signature.sketch.seed,
                max_hash: signature.sketch.max_hash,
                mins: signature.sketch.hashes.clone(),
                md5sum: signature.sketch.digest.clone(),
                abundances: signature.sketch.abundances.clone(),
                molecule: "DNA".to_owned(),
            }],
            version: 0.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn digest_matches_sourmash() {
        assert_eq!(
            digest(3, &[1_731_421_407_650_554_201]),
            "08c05d56c36a44d38e708f93eb4f87b2"
        );
    }

    #[test]
    fn scaled_threshold_matches_sourmash() {
        assert_eq!(max_hash_for_scaled(1), u64::MAX);
        assert_eq!(max_hash_for_scaled(1_000), 18_446_744_073_709_552);
    }

    #[test]
    fn gzip_round_trip_preserves_validated_signature() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("one.sig.gz");
        let signature = Signature::new(
            "one.fa".to_owned(),
            Some("one".to_owned()),
            3,
            42,
            1,
            vec![1_731_421_407_650_554_201],
            Some(vec![4]),
        );
        write_signatures(&path, &[signature]).unwrap();
        let loaded = load_signatures(&path).unwrap();
        assert_eq!(loaded[0].name(), Some("one"));
        assert_eq!(loaded[0].sketch().abundances(), Some([4].as_slice()));
    }

    #[test]
    fn digest_mismatch_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad.sig");
        let signature = Signature::new(
            "one.fa".to_owned(),
            None,
            3,
            42,
            1,
            vec![1_731_421_407_650_554_201],
            None,
        );
        let mut raw = RawSignature::from(&signature);
        raw.signatures[0].md5sum = "00000000000000000000000000000000".to_owned();
        fs::write(&path, serde_json::to_vec(&vec![raw]).unwrap()).unwrap();
        let error = load_signatures(&path).unwrap_err();
        assert!(error.to_string().contains("digest mismatch"));
    }
}
