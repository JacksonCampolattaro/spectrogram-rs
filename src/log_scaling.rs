use std::ops::Range;
use num_traits::Float;
use plotters::coord::ranged1d::{DefaultFormatting, KeyPointHint, ReversibleRanged};
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::{LogCoord, LogScalable, Ranged};

//#[derive(Clone)]
pub struct ReversibleLogCoord<V: LogScalable>(pub LogCoord<V>);

impl ReversibleRanged for ReversibleLogCoord<f64> {
    fn unmap(&self, input: i32, limit: (i32, i32)) -> Option<Self::ValueType> {
        let range = self.0.range();
        let linear = RangedCoordf64::from(range);
        let linear_value = linear.unmap(input, limit);
        Some(linear_value.unwrap().exp())
    }
}

impl<V: LogScalable> Ranged for ReversibleLogCoord<V> {
    type FormatOption = DefaultFormatting;
    type ValueType = V;

    fn map(&self, value: &Self::ValueType, limit: (i32, i32)) -> i32 {
        self.0.map(value, limit)
    }

    fn key_points<Hint: KeyPointHint>(&self, hint: Hint) -> Vec<Self::ValueType> {
        self.0.key_points(hint)
    }

    fn range(&self) -> Range<Self::ValueType> {
        self.0.range()
    }
}
