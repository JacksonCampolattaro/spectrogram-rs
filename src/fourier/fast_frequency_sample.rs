use std::ops::{Add, Mul, Range, Sub, SubAssign};
use std::convert::TryInto;
use std::fmt::Debug;
use cpal::SampleRate;
use num_traits::{FloatConst, Num, Zero};
use crate::fourier::{Frequency, FrequencySample, Period, StereoMagnitude};

// pub fn cubic_interpolate(y: &[StereoMagnitude], mu: f32) -> StereoMagnitude {
//     let mu2 = mu * mu;
//
//     // This handles upper end safely (<4 samples)
//     let y0 = y[0];
//     let y1 = y.get(1).unwrap_or(&y0);
//     let y2 = y.get(2).unwrap_or(&y1);
//     let y3 = y.get(3).unwrap_or(&y2);
//
//     let a0 = y3 - y2 - y0 + y1;
//     let a1 = y0 - y1 - a0;
//     let a2 = y2 - y0;
//     let a3 = y1;
//     return (a0 * mu * mu2 + a1 * mu2 + a2 * mu + a3);
// }

pub struct FastFrequencySample {
    pub prefix_sum_of_magnitudes: Vec<StereoMagnitude>,
    pub sample_rate: SampleRate,
}

impl FastFrequencySample {
    pub fn new<I>(magnitudes: I, sample_rate: SampleRate) -> Self
        where I: IntoIterator<Item=StereoMagnitude> {
        FastFrequencySample {
            prefix_sum_of_magnitudes: magnitudes.into_iter()
                .scan(StereoMagnitude::zero(), |acc, m| {
                    *acc += m;
                    Some(*acc)
                })
                .collect(),
            sample_rate,
        }
    }

    // todo: eliminate duplicate code
    fn index_of(&self, frequency: &Frequency) -> f32 {
        let index = frequency * self.period();
        assert!(0.0 < index && index < (self.prefix_sum_of_magnitudes.len() - 1) as f32);
        index as f32
    }

    fn frequency_of(&self, index: f32) -> Frequency {
        index / self.period()
    }

    fn cell_indices_of(&self, frequency: &Frequency) -> Range<usize> {
        let index = self.index_of(frequency);
        let low = index.floor() as usize;
        let high = (index.ceil() as usize).min(low + 1);
        low..high
    }

    fn frequencies_of(&self, indices: &Range<usize>) -> Range<Frequency> {
        self.frequency_of(indices.start as f32)..self.frequency_of(indices.end as f32)
    }

    fn net_magnitude_below(&self, frequency: &Frequency) -> StereoMagnitude {
        let cell_indices = self.cell_indices_of(frequency);
        let cell_magnitudes = self.prefix_sum_of_magnitudes[cell_indices.start]..self.prefix_sum_of_magnitudes[cell_indices.end];

        // Interpolation is done in frequency space, to account for log scaling
        let cell_frequencies = self.frequencies_of(&cell_indices);
        let offset = ((*frequency - cell_frequencies.start) / (cell_frequencies.end - cell_frequencies.start)) as f32;

        // The net magnitude is everything below this cell,
        // plus the portion of this cell's magnitude which should be included
        (cell_magnitudes.start * (1.0 - offset) + cell_magnitudes.end * (offset)) / 2.0
    }
}

impl FrequencySample for FastFrequencySample {
    fn period(&self) -> Period {
        2.0 * self.prefix_sum_of_magnitudes.len() as f32 / self.sample_rate.0 as f32
    }

    fn frequencies(&self) -> Range<Frequency> {
        0.0..(self.sample_rate.0 as Frequency / 2.0)
    }

    fn magnitude_in(&self, frequencies: Range<Frequency>) -> StereoMagnitude {
        // let v = self.net_magnitude_below(&frequencies.end) - self.net_magnitude_below(&frequencies.start);
        // println!("{},{} -> {}", frequencies.start, frequencies.end, v.norm());
        self.net_magnitude_below(&frequencies.end) - self.net_magnitude_below(&frequencies.start)
    }
}
