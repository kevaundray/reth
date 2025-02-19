//! Utility functions.

use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

pub(crate) fn find_all_files_with_postfix(path: &Path, postfix: &str) -> Vec<PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(postfix))
        .map(DirEntry::into_path)
        .collect::<Vec<PathBuf>>()
}

/// Tracing utility
pub mod reth_tracing {
    use tracing::Subscriber;
    use tracing_subscriber::{prelude::*, EnvFilter};

    /// Tracing modes
    pub enum TracingMode {
        /// Enable all info traces.
        All,
        /// Disable tracing
        Silent,
    }

    impl TracingMode {
        fn into_env_filter(self) -> EnvFilter {
            match self {
                Self::All => EnvFilter::new("reth=info"),
                Self::Silent => EnvFilter::new(""),
            }
        }
    }

    /// Build subscriber
    pub fn build_subscriber(mods: TracingMode) -> impl Subscriber {
        let nocolor = std::env::var("RUST_LOG_STYLE").map(|val| val == "never").unwrap_or(false);

        // Take env over config
        let filter = if std::env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default().is_empty() {
            mods.into_env_filter()
        } else {
            EnvFilter::from_default_env()
        };

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_ansi(!nocolor).with_target(false))
            .with(filter)
    }
}
