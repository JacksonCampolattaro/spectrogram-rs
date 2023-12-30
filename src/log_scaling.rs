// use std::ops::Range;
// use num_traits::Float;
// use plotters::coord::ranged1d::{DefaultFormatting, KeyPointHint, ReversibleRanged};
// use plotters::coord::types::RangedCoordf64;
// use plotters::prelude::{LogCoord, LogScalable, Ranged};

use std::marker::PhantomData;
use std::ops::Range;
use plotters::coord::ranged1d::{DefaultFormatting, KeyPointHint, ReversibleRanged};
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::{LogCoord, LogScalable, Ranged};

#[derive(Clone)]
pub struct LogCoordf64 {
    linear: RangedCoordf64,
    logic: Range<f64>,
    normalized: Range<f64>,
    base: f64,
    zero_point: f64,
    negative: bool,
    marker: PhantomData<f64>,
}

impl LogCoordf64 {
    fn value_to_f64(&self, value: &f64) -> f64 {
        // todo: this needs a better name
        let fv = value - self.zero_point;
        if self.negative {
            -fv
        } else {
            fv
        }
    }

    fn f64_to_value(&self, fv: f64) -> f64 {
        let fv = if self.negative { -fv } else { fv };
        fv + self.zero_point
    }

    fn is_inf(&self, fv: f64) -> bool {
        let fv = if self.negative { -fv } else { fv };
        let a = fv + self.zero_point;
        let b = self.zero_point;

        (a - b).abs() < f64::EPSILON
    }
}

impl Ranged for LogCoordf64 {
    type FormatOption = DefaultFormatting;
    type ValueType = f64;

    fn map(&self, value: &f64, limit: (i32, i32)) -> i32 {
        let fv = self.value_to_f64(value);
        let value_ln = fv.ln();
        self.linear.map(&value_ln, limit)
    }

    fn key_points<Hint: KeyPointHint>(&self, hint: Hint) -> Vec<Self::ValueType> {
        let max_points = hint.max_num_points();

        let base = self.base;
        let base_ln = base.ln();

        let Range { mut start, mut end } = self.normalized;

        if start > end {
            std::mem::swap(&mut start, &mut end);
        }

        let bold_count = ((end / start).ln().abs() / base_ln).floor().max(1.0) as usize;

        let light_density = if max_points < bold_count {
            0
        } else {
            let density = 1 + (max_points - bold_count) / bold_count;
            let mut exp = 1;
            while exp * 10 <= density {
                exp *= 10;
            }
            exp - 1
        };

        let mut multiplier = base;
        let mut cnt = 1;
        while max_points < bold_count / cnt {
            multiplier *= base;
            cnt += 1;
        }

        let mut ret = vec![];
        let mut val = (base).powf((start.ln() / base_ln).ceil());

        while val <= end {
            if !self.is_inf(val) {
                ret.push(self.f64_to_value(val));
            }
            for i in 1..=light_density {
                let v = val
                    * (1.0
                    + multiplier / f64::from(light_density as u32 + 1) * f64::from(i as u32));
                if v > end {
                    break;
                }
                if !self.is_inf(val) {
                    ret.push(self.f64_to_value(v));
                }
            }
            val *= multiplier;
        }

        ret
    }

    fn range(&self) -> Range<f64> {
        self.logic.clone()
    }
}

impl ReversibleRanged for LogCoordf64 {
    fn unmap(&self, input: i32, limit: (i32, i32)) -> Option<Self::ValueType> {
        let linear_value = self.linear.unmap(input, limit).unwrap();
        Some(self.f64_to_value(linear_value.exp()))
    }
}

pub trait IntoReversibleLogRange {
    /// Make the log scale coordinate
    fn reversible_log_scale(self) -> ReversibleLogRangeExt;
}

impl IntoReversibleLogRange for Range<f64> {
    fn reversible_log_scale(self) -> ReversibleLogRangeExt {
        ReversibleLogRangeExt {
            range: self,
            zero: 0.0,
            base: 10.0,
        }
    }
}

#[derive(Clone)]
pub struct ReversibleLogRangeExt {
    range: Range<f64>,
    zero: f64,
    base: f64,
}

impl ReversibleLogRangeExt {
    /// Set the zero point of the log scale coordinate. Zero point is the point where we map -inf
    /// of the axis to the coordinate
    pub fn zero_point(mut self, value: f64) -> Self {
        self.zero = value;
        self
    }

    /// Set the base multipler
    pub fn base(mut self, base: f64) -> Self {
        if self.base > 1.0 {
            self.base = base;
        }
        self
    }
}

impl From<ReversibleLogRangeExt> for LogCoordf64 {
    fn from(spec: ReversibleLogRangeExt) -> LogCoordf64 {
        let zero_point = spec.zero;
        let mut start = spec.range.start.as_f64() - zero_point;
        let mut end = spec.range.end.as_f64() - zero_point;
        let negative = if start < 0.0 || end < 0.0 {
            start = -start;
            end = -end;
            true
        } else {
            false
        };

        if start < end {
            if start == 0.0 {
                start = start.max(end * 1e-5);
            }
        } else if end == 0.0 {
            end = end.max(start * 1e-5);
        }

        LogCoordf64 {
            linear: (start.ln()..end.ln()).into(),
            logic: spec.range,
            normalized: start..end,
            base: spec.base,
            zero_point,
            negative,
            marker: PhantomData,
        }
    }
}


//#[derive(Clone)]
// pub struct ReversibleLogCoord<V: LogScalable>(pub LogCoord<V>);
//
// impl ReversibleRanged for ReversibleLogCoord<f64> {
//     fn unmap(&self, input: i32, limit: (i32, i32)) -> Option<Self::ValueType> {
//         let range = self.0.range();
//         let linear = RangedCoordf64::from(range);
//         let linear_value = linear.unmap(input, limit);
//         Some(linear_value.unwrap().exp())
//     }
// }
//
// impl<V: LogScalable> Ranged for ReversibleLogCoord<V> {
//     type FormatOption = DefaultFormatting;
//     type ValueType = V;
//
//     fn map(&self, value: &Self::ValueType, limit: (i32, i32)) -> i32 {
//         self.0.map(value, limit)
//     }
//
//     fn key_points<Hint: KeyPointHint>(&self, hint: Hint) -> Vec<Self::ValueType> {
//         self.0.key_points(hint)
//     }
//
//     fn range(&self) -> Range<Self::ValueType> {
//         self.0.range()
//     }
// }
