use clap::{Parser, Subcommand};

mod album;
mod library;

#[derive(Parser)]
#[command(name = "mue", version = "0.1.0")]
struct Cli {
    #[arg(global = true, long)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Album {
        #[command(subcommand)]
        command: AlbumCommands,
    },
    Library {
        #[command(subcommand)]
        command: LibraryCommands,
    },
}

#[derive(Subcommand)]
enum AlbumCommands {
    Build {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        flake: Option<String>,
    },
    Fetch {
        #[arg(long)]
        source: String,
        #[arg(default_value = ".")]
        path: String,
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
}

#[derive(Subcommand)]
enum LibraryCommands {
    Rebuild,
    Init {
        path: String,
    },
    MigrateStore,
}

fn main() {
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
        Commands::Album { command } => match command {
            AlbumCommands::Build { path, source, flake } => {
                if let Err(e) = album::build::run(&path, source.as_deref(), flake.as_deref()) {
                    log::error!("{e}");
                    std::process::exit(1);
                }
            }
            AlbumCommands::Fetch { source, path } => {
                if let Err(e) = album::fetch::run(&source, &path) {
                    log::error!("{e}");
                    std::process::exit(1);
                }
            }
            AlbumCommands::Manifest { torrent, path, tracks, metadata, intermediary } => {
                if let Err(e) = album::manifest::run(&path, &tracks, torrent.as_deref(), metadata.as_deref(), intermediary) {
                    log::error!("{e}");
                    std::process::exit(1);
                }
            }
        },
        Commands::Library { command } => match command {
            LibraryCommands::Rebuild => {
                if let Err(e) = library::rebuild::run() {
                    log::error!("{e}");
                    std::process::exit(1);
                }
            }
            LibraryCommands::Init { path } => {
                if let Err(e) = library::init::run(&path) {
                    log::error!("{e}");
                    std::process::exit(1);
                }
            }
            LibraryCommands::MigrateStore => {
                if let Err(e) = library::migrate_store::run() {
                    log::error!("{e}");
                    std::process::exit(1);
                }
            }
        },
    }
}
