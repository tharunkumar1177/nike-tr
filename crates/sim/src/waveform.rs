//! Reducing a result vector to the samples a plot of a given width can show, and measuring
//! it exactly between two points.

use std::borrow::Cow;

use crate::ngspice::Plot;

/// One point of a waveform, in SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f64,
    pub y: f64,
}

/// A vector against the plot's scale.
#[derive(Debug, Clone)]
pub struct Series<'a> {
    /// The scale, or its base-10 logarithm when `log_x` is set.
    pub x: Cow<'a, [f64]>,
    pub y: Cow<'a, [f64]>,
    /// The scale is a frequency, plotted and interpolated on a log axis. The methods take
    /// and return frequencies all the same.
    pub log_x: bool,
}

/// What an `.ac` result shows of each complex value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcQuantity {
    /// The magnitude in decibels, `20 log10 |v|`.
    MagnitudeDb,
    /// The phase in degrees, unwrapped so that it never jumps by more than 180°.
    PhaseDeg,
}

/// The statistics of a waveform over a span of x, in the units of its vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurements {
    /// The span measured, clipped to the samples the waveform has.
    pub from: f64,
    pub to: f64,
    pub min: f64,
    pub max: f64,
    /// The mean over x, so uneven steps are weighted by their width.
    pub average: f64,
    pub rms: f64,
    pub peak_to_peak: f64,
}

impl Plot {
    /// The vector `name` against the plot's scale. Without a scale, as for an operating
    /// point, x is the sample index. `None` when the plot has no vector of that name.
    pub fn series(&self, name: &str) -> Option<Series<'_>> {
        let y = self.vector(name)?;
        let scale = self.scale.as_deref().and_then(|scale| self.vector(scale));
        let x = match scale {
            Some(x) => Cow::Borrowed(x),
            None => Cow::Owned((0..y.len()).map(|index| index as f64).collect()),
        };
        Some(Series {
            x,
            y: Cow::Borrowed(y),
            log_x: false,
        })
    }

    /// The complex vector `name` of an `.ac` plot as `quantity`, against log frequency.
    /// `None` when the plot has no such vector, the vector is real, or the plot has no
    /// scale. A magnitude of zero has no decibel value and becomes NaN, so it is skipped.
    pub fn ac_series(&self, name: &str, quantity: AcQuantity) -> Option<Series<'_>> {
        let vector = self.find(name)?;
        let imaginary = vector.imaginary.as_deref()?;
        let scale = self.vector(self.scale.as_deref()?)?;
        let parts = vector.values.iter().zip(imaginary);
        let y: Vec<f64> = match quantity {
            AcQuantity::MagnitudeDb => parts
                .map(|(re, im)| {
                    let db = 20.0 * re.hypot(*im).log10();
                    if db.is_finite() { db } else { f64::NAN }
                })
                .collect(),
            AcQuantity::PhaseDeg => {
                let mut phase: Vec<f64> =
                    parts.map(|(re, im)| im.atan2(*re).to_degrees()).collect();
                unwrap_degrees(&mut phase);
                phase
            }
        };
        Some(Series {
            x: Cow::Owned(scale.iter().copied().map(log_bound).collect()),
            y: Cow::Owned(y),
            log_x: true,
        })
    }

    /// The vector `name` reduced with [`decimate`]. `None` when the plot has no vector of
    /// that name.
    pub fn waveform(
        &self,
        name: &str,
        min_x: f64,
        max_x: f64,
        budget: usize,
    ) -> Option<Vec<Sample>> {
        Some(self.series(name)?.decimate(min_x, max_x, budget))
    }
}

/// The base-10 logarithm of a frequency bound, where zero or less means no lower bound.
fn log_bound(value: f64) -> f64 {
    if value <= 0.0 {
        f64::NEG_INFINITY
    } else {
        value.log10()
    }
}

/// Removes the 360° jumps from a phase in degrees, so each step is at most 180°. NaN
/// samples are left as they are and skipped.
pub fn unwrap_degrees(phase: &mut [f64]) {
    let mut previous: Option<f64> = None;
    for value in phase.iter_mut().filter(|value| !value.is_nan()) {
        if let Some(previous) = previous {
            *value -= 360.0 * ((*value - previous) / 360.0).round();
        }
        previous = Some(*value);
    }
}

impl Series<'_> {
    fn to_x(&self, at: f64) -> f64 {
        if self.log_x { log_bound(at) } else { at }
    }

    fn from_x(&self, x: f64) -> f64 {
        if self.log_x { 10f64.powf(x) } else { x }
    }

    /// The series reduced with [`decimate`] between `min_x` and `max_x`.
    pub fn decimate(&self, min_x: f64, max_x: f64, budget: usize) -> Vec<Sample> {
        let (min_x, max_x) = (self.to_x(min_x), self.to_x(max_x));
        let mut samples = decimate(&self.x, &self.y, min_x, max_x, budget);
        if self.log_x {
            for sample in &mut samples {
                sample.x = self.from_x(sample.x);
            }
        }
        samples
    }

    /// See [`measure`]. On a log axis, the span and the straight lines between samples are
    /// taken on log frequency, so the average and RMS are over log frequency too.
    pub fn measure(&self, from: f64, to: f64) -> Option<Measurements> {
        let measured = measure(&self.x, &self.y, self.to_x(from), self.to_x(to))?;
        Some(Measurements {
            from: self.from_x(measured.from),
            to: self.from_x(measured.to),
            ..measured
        })
    }

    /// See [`value_at`]. On a log axis, samples are joined by straight lines in log
    /// frequency.
    pub fn value_at(&self, at: f64) -> Option<f64> {
        value_at(&self.x, &self.y, self.to_x(at))
    }
}

/// The samples of `y` against `x` without NaN coordinates, or `None` unless some remain
/// and `x` never decreases.
fn increasing_samples(x: &[f64], y: &[f64]) -> Option<Vec<Sample>> {
    let samples: Vec<Sample> = x
        .iter()
        .zip(y)
        .filter(|(x, y)| !x.is_nan() && !y.is_nan())
        .map(|(&x, &y)| Sample { x, y })
        .collect();
    let increasing = samples.windows(2).all(|pair| pair[0].x <= pair[1].x);
    (!samples.is_empty() && increasing).then_some(samples)
}

/// The value at `at` of the line through `samples`, which must be in x order and include
/// `at`. Where x repeats, the last sample at that x wins.
fn interpolate(samples: &[Sample], at: f64) -> f64 {
    let after = samples.partition_point(|sample| sample.x <= at);
    if after == 0 {
        return samples[0].y;
    }
    let before = samples[after - 1];
    match samples.get(after) {
        Some(next) if before.x < at => {
            let fraction = (at - before.x) / (next.x - before.x);
            before.y + (next.y - before.y) * fraction
        }
        _ => before.y,
    }
}

/// The value of `y` at `x = at`, joining samples with straight lines as the plot does.
/// `None` when `at` is outside the samples, when no sample is left once NaN samples are
/// dropped, or when `x` ever decreases.
pub fn value_at(x: &[f64], y: &[f64], at: f64) -> Option<f64> {
    let samples = increasing_samples(x, y)?;
    let first = samples[0].x;
    let last = samples[samples.len() - 1].x;
    (at >= first && at <= last).then(|| interpolate(&samples, at))
}

/// The minimum, maximum, average, RMS, and peak-to-peak value of `y` between `x = from`
/// and `x = to`, treating the waveform as straight lines between samples, so the average
/// and RMS are exact for that line whatever the step sizes. The span is clipped to the
/// samples. A span of zero width measures the single value there.
///
/// `None` when the span is reversed or NaN, misses the samples entirely, no sample is left
/// once NaN samples are dropped, or `x` ever decreases.
pub fn measure(x: &[f64], y: &[f64], from: f64, to: f64) -> Option<Measurements> {
    if from.is_nan() || to.is_nan() || from > to {
        return None;
    }
    let samples = increasing_samples(x, y)?;
    let first = samples[0].x;
    let last = samples[samples.len() - 1].x;
    if to < first || from > last {
        return None;
    }
    let from = from.max(first);
    let to = to.min(last);
    let start = interpolate(&samples, from);
    if to == from {
        return Some(Measurements {
            from,
            to,
            min: start,
            max: start,
            average: start,
            rms: start.abs(),
            peak_to_peak: 0.0,
        });
    }
    let inside = samples
        .iter()
        .filter(|sample| sample.x > from && sample.x < to)
        .copied();
    let end = Sample {
        x: to,
        y: interpolate(&samples, to),
    };
    let mut previous = Sample { x: from, y: start };
    let (mut min, mut max) = (start, start);
    let (mut area, mut square_area) = (0.0, 0.0);
    for sample in inside.chain(std::iter::once(end)) {
        let width = sample.x - previous.x;
        let (a, b) = (previous.y, sample.y);
        area += width * (a + b) / 2.0;
        square_area += width * (a * a + a * b + b * b) / 3.0;
        min = min.min(b);
        max = max.max(b);
        previous = sample;
    }
    let span = to - from;
    Some(Measurements {
        from,
        to,
        min,
        max,
        average: area / span,
        rms: (square_area / span).max(0.0).sqrt(),
        peak_to_peak: max - min,
    })
}

/// The samples of `y` against `x` to draw between `min_x` and `max_x`, at most `budget`
/// of them (a budget below 2 counts as 2).
///
/// Samples with a NaN coordinate are dropped. The nearest sample outside each end of the
/// range is kept so that lines reach the edges. When more samples remain than the budget,
/// the range is split into `budget / 2` equal spans of x, and each span keeps its lowest
/// and highest sample, in x order, so peaks survive. If `x` ever decreases, the range is
/// ignored and the spans are runs of equal sample count instead. A range whose start is
/// after its end yields nothing, and a single sample is returned whatever the range.
pub fn decimate(x: &[f64], y: &[f64], min_x: f64, max_x: f64, budget: usize) -> Vec<Sample> {
    let budget = budget.max(2);
    if min_x.is_nan() || max_x.is_nan() || min_x > max_x {
        return Vec::new();
    }
    let samples: Vec<Sample> = x
        .iter()
        .zip(y)
        .filter(|(x, y)| !x.is_nan() && !y.is_nan())
        .map(|(&x, &y)| Sample { x, y })
        .collect();
    if samples.len() <= 1 {
        return samples;
    }
    let spans = budget / 2;
    let increasing = samples.windows(2).all(|pair| pair[0].x <= pair[1].x);
    if !increasing {
        let per_span = samples.len().div_ceil(spans);
        return min_max(&samples, |index, _| index / per_span);
    }

    let first_inside = samples.partition_point(|sample| sample.x < min_x);
    let end_inside = samples.partition_point(|sample| sample.x <= max_x);
    if end_inside == 0 || first_inside == samples.len() {
        return Vec::new();
    }
    let start = first_inside.saturating_sub(1);
    let end = (end_inside + 1).min(samples.len());
    let visible = &samples[start..end];
    if visible.len() <= budget {
        return visible.to_vec();
    }
    let low = visible[0].x;
    let width = (visible[visible.len() - 1].x - low) / spans as f64;
    if !width.is_finite() || width <= 0.0 {
        let per_span = visible.len().div_ceil(spans);
        return min_max(visible, |index, _| index / per_span);
    }
    min_max(visible, |_, sample| {
        (((sample.x - low) / width) as usize).min(spans - 1)
    })
}

/// The lowest and highest sample of each group, in sample order. `group` must never
/// decrease along `samples`.
fn min_max(samples: &[Sample], group: impl Fn(usize, Sample) -> usize) -> Vec<Sample> {
    let mut kept = Vec::new();
    let mut keep = |low: usize, high: usize| {
        kept.push(samples[low.min(high)]);
        if low != high {
            kept.push(samples[low.max(high)]);
        }
    };
    let mut current: Option<(usize, usize, usize)> = None;
    for (index, &sample) in samples.iter().enumerate() {
        let id = group(index, sample);
        match current.as_mut() {
            Some((current_id, low, high)) if *current_id == id => {
                if sample.y < samples[*low].y {
                    *low = index;
                }
                if sample.y > samples[*high].y {
                    *high = index;
                }
            }
            _ => {
                if let Some((_, low, high)) = current {
                    keep(low, high);
                }
                current = Some((id, index, index));
            }
        }
    }
    if let Some((_, low, high)) = current {
        keep(low, high);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(count: usize) -> (Vec<f64>, Vec<f64>) {
        let x: Vec<f64> = (0..count).map(|index| index as f64).collect();
        let y = x.iter().map(|x| (x / 10.0).sin()).collect();
        (x, y)
    }

    #[test]
    fn long_waveforms_fit_the_budget_and_keep_their_peaks() {
        let (x, mut y) = ramp(10_000);
        y[4321] = 50.0;
        y[8765] = -50.0;
        let samples = decimate(&x, &y, 0.0, 9_999.0, 200);
        assert!(samples.len() <= 200, "{}", samples.len());
        assert!(samples.len() >= 100, "{}", samples.len());
        assert!(samples.contains(&Sample { x: 4321.0, y: 50.0 }));
        assert!(samples.contains(&Sample {
            x: 8765.0,
            y: -50.0
        }));
        assert!(samples.windows(2).all(|pair| pair[0].x < pair[1].x));
    }

    #[test]
    fn short_ranges_return_every_sample_with_one_neighbour_each_side() {
        let (x, y) = ramp(100);
        let samples = decimate(&x, &y, 10.5, 13.0, 50);
        let xs: Vec<f64> = samples.iter().map(|sample| sample.x).collect();
        assert_eq!(xs, [10.0, 11.0, 12.0, 13.0, 14.0]);
        let between = decimate(&x, &y, 10.2, 10.8, 50);
        assert_eq!(between.len(), 2);
        assert!(decimate(&x, &y, 200.0, 300.0, 50).is_empty());
        assert!(decimate(&x, &y, -9.0, -1.0, 50).is_empty());
    }

    #[test]
    fn degenerate_inputs_have_defined_results() {
        let (x, y) = ramp(100);
        assert!(decimate(&x, &y, 5.0, 1.0, 50).is_empty());
        assert!(decimate(&x, &y, f64::NAN, 1.0, 50).is_empty());
        assert_eq!(decimate(&x, &y, 0.0, 99.0, 0).len(), 2);
        assert!(decimate(&[], &[], 0.0, 1.0, 50).is_empty());
        let single = decimate(&[0.0], &[2.5], 10.0, 20.0, 50);
        assert_eq!(single, [Sample { x: 0.0, y: 2.5 }]);
        let with_nan = decimate(&[0.0, 1.0, 2.0], &[1.0, f64::NAN, 3.0], 0.0, 2.0, 50);
        assert_eq!(with_nan.len(), 2);
    }

    #[test]
    fn decreasing_scales_are_reduced_by_sample_count() {
        let x: Vec<f64> = (0..1_000).map(|index| f64::from(index % 10)).collect();
        let y: Vec<f64> = (0..1_000).map(f64::from).collect();
        let samples = decimate(&x, &y, 100.0, 200.0, 20);
        assert_eq!(samples.len(), 20);
        assert_eq!(samples[0].y, 0.0);
        assert_eq!(samples[19].y, 999.0);
    }

    fn close(actual: f64, expected: f64) {
        let tolerance = 1e-12 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn uneven_steps_are_weighted_by_their_width() {
        // In milliseconds: 0 V for 1 ms, then a ramp to 2 V over 1 ms in one step, then
        // 2 V for 2 ms sampled densely.
        let x = [0.0, 0.25, 0.5, 1.0, 2.0, 2.5, 3.0, 3.5, 4.0];
        let y = [0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 2.0, 2.0];
        let measured = measure(&x, &y, 0.0, 4.0).unwrap();
        assert_eq!((measured.from, measured.to), (0.0, 4.0));
        assert_eq!((measured.min, measured.max), (0.0, 2.0));
        assert_eq!(measured.peak_to_peak, 2.0);
        // Area: 1 ms of the ramp at 1 V on average, then 2 ms at 2 V.
        close(measured.average, 5.0 / 4.0);
        // Square area: the ramp gives 4/3 V² ms, and the flat part 8 V² ms.
        close(measured.rms, ((4.0 / 3.0 + 8.0) / 4.0_f64).sqrt());
    }

    #[test]
    fn a_span_between_samples_is_cut_by_interpolation() {
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 10.0, 0.0];
        let measured = measure(&x, &y, 0.5, 1.5).unwrap();
        assert_eq!((measured.min, measured.max), (5.0, 10.0));
        close(measured.average, 7.5);
        let peak = measure(&x, &y, 0.8, 0.9).unwrap();
        close(peak.min, 8.0);
        close(peak.max, 9.0);
        close(peak.average, 8.5);
    }

    #[test]
    fn a_sine_over_whole_periods_has_zero_mean_and_its_known_rms() {
        let count = 10_001;
        let x: Vec<f64> = (0..count)
            .map(|index| index as f64 / (count - 1) as f64)
            .collect();
        let tau = std::f64::consts::TAU;
        let y: Vec<f64> = x.iter().map(|x| 3.0 * (tau * 5.0 * x).sin()).collect();
        let measured = measure(&x, &y, f64::NEG_INFINITY, f64::INFINITY).unwrap();
        assert_eq!((measured.from, measured.to), (0.0, 1.0));
        assert!(measured.average.abs() < 1e-9, "{}", measured.average);
        let rms = 3.0 / 2.0_f64.sqrt();
        assert!((measured.rms - rms).abs() < 1e-4, "{}", measured.rms);
        assert!((measured.peak_to_peak - 6.0).abs() < 1e-3);
    }

    #[test]
    fn spans_are_clipped_and_degenerate_spans_have_defined_results() {
        let x = [1.0, 2.0, 3.0];
        let y = [-4.0, 4.0, 4.0];
        let clipped = measure(&x, &y, 0.0, 2.0).unwrap();
        assert_eq!((clipped.from, clipped.to), (1.0, 2.0));
        close(clipped.average, 0.0);
        let point = measure(&x, &y, 1.5, 1.5).unwrap();
        assert_eq!((point.min, point.max, point.average), (0.0, 0.0, 0.0));
        assert_eq!(point.peak_to_peak, 0.0);
        let negative = measure(&x, &y, 1.0, 1.0).unwrap();
        assert_eq!((negative.average, negative.rms), (-4.0, 4.0));
        assert!(measure(&x, &y, 2.0, 1.0).is_none());
        assert!(measure(&x, &y, f64::NAN, 1.0).is_none());
        assert!(measure(&x, &y, 4.0, 5.0).is_none());
        assert!(measure(&[], &[], 0.0, 1.0).is_none());
        assert!(measure(&[2.0, 1.0], &[0.0, 0.0], 0.0, 3.0).is_none());
    }

    #[test]
    fn values_are_interpolated_between_samples_and_missing_outside_them() {
        let x = [0.0, 1.0, 1.0, 3.0];
        let y = [0.0, 2.0, 6.0, 2.0];
        assert_eq!(value_at(&x, &y, 0.0), Some(0.0));
        assert_eq!(value_at(&x, &y, 0.25), Some(0.5));
        // At a repeated x, the later sample wins, as at a step in a transient.
        assert_eq!(value_at(&x, &y, 1.0), Some(6.0));
        assert_eq!(value_at(&x, &y, 2.0), Some(4.0));
        assert_eq!(value_at(&x, &y, 3.0), Some(2.0));
        assert_eq!(value_at(&x, &y, -0.1), None);
        assert_eq!(value_at(&x, &y, 3.1), None);
        assert_eq!(value_at(&x, &y, f64::NAN), None);
        let with_nan = [1.0, f64::NAN, 3.0];
        assert_eq!(value_at(&[0.0, 1.0, 2.0], &with_nan, 1.0), Some(2.0));
    }

    #[test]
    fn a_plot_without_a_scale_is_measured_by_sample_index() {
        use crate::ngspice::Vector;
        let plot = Plot {
            name: "op1".to_owned(),
            scale: None,
            vectors: vec![Vector {
                name: "/out".to_owned(),
                values: vec![1.5],
                imaginary: None,
            }],
        };
        let series = plot.series("/out").unwrap();
        assert_eq!(series.value_at(0.0), Some(1.5));
        let measured = series.measure(0.0, 0.0).unwrap();
        assert_eq!(measured.average, 1.5);
        assert!(plot.series("/in").is_none());
    }

    #[test]
    fn phase_is_unwrapped_across_the_half_turn() {
        let mut phase = [170.0, 179.0, -179.0, -170.0, f64::NAN, 175.0, -10.0];
        unwrap_degrees(&mut phase);
        assert_eq!(phase[..4], [170.0, 179.0, 181.0, 190.0]);
        assert!(phase[4].is_nan());
        assert_eq!(phase[5..], [175.0, 350.0]);
    }

    /// A first-order low-pass, `1 / (1 + j f / cutoff)`, ten points per decade.
    fn low_pass_ac(cutoff: f64, magnitude: f64) -> Plot {
        use crate::ngspice::Vector;
        let frequency: Vec<f64> = (0..=60)
            .map(|index| 10f64.powf(f64::from(index) / 10.0))
            .collect();
        let (real, imaginary): (Vec<f64>, Vec<f64>) = frequency
            .iter()
            .map(|f| {
                let w = f / cutoff;
                let denominator = 1.0 + w * w;
                (magnitude / denominator, -magnitude * w / denominator)
            })
            .unzip();
        Plot {
            name: "ac1".to_owned(),
            scale: Some("frequency".to_owned()),
            vectors: vec![
                Vector {
                    name: "frequency".to_owned(),
                    values: frequency,
                    imaginary: None,
                },
                Vector {
                    name: "/out".to_owned(),
                    values: real,
                    imaginary: Some(imaginary),
                },
            ],
        }
    }

    #[test]
    fn ac_vectors_are_shown_in_decibels_and_degrees_against_log_frequency() {
        let plot = low_pass_ac(1e3, 1.0);
        let magnitude = plot.ac_series("/out", AcQuantity::MagnitudeDb).unwrap();
        assert!(magnitude.log_x);
        let at_cutoff = magnitude.value_at(1e3).unwrap();
        assert!((at_cutoff + 10.0 * 2f64.log10()).abs() < 1e-9, "{at_cutoff}");
        let between = magnitude.value_at(10f64.powf(0.05)).unwrap();
        let expected = (magnitude.y[0] + magnitude.y[1]) / 2.0;
        close(between, expected);
        assert_eq!(magnitude.value_at(0.0), None);

        let phase = plot.ac_series("/out", AcQuantity::PhaseDeg).unwrap();
        close(phase.value_at(1e3).unwrap(), -45.0);
        let measured = phase.measure(10.0, 1e5).unwrap();
        close(measured.from, 10.0);
        close(measured.to, 1e5);
        assert!(measured.max < 0.0 && measured.min > -90.0);

        let samples = magnitude.decimate(0.0, f64::INFINITY, 1_000);
        assert_eq!(samples.len(), 61);
        close(samples[0].x, 1.0);
        close(samples[60].x, 1e6);
        // A decade each side of the cutoff, and the neighbouring sample past each end.
        let zoomed = magnitude.decimate(100.0, 1e4, 1_000);
        assert_eq!(zoomed.len(), 23);
        assert!((zoomed[0].x.log10() - 1.9).abs() < 1e-9);
        assert!((zoomed[22].x.log10() - 4.1).abs() < 1e-9);

        assert!(plot.ac_series("frequency", AcQuantity::MagnitudeDb).is_none());
        assert!(plot.ac_series("/in", AcQuantity::PhaseDeg).is_none());
    }

    #[test]
    fn a_zero_magnitude_has_no_decibel_value() {
        let plot = low_pass_ac(1e3, 0.0);
        let magnitude = plot.ac_series("/out", AcQuantity::MagnitudeDb).unwrap();
        assert!(magnitude.y.iter().all(|db| db.is_nan()));
        assert_eq!(magnitude.value_at(1e3), None);
        assert!(magnitude.decimate(0.0, f64::INFINITY, 100).is_empty());
    }
}
