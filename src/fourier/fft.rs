use std::cell::RefCell;
use std::iter::zip;
use cpal::SampleRate;
use fftw::array::AlignedVec;
use fftw::plan::{C2CPlan, C2CPlan32};
use fftw::types::{c32, Sign};
use itertools::Itertools;
use num_traits::{FloatConst, Zero};

use crate::fourier::audio_transform::AudioTransform;
use crate::fourier::interpolated_frequency_sample::InterpolatedFrequencySample;
use crate::fourier::{Period, Frequency, StereoMagnitude, PADDED_FFT_WINDOW_SIZE};

pub struct FastFourierTransform {
    plan: C2CPlan32,
    sample_rate: Frequency,
    period: Period,
}

impl FastFourierTransform {
    pub fn new(sample_rate: Frequency, period: Period) -> Self {
        let window_size: usize = (period * sample_rate) as usize;
        let plan = C2CPlan32::aligned(
            &[window_size * 2],
            Sign::Forward,
            fftw::types::Flag::MEASURE,
        ).unwrap();

        Self {
            plan,
            sample_rate,
            period,
        }
    }

    pub fn num_output_frequencies(&self) -> usize { self.num_input_samples() - 1 }
}

impl AudioTransform for FastFourierTransform {
    type Output = Vec<StereoMagnitude>;

    fn sample_rate(&self) -> Frequency { self.sample_rate }

    fn num_input_samples(&self) -> usize { (self.period * self.sample_rate) as usize }

    fn process<'a>(&mut self, samples: impl IntoIterator<Item=&'a StereoMagnitude>) -> Option<Self::Output> {
        let padded_window_size = self.num_input_samples() * 2;

        // Perform preprocessing
        let mut samples_processed: usize = 0;
        let frame = samples.into_iter()
            // Consume at most one frame of input from the iterator
            .take(self.num_input_samples())
            // Make keep track of the number of samples we actually receive
            .map(|s| {
                samples_processed += 1;
                s
            })
            // Convert to complex numbers
            .map(|(l, r)| c32::new(*l, *r))
            // Apply a Hann window function
            .enumerate()
            .map(|(i, v)| {
                let scale = 0.5 * (1.0 - ((f32::TAU() * i as f32) / (self.num_input_samples() as f32)).cos());
                v * scale
            })
            // Pad to increase the output resolution
            .pad_using(padded_window_size, |_| c32::zero());

        // Write the processed data to the input buffer
        let mut sample_buffer = AlignedVec::new(padded_window_size);
        for (src, dest) in zip(frame, sample_buffer.iter_mut()) { *dest = src; };

        // If we didn't process enough samples, we shouldn't perform the FFT
        if samples_processed < self.num_input_samples() { return None; }

        // Perform the FFT
        // for a complex FFT, the output is the same size as the input
        let mut frequency_buffer = AlignedVec::new(padded_window_size);
        self.plan.c2c(&mut sample_buffer, &mut frequency_buffer).unwrap();

        // Convert to stereo using the equation given in:
        // https://web.archive.org/web/20180312110051/http://www.engineeringproductivitytools.com/stuff/T0001/PT10.HTM
        let real_frequencies = frequency_buffer.iter().skip(1).take(self.num_output_frequencies());
        let imaginary_frequencies = frequency_buffer.iter().rev().take(self.num_output_frequencies());
        let magnitudes = zip(real_frequencies, imaginary_frequencies)
            .map(|(a, b)| c32::new(
                // note: we can simplify the equations because we're only finding magnitude!
                // eventually, it may make sense to keep track of phase, too
                (a + b.conj()).norm() / 2.0,
                (a - b.conj()).norm() / 2.0,
            ));

        // Apply postprocessing
        let scale = 2.0 / self.num_input_samples() as f32;
        Some(
            magnitudes
                .map(|m| m * scale)
                .map(|m| (m.re, m.im))
                .collect()
        )
    }
}
