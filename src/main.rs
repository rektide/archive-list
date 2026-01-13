mod cli;
mod commands;
mod config;
mod failure;
mod provider;
mod util;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands};
use commands::readme_get::readme_get;

#[tokio::main]
async fn main() -> Result<()> {
    let mut cli = Cli::parse();

    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "archive-list", &mut std::io::stdout());
        return Ok(());
    }

    match cli.command {
        Commands::ReadmeGet(args) => readme_get(args).await?,
    }

    Ok(())
}
