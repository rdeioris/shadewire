mod args;
mod run;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

fn main() {
    let args = args::Args::parse();
    if let args::Command::Compile(compile) = &args.command
        && let Some(flag) = compile.misplaced_gdshader_flag()
    {
        let message = format!("{flag} applies to --lang gdshader only");
        args::Args::command().error(ErrorKind::ArgumentConflict, message).exit();
    }
    if let Err(error) = run::run(args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
