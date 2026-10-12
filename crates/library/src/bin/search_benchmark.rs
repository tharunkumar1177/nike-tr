//! Checks a built component library index before it is published: it must hold a full KiCad
//! release, every benchmark query must answer within the latency budget, and well-known parts
//! must rank first for their own names.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tron_library::{Index, ItemKind, LibraryError};

/// The slowest median latency any benchmark query may have.
const BUDGET: Duration = Duration::from_millis(50);
const RUNS: usize = 25;
/// The page size the library browser requests.
const LIMIT: usize = 50;
/// Fewer items of a kind than this means the build dropped most of a KiCad release.
const MIN_ITEMS: usize = 5_000;

const QUERIES: [&str; 10] = [
    "r",
    "resistor",
    "LM358",
    "R_0603_1608Metric",
    "usb c",
    "conn 2x10",
    "opamp dual",
    "Device:LED",
    "sot-23",
    "capacitor 0805",
];

/// Parts every KiCad release ships, each with the kind its exact name must rank first as.
const EXPECTED: [(&str, ItemKind); 3] = [
    ("R", ItemKind::Symbol),
    ("LM358", ItemKind::Symbol),
    ("R_0603_1608Metric", ItemKind::Footprint),
];

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn run(path: &Path) -> Result<bool, LibraryError> {
    let index = Index::open(path)?;
    let mut passed = true;

    for kind in [ItemKind::Symbol, ItemKind::Footprint] {
        let count = index.item_count(kind)?;
        println!("{:<10} {count}", kind.as_str());
        if count < MIN_ITEMS {
            println!("  FAIL: expected at least {MIN_ITEMS}");
            passed = false;
        }
    }

    for query in QUERIES {
        let hits = index.search(query, LIMIT)?.len();
        let mut timings = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            let start = Instant::now();
            index.search(query, LIMIT)?;
            timings.push(start.elapsed());
        }
        timings.sort();
        let median = timings[RUNS / 2];
        println!(
            "{query:<20} median {:>7.2} ms  slowest {:>7.2} ms  {hits} hits",
            milliseconds(median),
            milliseconds(timings[RUNS - 1])
        );
        if median > BUDGET {
            println!("  FAIL: median exceeds {} ms", BUDGET.as_millis());
            passed = false;
        }
        if hits == 0 {
            println!("  FAIL: no results");
            passed = false;
        }
    }

    for (name, kind) in EXPECTED {
        let hits = index.search(name, LIMIT)?;
        match hits.first() {
            Some(hit) if hit.name == name && hit.kind == kind => {
                println!("{name:<20} ranks first as {}:{}", hit.lib, hit.name);
            }
            Some(hit) => {
                println!(
                    "{name:<20} FAIL: the first result is the {} {}:{}",
                    hit.kind.as_str(),
                    hit.lib,
                    hit.name
                );
                passed = false;
            }
            None => {
                println!("{name:<20} FAIL: no results");
                passed = false;
            }
        }
    }

    Ok(passed)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("usage: search-benchmark <index file>");
        return ExitCode::from(2);
    };
    match run(&PathBuf::from(path)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
