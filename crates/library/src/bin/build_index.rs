//! Builds the component library index from extracted KiCad library releases.

use std::path::PathBuf;
use std::process::ExitCode;

use tron_library::build_index;

const USAGE: &str =
    "usage: build-index --symbols <dir> --footprints <dir> --kicad-tag <tag> --output <file>";

struct Args {
    symbols: PathBuf,
    footprints: PathBuf,
    kicad_tag: String,
    output: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut symbols = None;
    let mut footprints = None;
    let mut kicad_tag = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            return Err(format!("{flag} needs a value"));
        };
        match flag.as_str() {
            "--symbols" => symbols = Some(PathBuf::from(value)),
            "--footprints" => footprints = Some(PathBuf::from(value)),
            "--kicad-tag" => kicad_tag = Some(value),
            "--output" => output = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    Ok(Args {
        symbols: symbols.ok_or("--symbols is required")?,
        footprints: footprints.ok_or("--footprints is required")?,
        kicad_tag: kicad_tag.ok_or("--kicad-tag is required")?,
        output: output.ok_or("--output is required")?,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match build_index(
        &args.symbols,
        &args.footprints,
        &args.kicad_tag,
        &args.output,
    ) {
        Ok(report) => {
            println!(
                "Indexed {} symbols from {} libraries",
                report.symbols, report.symbol_libraries
            );
            println!(
                "Indexed {} footprints from {} libraries",
                report.footprints, report.footprint_libraries
            );
            println!("Wrote {}", args.output.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
