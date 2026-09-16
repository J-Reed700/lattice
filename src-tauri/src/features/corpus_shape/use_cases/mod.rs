//! Corpus-shape use cases.

pub mod run_clustering;

pub use run_clustering::{
    ClusterProgress, ProgressSink, RunClusteringOutcome, RunClusteringUseCase,
};
