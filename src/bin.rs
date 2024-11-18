use clap::{Parser, Subcommand};
use std::env;
use yoink::collection::collecter::Collecter;

use std::io::Write;
use std::{path::Path, fs::File};
use zip::write::FileOptions;
use zip::ZipWriter;
use zip::ZipArchive;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>
}

#[derive(Subcommand)]
enum Commands {
    Collect {
        artefacts: Vec<String>
    }
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Collect {
            artefacts,
        }) => {
            let mut collector = Collecter::new(env::consts::OS.to_string(), Some("asdf".to_string())).unwrap();
            collector.collect_all();
            collector.compress_collection("output_file.zip");

        }
        None => println!("Unsupported!")
    }
}