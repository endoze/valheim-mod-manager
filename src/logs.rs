use tracing_subscriber::{EnvFilter, FmtSubscriber};

use crate::config::APP_CONFIG;

/// Initializes the application's logging system.
///
/// This function sets up tracing with a filter based on the log_level specified
/// in the application configuration. The logging output follows the structured
/// logging format provided by tracing_subscriber.
///
/// # Panics
///
/// Panics if the global default subscriber has already been set or if subscriber
/// initialization fails.
#[cfg(not(tarpaulin_include))]
pub fn setup_logging() {
  let filter = EnvFilter::new(format!("vmm={}", APP_CONFIG.log_level));
  let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
  tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
