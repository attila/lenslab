use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "lenslab",
    version,
    about = "Characterise a camera lens from DNG/TIFF frames"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// EXIF + decode info + corrections-present flag (no measurement).
    Inspect {
        /// DNG or TIFF frame to inspect.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Inspect { file } => inspect(&file),
    }
}

fn inspect(path: &Path) -> anyhow::Result<()> {
    let decoder = lenslab_decode::decoder_for(path)?;
    let info = decoder.inspect(path)?;
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}
