use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use rsomics_common::{Result, RsomicsError, reject_output_alias, write_atomic};
use serde::Serialize;

use crate::signature::{FracMinHash, Signature, load_signatures, max_hash_for_scaled};

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum OutputShape {
    #[default]
    Pairs,
    Matrix,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum MatrixMeasure {
    #[default]
    Jaccard,
    Containment,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMeasure {
    #[default]
    Jaccard,
    Containment,
}

#[derive(Clone, Debug)]
pub struct CompareOptions {
    pub shape: OutputShape,
    pub matrix_measure: MatrixMeasure,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            shape: OutputShape::Pairs,
            matrix_measure: MatrixMeasure::Jaccard,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub measure: SearchMeasure,
    pub threshold: f64,
    pub limit: Option<NonZeroUsize>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            measure: SearchMeasure::Jaccard,
            threshold: 0.08,
            limit: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct InspectReport {
    pub signatures: usize,
    pub output: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompareReport {
    pub signatures: usize,
    pub comparisons: usize,
    pub output: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchReport {
    pub candidates: usize,
    pub matches: usize,
    pub measure: SearchMeasure,
    pub threshold: f64,
    pub output: PathBuf,
}

#[derive(Clone, Copy)]
struct Metrics {
    scaled: u32,
    intersection: usize,
    left_hashes: usize,
    right_hashes: usize,
    jaccard: f64,
    left_containment: f64,
    right_containment: f64,
    left_ani: f64,
    right_ani: f64,
}

struct Ranked<'a> {
    ordinal: usize,
    signature: &'a Signature,
    metrics: Metrics,
    score: f64,
}

pub fn inspect(inputs: &[PathBuf], output: &Path) -> Result<InspectReport> {
    reject_output_alias(output, inputs.iter().map(PathBuf::as_path))?;
    let signatures = load_all(inputs)?;
    write_atomic(output, |writer| {
        let mut csv = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_writer(writer);
        csv.write_record([
            "location",
            "index",
            "name",
            "filename",
            "md5",
            "ksize",
            "molecule",
            "scaled",
            "seed",
            "hashes",
            "abundance",
            "license",
        ])
        .map_err(csv_error)?;
        for (location, index, signature) in &signatures {
            let sketch = signature.sketch();
            csv.write_record([
                location.display().to_string(),
                (index + 1).to_string(),
                signature.name().unwrap_or_default().to_owned(),
                signature.filename().unwrap_or_default().to_owned(),
                sketch.digest().to_owned(),
                sketch.ksize().to_string(),
                "DNA".to_owned(),
                sketch.scaled().to_string(),
                sketch.seed().to_string(),
                sketch.hashes().len().to_string(),
                sketch.abundances().is_some().to_string(),
                signature.license().to_owned(),
            ])
            .map_err(csv_error)?;
        }
        csv.flush().map_err(RsomicsError::Io)
    })?;
    Ok(InspectReport {
        signatures: signatures.len(),
        output: output.to_owned(),
    })
}

pub fn compare(
    inputs: &[PathBuf],
    output: &Path,
    options: &CompareOptions,
) -> Result<CompareReport> {
    reject_output_alias(output, inputs.iter().map(PathBuf::as_path))?;
    let loaded = load_all(inputs)?;
    if loaded.len() < 2 {
        return Err(RsomicsError::InvalidInput(
            "compare requires at least two signatures".to_owned(),
        ));
    }
    let signatures: Vec<_> = loaded.iter().map(|(_, _, signature)| signature).collect();
    for pair in signatures.windows(2) {
        compatible(pair[0].sketch(), pair[1].sketch())?;
    }
    write_atomic(output, |writer| match options.shape {
        OutputShape::Pairs => write_pairs(writer, &signatures),
        OutputShape::Matrix => write_matrix(writer, &signatures, options.matrix_measure),
    })?;
    Ok(CompareReport {
        signatures: signatures.len(),
        comparisons: signatures.len() * (signatures.len() - 1) / 2,
        output: output.to_owned(),
    })
}

pub fn search(
    query_path: &Path,
    databases: &[PathBuf],
    output: &Path,
    options: &SearchOptions,
) -> Result<SearchReport> {
    if !options.threshold.is_finite() || !(0.0..=1.0).contains(&options.threshold) {
        return Err(RsomicsError::InvalidInput(format!(
            "threshold must be finite and in 0..=1, got {}",
            options.threshold
        )));
    }
    if databases.is_empty() {
        return Err(RsomicsError::InvalidInput(
            "search requires at least one signature collection".to_owned(),
        ));
    }
    reject_output_alias(
        output,
        std::iter::once(query_path).chain(databases.iter().map(PathBuf::as_path)),
    )?;
    let query = load_signatures(query_path)?;
    let [query] = <[Signature; 1]>::try_from(query).map_err(|signatures| {
        RsomicsError::InvalidInput(format!(
            "query {} must contain exactly one signature, found {}",
            query_path.display(),
            signatures.len()
        ))
    })?;
    let loaded = load_all(databases)?;
    let mut ranked = Vec::with_capacity(loaded.len());
    for (ordinal, (_, _, target)) in loaded.iter().enumerate() {
        let metrics = metrics(query.sketch(), target.sketch())?;
        let score = match options.measure {
            SearchMeasure::Jaccard => metrics.jaccard,
            SearchMeasure::Containment => metrics.left_containment,
        };
        if score > options.threshold {
            ranked.push(Ranked {
                ordinal,
                signature: target,
                metrics,
                score,
            });
        }
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });
    if let Some(limit) = options.limit {
        ranked.truncate(limit.get());
    }
    write_search(output, &query, options.measure, &ranked)?;
    Ok(SearchReport {
        candidates: loaded.len(),
        matches: ranked.len(),
        measure: options.measure,
        threshold: options.threshold,
        output: output.to_owned(),
    })
}

fn load_all(paths: &[PathBuf]) -> Result<Vec<(PathBuf, usize, Signature)>> {
    if paths.is_empty() {
        return Err(RsomicsError::InvalidInput(
            "at least one signature input is required".to_owned(),
        ));
    }
    let mut loaded = Vec::new();
    for path in paths {
        for (index, signature) in load_signatures(path)?.into_iter().enumerate() {
            loaded.push((path.clone(), index, signature));
        }
    }
    Ok(loaded)
}

fn compatible(left: &FracMinHash, right: &FracMinHash) -> Result<()> {
    if left.ksize() != right.ksize() {
        return Err(incompatible("ksize", left.ksize(), right.ksize()));
    }
    if left.seed() != right.seed() {
        return Err(incompatible("seed", left.seed(), right.seed()));
    }
    if left.abundances().is_some() != right.abundances().is_some() {
        return Err(RsomicsError::InvalidInput(
            "incompatible abundance profiles".to_owned(),
        ));
    }
    Ok(())
}

fn metrics(left: &FracMinHash, right: &FracMinHash) -> Result<Metrics> {
    compatible(left, right)?;
    let scaled = left.scaled().max(right.scaled());
    let max_hash = max_hash_for_scaled(scaled);
    let left_hashes = left.hashes().partition_point(|hash| *hash <= max_hash);
    let right_hashes = right.hashes().partition_point(|hash| *hash <= max_hash);
    let intersection = intersection(
        &left.hashes()[..left_hashes],
        &right.hashes()[..right_hashes],
    );
    let union = left_hashes + right_hashes - intersection;
    let jaccard = ratio(intersection, union);
    let left_containment = ratio(intersection, left_hashes);
    let right_containment = ratio(intersection, right_hashes);
    let ksize = f64::from(left.ksize());
    Ok(Metrics {
        scaled,
        intersection,
        left_hashes,
        right_hashes,
        jaccard,
        left_containment,
        right_containment,
        left_ani: left_containment.powf(1.0 / ksize),
        right_ani: right_containment.powf(1.0 / ksize),
    })
}

fn intersection(left: &[u64], right: &[u64]) -> usize {
    let mut left_index = 0;
    let mut right_index = 0;
    let mut shared = 0;
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            std::cmp::Ordering::Less => left_index += 1,
            std::cmp::Ordering::Greater => right_index += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                left_index += 1;
                right_index += 1;
            }
        }
    }
    shared
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn write_pairs(writer: &mut std::fs::File, signatures: &[&Signature]) -> Result<()> {
    let mut csv = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(writer);
    csv.write_record([
        "left",
        "right",
        "left_md5",
        "right_md5",
        "scaled",
        "intersection",
        "left_hashes",
        "right_hashes",
        "jaccard",
        "left_containment",
        "right_containment",
        "left_ani_containment_root",
        "right_ani_containment_root",
    ])
    .map_err(csv_error)?;
    for left in 0..signatures.len() {
        for right in left + 1..signatures.len() {
            let metrics = metrics(signatures[left].sketch(), signatures[right].sketch())?;
            csv.write_record([
                signatures[left].label().to_owned(),
                signatures[right].label().to_owned(),
                signatures[left].sketch().digest().to_owned(),
                signatures[right].sketch().digest().to_owned(),
                metrics.scaled.to_string(),
                metrics.intersection.to_string(),
                metrics.left_hashes.to_string(),
                metrics.right_hashes.to_string(),
                metrics.jaccard.to_string(),
                metrics.left_containment.to_string(),
                metrics.right_containment.to_string(),
                metrics.left_ani.to_string(),
                metrics.right_ani.to_string(),
            ])
            .map_err(csv_error)?;
        }
    }
    csv.flush().map_err(RsomicsError::Io)
}

fn write_matrix(
    writer: &mut std::fs::File,
    signatures: &[&Signature],
    measure: MatrixMeasure,
) -> Result<()> {
    let mut csv = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(writer);
    let mut header = Vec::with_capacity(signatures.len() + 1);
    header.push(String::new());
    header.extend(
        signatures
            .iter()
            .map(|signature| signature.label().to_owned()),
    );
    csv.write_record(header).map_err(csv_error)?;
    for (row, left) in signatures.iter().enumerate() {
        let mut values = Vec::with_capacity(signatures.len() + 1);
        values.push(left.label().to_owned());
        for (column, right) in signatures.iter().enumerate() {
            let value = if row == column {
                1.0
            } else {
                let metrics = metrics(left.sketch(), right.sketch())?;
                match measure {
                    MatrixMeasure::Jaccard => metrics.jaccard,
                    MatrixMeasure::Containment => metrics.right_containment,
                }
            };
            values.push(value.to_string());
        }
        csv.write_record(values).map_err(csv_error)?;
    }
    csv.flush().map_err(RsomicsError::Io)
}

fn write_search(
    output: &Path,
    query: &Signature,
    measure: SearchMeasure,
    ranked: &[Ranked<'_>],
) -> Result<()> {
    write_atomic(output, |writer| {
        let mut csv = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_writer(writer);
        csv.write_record([
            "score",
            "measure",
            "target_md5",
            "target_name",
            "target_filename",
            "query_md5",
            "query_name",
            "query_filename",
            "scaled",
            "intersection",
            "query_hashes",
            "target_hashes",
            "query_containment",
            "target_containment",
            "query_ani_containment_root",
            "target_ani_containment_root",
        ])
        .map_err(csv_error)?;
        for result in ranked {
            csv.write_record([
                result.score.to_string(),
                match measure {
                    SearchMeasure::Jaccard => "jaccard",
                    SearchMeasure::Containment => "containment",
                }
                .to_owned(),
                result.signature.sketch().digest().to_owned(),
                result.signature.name().unwrap_or_default().to_owned(),
                result.signature.filename().unwrap_or_default().to_owned(),
                query.sketch().digest().to_owned(),
                query.name().unwrap_or_default().to_owned(),
                query.filename().unwrap_or_default().to_owned(),
                result.metrics.scaled.to_string(),
                result.metrics.intersection.to_string(),
                result.metrics.left_hashes.to_string(),
                result.metrics.right_hashes.to_string(),
                result.metrics.left_containment.to_string(),
                result.metrics.right_containment.to_string(),
                result.metrics.left_ani.to_string(),
                result.metrics.right_ani.to_string(),
            ])
            .map_err(csv_error)?;
        }
        csv.flush().map_err(RsomicsError::Io)
    })
}

fn incompatible<T: std::fmt::Display>(field: &str, left: T, right: T) -> RsomicsError {
    RsomicsError::InvalidInput(format!("incompatible {field}: left={left}, right={right}"))
}

fn csv_error(error: csv::Error) -> RsomicsError {
    if error.is_io_error() {
        RsomicsError::Io(std::io::Error::other(error))
    } else {
        RsomicsError::ConfigError(format!("serializing tabular output failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    fn signature(name: &str, hashes: Vec<u64>) -> Signature {
        Signature::new(
            format!("{name}.fa"),
            Some(name.to_owned()),
            3,
            42,
            1,
            hashes,
            None,
        )
    }

    #[test]
    fn metrics_match_sourmash_fixture() {
        let left = signature("left", (0..17).collect());
        let right = signature("right", (0..15).chain([20]).collect());
        let value = metrics(left.sketch(), right.sketch()).unwrap();
        assert_eq!(value.intersection, 15);
        assert_eq!(value.jaccard, 15.0 / 18.0);
        assert_eq!(value.left_containment, 15.0 / 17.0);
        assert_eq!(value.right_containment, 15.0 / 16.0);
    }

    #[test]
    fn downsampling_uses_the_coarser_scaled_value() {
        let threshold = max_hash_for_scaled(NonZeroU32::new(2).unwrap().get());
        let left = Signature::new(
            "left.fa".to_owned(),
            None,
            3,
            42,
            1,
            vec![1, threshold + 1],
            None,
        );
        let right = Signature::new("right.fa".to_owned(), None, 3, 42, 2, vec![1], None);
        let value = metrics(left.sketch(), right.sketch()).unwrap();
        assert_eq!(value.scaled, 2);
        assert_eq!(value.jaccard, 1.0);
    }

    #[test]
    fn empty_off_diagonal_uses_sourmash_zero_semantics() {
        let left = signature("left", Vec::new());
        let right = signature("right", Vec::new());
        let value = metrics(left.sketch(), right.sketch()).unwrap();
        assert_eq!(value.jaccard, 0.0);
        assert_eq!(value.left_containment, 0.0);
        assert_eq!(value.right_containment, 0.0);
    }
}
