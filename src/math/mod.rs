//! Linear algebra and trigonometry.

mod angle;
mod proj;
mod quat;
mod trans;

use core::simd::f32x4;

pub use angle::*;
#[cfg(not(test))]
pub use proj::*;
pub use quat::*;
#[cfg(not(test))]
pub use trans::*;

#[cfg(not(test))]
use crate::prim::*;
use crate::simd::*;

/// Tolerance for tests and generated values that depend on infinite series.
const TOLERANCE: f32 = 1.0 / 256.0;

#[cfg(test)]
#[track_caller]
fn expect_roughly(actual: f32, expected: f32)
{
    let passed = (expected - TOLERANCE ..= expected + TOLERANCE).contains(&actual);
    assert!(passed, "Value {actual} isn't anywhere close to {expected}");
}

#[cfg(test)]
#[track_caller]
fn expect_roughly_vec(actual: f32x4, expected: f32x4)
{
    for idx in 0 .. 4 {
        let actual_val = actual[idx];
        let expected_val = expected[idx];
        let passed = (expected_val - TOLERANCE ..= expected_val + TOLERANCE).contains(&actual_val);
        assert!(passed,
                "Value {actual:?} isn't anywhere close to {expected:?} at index {idx}");
    }
}

#[cfg(test)]
#[track_caller]
fn expect_roughly_mat(actual: f32x4x4, expected: f32x4x4)
{
    for idx in 0 .. 16 {
        let actual_val = actual.get(idx);
        let expected_val = expected.get(idx);
        let passed = (expected_val - TOLERANCE ..= expected_val + TOLERANCE).contains(&actual_val);
        assert!(passed,
                "Value {actual:?} isn't anywhere close to {expected:?} at index {idx}");
    }
}
