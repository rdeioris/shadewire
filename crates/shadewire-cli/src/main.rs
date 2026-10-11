mod args;
mod run;

use clap::Parser;

fn main() {
    if let Err(error) = run::run(args::Args::parse()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
