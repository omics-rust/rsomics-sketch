use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use rsomics_common::{OutputArgs, Result, ToolMeta};
use serde::Serialize;

use rsomics_sketch::{
    CompareOptions, CompareReport, InspectReport, MatrixMeasure, OutputShape, SearchMeasure,
    SearchOptions, SearchReport, SketchOptions, SketchReport, compare, inspect, search, sketch,
};

pub const META: ToolMeta = ToolMeta {
    name: env!("CARGO_PKG_NAME"),
    version: env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Parser)]
#[command(
    name = "rsomics-sketch",
    version,
    about = "Persistent DNA FracMinHash sketches, comparison, and linear search",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build sourmash-compatible DNA FracMinHash signatures.
    Sketch(SketchArgs),
    /// Validate signatures and emit their metadata.
    Inspect(InspectArgs),
    /// Compare compatible signatures pairwise or as a labelled matrix.
    Compare(CompareArgs),
    /// Search one query against signature files without an index.
    Search(SearchArgs),
}

#[derive(Debug, Args)]
struct SketchArgs {
    /// FASTA or FASTQ inputs; gzip and BGZF are detected by content.
    #[arg(required = true, num_args = 1..)]
    inputs: Vec<PathBuf>,

    /// Transactional sourmash 0.4 signature output; `.gz` enables gzip.
    #[arg(short, long, value_name = "SIGNATURE")]
    output: PathBuf,

    /// DNA k-mer length.
    #[arg(short = 'k', long, default_value_t = NonZeroU32::new(31).unwrap())]
    ksize: NonZeroU32,

    /// Keep hashes at the FracMinHash scaled threshold.
    #[arg(long, default_value_t = NonZeroU32::new(1_000).unwrap())]
    scaled: NonZeroU32,

    /// MurmurHash seed recorded in the signature.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// Track retained k-mer multiplicities.
    #[arg(long)]
    abundance: bool,

    /// Reject the first sequence byte outside ASCII A/C/G/T.
    #[arg(long)]
    check_sequence: bool,

    /// Set the display name; valid only with one input.
    #[arg(long)]
    name: Option<String>,
}

#[derive(Debug, Args)]
struct InspectArgs {
    /// Sourmash 0.4 signature inputs.
    #[arg(required = true, num_args = 1..)]
    inputs: Vec<PathBuf>,

    /// Transactional tab-separated metadata output.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Sourmash 0.4 signature inputs or collections.
    #[arg(required = true, num_args = 1..)]
    inputs: Vec<PathBuf>,

    /// Transactional tab-separated comparison output.
    #[arg(short, long)]
    output: PathBuf,

    /// Emit one row per pair or a labelled matrix.
    #[arg(long, value_enum, default_value_t = OutputShape::Pairs)]
    shape: OutputShape,

    /// Score represented by matrix cells; containment is column-in-row.
    #[arg(long, value_enum, default_value_t = MatrixMeasure::Jaccard)]
    matrix_measure: MatrixMeasure,
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// File containing exactly one query signature.
    query: PathBuf,

    /// Signature files or collections searched in the given order.
    #[arg(required = true, num_args = 1..)]
    databases: Vec<PathBuf>,

    /// Transactional tab-separated search result output.
    #[arg(short, long)]
    output: PathBuf,

    /// Ranking and threshold measure.
    #[arg(long, value_enum, default_value_t = SearchMeasure::Jaccard)]
    measure: SearchMeasure,

    /// Report scores strictly above this value in 0..=1.
    #[arg(short, long, default_value_t = 0.08)]
    threshold: f64,

    /// Maximum number of matches after stable ranking.
    #[arg(short = 'n', long)]
    limit: Option<NonZeroUsize>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum CommandReport {
    Sketch(SketchReport),
    Inspect(InspectReport),
    Compare(CompareReport),
    Search(SearchReport),
}

impl Cli {
    pub fn execute(self) -> Result<CommandReport> {
        match self.command {
            Command::Sketch(args) => sketch(
                &args.inputs,
                &args.output,
                &SketchOptions {
                    ksize: args.ksize,
                    scaled: args.scaled,
                    seed: args.seed,
                    abundance: args.abundance,
                    check_sequence: args.check_sequence,
                    name: args.name,
                },
            )
            .map(CommandReport::Sketch),
            Command::Inspect(args) => {
                inspect(&args.inputs, &args.output).map(CommandReport::Inspect)
            }
            Command::Compare(args) => compare(
                &args.inputs,
                &args.output,
                &CompareOptions {
                    shape: args.shape,
                    matrix_measure: args.matrix_measure,
                },
            )
            .map(CommandReport::Compare),
            Command::Search(args) => search(
                &args.query,
                &args.databases,
                &args.output,
                &SearchOptions {
                    measure: args.measure,
                    threshold: args.threshold,
                    limit: args.limit,
                },
            )
            .map(CommandReport::Search),
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn command_tree_is_valid() {
        rsomics_help::command::<Cli>().debug_assert();
        Cli::command().debug_assert();
    }
}
