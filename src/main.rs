mod direct;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Increase debug info
    #[clap(short, long, parse(from_occurrences), global(true), max_occurrences(2))]
    verbose: u8,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test for changes between two files
    Direct {
	/// File containing reference implementaiton
	#[clap(parse(from_os_str), value_name = "FILE_OLD")]
	file_old: PathBuf,
	
	/// Updated implementation to check against
	#[clap(parse(from_os_str), value_name = "FILE_NEW")]
	file_new: PathBuf,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Direct { file_old, file_new } => {
	    direct::run(file_old, file_new, cli.verbose)?;
        }
    }
    
    Ok(())
}
