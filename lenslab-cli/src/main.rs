use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod contact;

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
    /// Render a labelled PNG contact sheet from DNG/TIFF frames.
    Contact {
        /// DNG or TIFF frames to include, in contact-sheet order.
        #[arg(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// PNG output path.
        #[arg(long, short)]
        out: PathBuf,
    },

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
        Command::Contact { paths, out } => contact::write_contact_sheet(&paths, &out),
        Command::Inspect { file } => inspect(&file),
    }
}

fn inspect(path: &Path) -> anyhow::Result<()> {
    let decoder = lenslab_decode::decoder_for(path)?;
    let info = decoder.inspect(path)?;
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}
