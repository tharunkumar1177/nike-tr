//! Reducing a result vector to the samples a plot of a given width can show.

use crate::ngspice::Plot;

/// One point of a waveform, in SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f64,
    pub y: f64,
}

impl Plot {
    /// The vector `name` against the plot's scale, reduced with [`decimate`]. Without a
    /// scale, as for an operating point, x is the sample index. `None` when the plot has
    /// no vector of that name.
    pub fn waveform(
        &self,
        name: &str,
        min_x: f64,
        max_x: f64,
        budget: usize,
    ) -> Option<Vec<Sample>> {
        let y = self.vector(name)?;
        let scale = self.scale.as_deref().and_then(|scale| self.vector(scale));
        let samples = match scale {
            Some(x) => decimate(x, y, min_x, max_x, budget),
            None => {
                let x: Vec<f64> = (0..y.len()).map(|index| index as f64).collect();
                decimate(&x, y, min_x, max_x, budget)
            }
        };
        Some(samples)
    }
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
}
