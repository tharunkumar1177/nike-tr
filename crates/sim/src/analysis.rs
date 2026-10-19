//! The analyses a simulation runs.

use tron_core::{AcSweep, AnalysisSettings};

use crate::value::spice_number;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Analysis {
    /// `.op`: the DC operating point.
    OperatingPoint,
    /// `.tran`: the response over time.
    Transient(Transient),
    /// `.ac`: the small-signal response over frequency.
    Ac(Ac),
}

/// The `.ac` parameters, in hertz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ac {
    pub sweep: AcSweep,
    /// Points per decade or octave, or in total for a linear sweep.
    pub points: u32,
    pub start: f64,
    pub stop: f64,
}

/// The `.tran` parameters, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transient {
    pub step: f64,
    pub stop: f64,
    /// Results before this time are computed but not kept.
    pub start: f64,
    /// The largest internal time step ngspice may take. ngspice picks one when unset.
    pub max_step: Option<f64>,
    /// Skip the operating point and start from the components' initial conditions (`uic`).
    pub initial_conditions: bool,
}

impl Analysis {
    /// The SPICE analysis line, or a message naming the parameter that is out of range.
    pub fn directive(&self) -> Result<String, String> {
        match self {
            Analysis::OperatingPoint => Ok(".op".to_owned()),
            Analysis::Transient(transient) => transient.directive(),
            Analysis::Ac(ac) => ac.directive(),
        }
    }
}

impl Ac {
    fn directive(&self) -> Result<String, String> {
        if self.points == 0 {
            return Err("The AC sweep needs at least one point.".to_owned());
        }
        if !(self.start.is_finite() && self.start > 0.0) {
            return Err("The AC start frequency must be greater than zero.".to_owned());
        }
        if !(self.stop.is_finite() && self.stop > self.start) {
            return Err(
                "The AC stop frequency must be greater than the start frequency.".to_owned(),
            );
        }
        let sweep = match self.sweep {
            AcSweep::Decade => "dec",
            AcSweep::Octave => "oct",
            AcSweep::Linear => "lin",
        };
        Ok(format!(
            ".ac {sweep} {} {} {}",
            self.points,
            spice_number(self.start),
            spice_number(self.stop)
        ))
    }
}

impl From<AnalysisSettings> for Analysis {
    fn from(settings: AnalysisSettings) -> Analysis {
        match settings {
            AnalysisSettings::OperatingPoint => Analysis::OperatingPoint,
            AnalysisSettings::Transient {
                step,
                stop,
                start,
                max_step,
                initial_conditions,
            } => Analysis::Transient(Transient {
                step,
                stop,
                start,
                max_step,
                initial_conditions,
            }),
            AnalysisSettings::Ac {
                sweep,
                points,
                fstart,
                fstop,
            } => Analysis::Ac(Ac {
                sweep,
                points,
                start: fstart,
                stop: fstop,
            }),
        }
    }
}

impl Transient {
    fn directive(&self) -> Result<String, String> {
        let positive = |name: &str, value: f64| {
            if value.is_finite() && value > 0.0 {
                Ok(())
            } else {
                Err(format!("The transient {name} must be greater than zero."))
            }
        };
        positive("step", self.step)?;
        positive("stop time", self.stop)?;
        let start_in_range = self.start.is_finite() && self.start >= 0.0 && self.start < self.stop;
        if !start_in_range {
            return Err(
                "The transient start time must be zero or later, and before the stop time."
                    .to_owned(),
            );
        }
        if self.step > self.stop - self.start {
            return Err(
                "The transient step must not be longer than the time it records.".to_owned(),
            );
        }
        let mut line = format!(
            ".tran {} {} {}",
            spice_number(self.step),
            spice_number(self.stop),
            spice_number(self.start)
        );
        if let Some(max_step) = self.max_step {
            positive("maximum step", max_step)?;
            line.push(' ');
            line.push_str(&spice_number(max_step));
        }
        if self.initial_conditions {
            line.push_str(" uic");
        }
        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transient(step: f64, stop: f64) -> Transient {
        Transient {
            step,
            stop,
            start: 0.0,
            max_step: None,
            initial_conditions: false,
        }
    }

    #[test]
    fn directives_list_the_set_parameters() {
        assert_eq!(Analysis::OperatingPoint.directive().unwrap(), ".op");
        let plain = Analysis::Transient(transient(1e-6, 1e-3));
        assert_eq!(plain.directive().unwrap(), ".tran 1e-6 1e-3 0e0");
        let full = Analysis::Transient(Transient {
            start: 2e-4,
            max_step: Some(5e-7),
            initial_conditions: true,
            ..transient(1e-6, 1e-3)
        });
        assert_eq!(full.directive().unwrap(), ".tran 1e-6 1e-3 2e-4 5e-7 uic");
    }

    #[test]
    fn out_of_range_parameters_are_named() {
        let cases = [
            (transient(0.0, 1e-3), "step"),
            (transient(1e-6, f64::NAN), "stop time"),
            (transient(2e-3, 1e-3), "longer"),
            (
                Transient {
                    start: 1e-3,
                    ..transient(1e-6, 1e-3)
                },
                "start time",
            ),
            (
                Transient {
                    max_step: Some(-1.0),
                    ..transient(1e-6, 1e-3)
                },
                "maximum step",
            ),
        ];
        for (transient, word) in cases {
            let error = Analysis::Transient(transient).directive().unwrap_err();
            assert!(error.contains(word), "{error}");
        }
    }

    fn ac(points: u32, start: f64, stop: f64) -> Analysis {
        Analysis::Ac(Ac {
            sweep: AcSweep::Decade,
            points,
            start,
            stop,
        })
    }

    #[test]
    fn ac_sweeps_name_their_spacing_and_check_their_range() {
        assert_eq!(
            ac(100, 1.0, 1e6).directive().unwrap(),
            ".ac dec 100 1e0 1e6"
        );
        let octave = Analysis::Ac(Ac {
            sweep: AcSweep::Octave,
            points: 10,
            start: 20.0,
            stop: 2e4,
        });
        assert_eq!(octave.directive().unwrap(), ".ac oct 10 2e1 2e4");
        let cases = [
            (ac(0, 1.0, 1e3), "at least one point"),
            (ac(10, 0.0, 1e3), "start frequency"),
            (ac(10, f64::INFINITY, 1e3), "start frequency"),
            (ac(10, 1e3, 1e3), "stop frequency"),
            (ac(10, 1.0, f64::NAN), "stop frequency"),
        ];
        for (analysis, words) in cases {
            let error = analysis.directive().unwrap_err();
            assert!(error.contains(words), "{error}");
        }
    }
}
