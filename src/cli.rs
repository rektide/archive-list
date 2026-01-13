use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "archive-list")]
#[command(about = "CLI tool to download README files from repositories", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, help = "Generate shell completion for the given shell")]
    pub completions: Option<Shell>,
}

#[derive(Subcommand)]
pub enum Commands {
    ReadmeGet(ReadmeGetArgs),
}

#[derive(Parser)]
pub struct ReadmeGetArgs {
    #[arg(long, help = "Process archlist from top to bottom instead of bottom to top")]
    pub top_down: bool,

    #[arg(long, help = "Refresh all URLs even if already downloaded")]
    pub refresh: bool,
}
