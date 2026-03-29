use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Initializes the application's logging system.
///
/// This function sets up tracing with a filter based on the provided log level.
/// The logging output follows the structured logging format provided by
/// tracing_subscriber.
///
/// # Panics
///
/// Panics if the global default subscriber has already been set or if subscriber
/// initialization fails.
#[cfg(not(tarpaulin_include))]
pub fn setup_logging(log_level: &str) {
  let filter = EnvFilter::new(format!("vmm={}", log_level));
  let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
  tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
