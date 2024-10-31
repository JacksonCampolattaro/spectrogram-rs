use std::ops::Range;
use cpal::SampleRate;
use fftw::types::c32;
use iter_num_tools::lin_space;
use num_traits::{FloatConst, pow};

use crate::fourier::{Frequency, FrequencySample, Period, StereoMagnitude};


pub struct InterpolatedFrequencySample {
    pub magnitudes: Vec<c32>,
    pub sample_rate: SampleRate,
}

impl InterpolatedFrequencySample {
    pub fn new<I>(magnitudes: I, sample_rate: SampleRate) -> Self
        where I: IntoIterator<Item=StereoMagnitude> {
        InterpolatedFrequencySample {
            magnitudes: magnitudes.into_iter().map(|(l, r)| c32::new(l, r)).collect(),
            sample_rate,
        }
    }

    fn index_of(&self, frequency: &Frequency) -> f32 {
        let index = frequency * self.period();
        // assert!(0.0 < index && index < (self.magnitudes.len() - 1) as f32);
        // if !(0.0 < index && index < (self.magnitudes.len() - 1) as f32) {
        //     return 0.0;
        // }
        index.clamp(0.0, (self.magnitudes.len() - 1) as f32)
    }

    fn frequency_of(&self, index: f32) -> Frequency {
        index / self.period()
    }

    fn cell_indices_of(&self, frequency: &Frequency) -> Range<usize> {
        let index = self.index_of(frequency);
        (index.floor() as usize)..(index.ceil() as usize)
    }

    fn frequencies_of(&self, indices: &Range<usize>) -> Range<Frequency> {
        self.frequency_of(indices.start as f32)..self.frequency_of(indices.end as f32)
    }

    fn magnitude_at(&self, frequency: &Frequency) -> StereoMagnitude {
        cubic_interpolate(self.magnitudes.as_slice(), self.index_of(&frequency))
    }
}

impl FrequencySample for InterpolatedFrequencySample {
    fn period(&self) -> Period {
        2.0 * self.magnitudes.len() as f32 / self.sample_rate.0 as f32
    }

    fn frequencies(&self) -> Range<Frequency> {
        0.0..(self.sample_rate.0 as Frequency / 2.0)
    }

    fn magnitude_in(&self, frequencies: Range<Frequency>) -> StereoMagnitude {

        // Determine the number of samples to take
        let indices = self.index_of(&frequencies.start)..self.index_of(&frequencies.end);
        let num_samples = ((indices.end - indices.start).floor() as usize).max(1);

        // Select some representative samples
        let sample_frequencies = lin_space(frequencies.clone(), num_samples);
        let sample_magnitudes = sample_frequencies
            .map(|f| self.magnitude_at(&f));

        let mean_magnitude = sample_magnitudes
            .map(|(l, r)| c32 { re: l, im: r })
            .sum::<c32>() / num_samples as f32;
        (mean_magnitude.re, mean_magnitude.im)
    }
}

#[allow(dead_code)]
fn cosine_interpolate(data: &[c32], index: f32) -> StereoMagnitude {
    let low = index.floor() as usize;
    let high = (index.ceil() as usize).clamp(low + 1, data.len() - 1);
    let offset = index - low as f32;
    let offset = (1.0 - f32::cos(offset * f32::PI())) / 2.0;
    let interpolated = (data[low] * (1.0 - offset)) + (data[high] * offset);
    (interpolated.re, interpolated.im)
}

#[allow(dead_code)]
fn cubic_interpolate(data: &[c32], index: f32) -> StereoMagnitude {
    // Adapted from: https://paulbourke.net/miscellaneous/interpolation/
    let mu = index - index.floor();
    let x0 = (index.floor() as usize - 1).max(0);
    let x1 = index.floor() as usize;
    let x2 = (x1 + 1).min(data.len() - 1);
    let x3 = (x1 + 2).min(data.len() - 1);
    let (y0, y1, y2, y3) = (data[x0], data[x1], data[x2], data[x3]);

    let a0 = y3 - y2 - y0 + y1;
    let a1 = y0 - y1 - a0;
    let a2 = y2 - y0;
    let a3 = y1;

    let interpolated = (a0 * pow(mu, 3)) + (a1 * pow(mu, 2)) + (a2 * mu + a3);
    (interpolated.re, interpolated.im)
}
