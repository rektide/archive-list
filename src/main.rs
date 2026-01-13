mod cli;
mod commands;
mod config;
mod failure;
mod provider;
mod util;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use commands::readme_get::readme_get;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::ReadmeGet(args) => readme_get(args).await?,
    }

    Ok(())
}
