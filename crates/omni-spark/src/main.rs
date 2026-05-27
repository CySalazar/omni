//! OMNI Spark — entry point.
//!
//! Launches OMNI Spark. See `lib.rs` for the full architecture
//! overview and OIP-025 for the specification.

fn main() {
    // Initialize tracing (structured logging).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("OMNI Spark starting");

    // Build the async runtime and run the application.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    if let Err(e) = rt.block_on(omni_spark::run()) {
        tracing::error!(error = %e, "fatal error");
        std::process::exit(1);
    }
}
