//! Integer length and angle units for document geometry.
//!
//! Lengths are `i64` nanometers and angles are `i32` tenths of a degree in
//! `[0, 3600)`. Floating point appears only at the conversion boundary.

use std::fmt;
use std::ops::{Add, Neg, Sub};

pub const NM_PER_MM: i64 = 1_000_000;
pub const NM_PER_MIL: i64 = 25_400;
pub const DECIDEGREES_PER_TURN: i32 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitError {
    NonFinite,
    OutOfRange,
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitError::NonFinite => f.write_str("value is not a finite number"),
            UnitError::OutOfRange => f.write_str("value exceeds the representable range"),
        }
    }
}

impl std::error::Error for UnitError {}

fn scale_to_i64(value: f64, factor: f64) -> Result<i64, UnitError> {
    if !value.is_finite() {
        return Err(UnitError::NonFinite);
    }
    let scaled = (value * factor).round();
    // `i64::MAX as f64` rounds up to 2^63, so a strict comparison keeps the cast exact.
    if scaled >= i64::MAX as f64 || scaled < i64::MIN as f64 {
        return Err(UnitError::OutOfRange);
    }
    Ok(scaled as i64)
}

/// A length in nanometers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Nm(pub i64);

impl Nm {
    pub const ZERO: Nm = Nm(0);

    /// Converts millimeters to the nearest nanometer.
    pub fn from_mm(mm: f64) -> Result<Nm, UnitError> {
        scale_to_i64(mm, NM_PER_MM as f64).map(Nm)
    }

    /// Converts thousandths of an inch to the nearest nanometer.
    pub fn from_mils(mils: f64) -> Result<Nm, UnitError> {
        scale_to_i64(mils, NM_PER_MIL as f64).map(Nm)
    }

    pub fn to_mm(self) -> f64 {
        self.0 as f64 / NM_PER_MM as f64
    }

    pub fn to_mils(self) -> f64 {
        self.0 as f64 / NM_PER_MIL as f64
    }

    pub fn checked_add(self, other: Nm) -> Option<Nm> {
        self.0.checked_add(other.0).map(Nm)
    }

    pub fn checked_sub(self, other: Nm) -> Option<Nm> {
        self.0.checked_sub(other.0).map(Nm)
    }

    pub fn abs(self) -> Nm {
        Nm(self.0.abs())
    }
}

impl Add for Nm {
    type Output = Nm;

    fn add(self, other: Nm) -> Nm {
        Nm(self.0 + other.0)
    }
}

impl Sub for Nm {
    type Output = Nm;

    fn sub(self, other: Nm) -> Nm {
        Nm(self.0 - other.0)
    }
}

impl Neg for Nm {
    type Output = Nm;

    fn neg(self) -> Nm {
        Nm(-self.0)
    }
}

/// A position in a document coordinate space. X grows right, Y grows down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: Nm,
    pub y: Nm,
}

impl Point {
    pub const ORIGIN: Point = Point {
        x: Nm::ZERO,
        y: Nm::ZERO,
    };

    pub fn new(x: Nm, y: Nm) -> Point {
        Point { x, y }
    }

    pub fn offset(self, dx: Nm, dy: Nm) -> Point {
        Point {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

/// An angle in tenths of a degree, always normalized to `[0, 3600)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Angle(i32);

impl Angle {
    pub const ZERO: Angle = Angle(0);

    pub fn from_decidegrees(decidegrees: i32) -> Angle {
        Angle(decidegrees.rem_euclid(DECIDEGREES_PER_TURN))
    }

    /// Converts degrees to the nearest tenth of a degree.
    pub fn from_degrees(degrees: f64) -> Result<Angle, UnitError> {
        if !degrees.is_finite() {
            return Err(UnitError::NonFinite);
        }
        let turns_removed = (degrees % 360.0) * 10.0;
        Ok(Angle::from_decidegrees(turns_removed.round() as i32))
    }

    pub fn decidegrees(self) -> i32 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        f64::from(self.0) / 10.0
    }

    pub fn rotate(self, by: Angle) -> Angle {
        Angle::from_decidegrees(self.0 + by.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimeters_convert_to_exact_nanometers() {
        assert_eq!(Nm::from_mm(1.0), Ok(Nm(1_000_000)));
        assert_eq!(Nm::from_mm(1.27), Ok(Nm(1_270_000)));
        assert_eq!(Nm::from_mm(-0.1), Ok(Nm(-100_000)));
        assert_eq!(Nm::from_mm(0.0), Ok(Nm::ZERO));
    }

    #[test]
    fn mils_convert_to_exact_nanometers() {
        assert_eq!(Nm::from_mils(1.0), Ok(Nm(25_400)));
        assert_eq!(Nm::from_mils(50.0), Nm::from_mm(1.27));
    }

    #[test]
    fn sub_nanometer_input_rounds_to_nearest() {
        assert_eq!(Nm::from_mm(0.000_000_4), Ok(Nm(0)));
        assert_eq!(Nm::from_mm(0.000_001_6), Ok(Nm(2)));
    }

    #[test]
    fn non_finite_and_overflowing_lengths_are_rejected() {
        assert_eq!(Nm::from_mm(f64::NAN), Err(UnitError::NonFinite));
        assert_eq!(Nm::from_mm(f64::INFINITY), Err(UnitError::NonFinite));
        assert_eq!(Nm::from_mm(1e13), Err(UnitError::OutOfRange));
        assert_eq!(Nm::from_mm(-1e13), Err(UnitError::OutOfRange));
    }

    #[test]
    fn nanometers_convert_back_to_display_units() {
        assert_eq!(Nm(2_540_000).to_mm(), 2.54);
        assert_eq!(Nm(2_540_000).to_mils(), 100.0);
    }

    #[test]
    fn checked_arithmetic_reports_overflow() {
        assert_eq!(Nm(i64::MAX).checked_add(Nm(1)), None);
        assert_eq!(Nm(i64::MIN).checked_sub(Nm(1)), None);
        assert_eq!(Nm(5).checked_add(Nm(-7)), Some(Nm(-2)));
    }

    #[test]
    fn points_offset_componentwise() {
        let p = Point::new(Nm(10), Nm(20)).offset(Nm(-15), Nm(5));
        assert_eq!(p, Point::new(Nm(-5), Nm(25)));
    }

    #[test]
    fn angles_normalize_into_one_turn() {
        assert_eq!(Angle::from_decidegrees(3600).decidegrees(), 0);
        assert_eq!(Angle::from_decidegrees(-900).decidegrees(), 2700);
        assert_eq!(Angle::from_decidegrees(7250).decidegrees(), 50);
        let half_turn = Angle::from_decidegrees(1800);
        assert_eq!(
            Angle::from_decidegrees(2700)
                .rotate(half_turn)
                .decidegrees(),
            900
        );
    }

    #[test]
    fn degrees_convert_to_tenths() {
        assert_eq!(Angle::from_degrees(90.0).map(Angle::decidegrees), Ok(900));
        assert_eq!(Angle::from_degrees(-45.0).map(Angle::decidegrees), Ok(3150));
        assert_eq!(Angle::from_degrees(720.25).map(Angle::decidegrees), Ok(3));
        assert_eq!(Angle::from_degrees(f64::NAN), Err(UnitError::NonFinite));
        assert_eq!(Angle::from_decidegrees(455).degrees(), 45.5);
    }
}
