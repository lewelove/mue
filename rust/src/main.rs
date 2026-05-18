use clap::{Parser, Subcommand};
use anyhow::Result;

mod build;
mod fetch;
mod manifest;
mod config;
mod utils;
mod discid;
mod remote;

#[derive(Parser)]
#[command(name = "munix", version = "0.1.0")]
struct Cli {
    #[arg(global = true, long)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        source: Option<String>,
    },
    Manifest {
        #[arg(long)]
        torrent: Option<String>,
        #[arg(default_value = ".")]
        path: String,
        #[arg(long, default_value = "flac,mp3,wav")]
        tracks: String,
        #[arg(long)]
        metadata: Option<String>,
        #[arg(long)]
        intermediary: bool,
    },
    Fetch {
        #[arg(long)]
        source: String,
        #[arg(default_value = ".")]
        path: String,
    },
    Discid {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        tracks: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    simple_logger::SimpleLogger::new()
        .with_level(log_level)
        .env()
        .init()
        .unwrap();

    match cli.command {
        Commands::Build { path, source } => {
            if let Err(e) = build::run(&path, source.as_deref()) {
                log::error!("{}", e);
                std::process::exit(1);
            }
        }
        Commands::Manifest { torrent, path, tracks, metadata, intermediary } => {
            if let Err(e) = manifest::run(&path, &tracks, torrent.as_deref(), metadata.as_deref(), intermediary) {
                log::error!("{}", e);
                std::process::exit(1);
            }
        }
        Commands::Fetch { source, path } => {
            if let Err(e) = fetch::run(&source, &path) {
                log::error!("{}", e);
                std::process::exit(1);
            }
        }
        Commands::Discid { source, tracks } => {
            discid::run(source.as_deref(), tracks.as_deref());
        }
    }

    Ok(())
}
