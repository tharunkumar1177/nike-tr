use std::fmt;

/// Why a schematic cannot be simulated, found before ngspice runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightError {
    /// No analysis is set.
    NoAnalysis,
    /// The analysis parameters are out of range.
    InvalidAnalysis(String),
    /// No simulated device connects to a ground net (`GND` or `0`).
    NoGround,
    /// A symbol has no simulation model Tron can use.
    NoModel { reference: String, reason: String },
    /// A symbol still has an unannotated reference such as `R?`.
    Unannotated(String),
    /// Two simulated symbols share a reference, so their SPICE names collide.
    DuplicateReference(String),
    /// The schematic's connectivity could not be derived.
    Netlist(String),
    /// No symbol takes part in the simulation.
    Empty,
}

impl fmt::Display for PreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreflightError::NoAnalysis => write!(
                f,
                "No analysis is set. Choose an operating point or a transient analysis."
            ),
            PreflightError::InvalidAnalysis(message) => f.write_str(message),
            PreflightError::NoGround => write!(
                f,
                "The circuit has no ground. Connect a GND power symbol to the circuit."
            ),
            PreflightError::NoModel { reference, reason } => write!(f, "{reference}: {reason}"),
            PreflightError::Unannotated(reference) => write!(
                f,
                "{reference} is not annotated. Annotate the schematic before simulating."
            ),
            PreflightError::DuplicateReference(reference) => write!(
                f,
                "More than one symbol is {reference}. Annotate the schematic before simulating."
            ),
            PreflightError::Netlist(message) => f.write_str(message),
            PreflightError::Empty => write!(f, "The schematic has nothing to simulate."),
        }
    }
}

impl std::error::Error for PreflightError {}
