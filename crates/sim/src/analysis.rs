//! The analyses a simulation runs.

use crate::value::spice_number;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Analysis {
    /// `.op`: the DC operating point.
    OperatingPoint,
    /// `.tran`: the response over time.
    Transient(Transient),
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
}
