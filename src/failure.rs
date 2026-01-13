use std::fs::OpenOptions;
use std::io::Write;
use anyhow::{Result, Context};

pub fn log_failure(url: &str, error_code: &str, fail_file: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(fail_file)
        .context("Failed to open .fail file")?;

    writeln!(file, "{} {}", url, error_code)?;
    Ok(())
}
