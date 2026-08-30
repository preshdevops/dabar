mod cli;
mod config;
mod db;

use colored::Colorize;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dabar_cli=info,dabar_core=info".into()),
        )
        .init();
    
    if let Err(e) = cli::run().await {
        eprintln!("{} {:?}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}
