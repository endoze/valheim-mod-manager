use tracing_subscriber::{EnvFilter, FmtSubscriber};

use crate::config::APP_CONFIG;

pub fn setup_logging() {
  let filter = EnvFilter::new(format!("vmm={}", APP_CONFIG.log_level));
  let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
  tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
