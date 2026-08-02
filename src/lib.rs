#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

mod compare;
mod signature;
mod sketch;

pub use compare::{
    CompareOptions, CompareReport, InspectReport, MatrixMeasure, OutputShape, SearchMeasure,
    SearchOptions, SearchReport, compare, inspect, search,
};
pub use signature::{FracMinHash, Signature, load_signatures};
pub use sketch::{SketchOptions, SketchReport, sketch};
