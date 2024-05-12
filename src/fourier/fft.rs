use std::cell::RefCell;
use std::iter::zip;
use cpal::SampleRate;
use fftw::array::AlignedVec;
use fftw::plan::{C2CPlan, C2CPlan32};
use fftw::types::Sign;
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
}

impl AudioTransform for FastFourierTransform {
    type Output = InterpolatedFrequencySample;

    fn sample_rate(&self) -> Frequency { self.sample_rate }

    fn process<'a>(&mut self, samples: impl IntoIterator<Item=&'a StereoMagnitude>) -> Option<Self::Output> {
        let window_size: usize = (self.period * self.sample_rate) as usize;
        let padded_window_size = window_size * 2;
        let num_frequencies = 1 + (padded_window_size / 2);

        // Perform preprocessing
        let mut samples_processed: usize = 0;
        let frame = samples.into_iter()
            // Consume at most one frame of input from the iterator
            .take(window_size)
            // Make keep track of the number of samples we actually receive
            .map(|s| {
                samples_processed += 1;
                s
            })
            // Apply a Hann window function
            .enumerate()
            .map(|(i, v)| {
                let scale = 0.5 * (1.0 - ((f32::TAU() * i as f32) / (window_size as f32)).cos());
                v * scale
            })
            // Pad to increase the output resolution
            .pad_using(padded_window_size, |_| StereoMagnitude::zero());

        // Write the processed data to the input buffer
        let mut sample_buffer = AlignedVec::new(padded_window_size);
        for (src, dest) in zip(frame, sample_buffer.iter_mut()) { *dest = src; };

        // If we didn't process enough samples, we shouldn't perform the FFT
        if samples_processed < window_size { return None; }

        // Perform the FFT
        // for a complex FFT, the output is the same size as the input
        let mut frequency_buffer = AlignedVec::new(padded_window_size);
        self.plan.c2c(&mut sample_buffer, &mut frequency_buffer).unwrap();

        // Convert to stereo using the equation given in:
        // https://web.archive.org/web/20180312110051/http://www.engineeringproductivitytools.com/stuff/T0001/PT10.HTM
        let real_frequencies = frequency_buffer.iter().skip(1).take(num_frequencies);
        let imaginary_frequencies = frequency_buffer.iter().rev().take(num_frequencies);
        let frequencies = zip(real_frequencies, imaginary_frequencies)
            .map(|(a, b)| StereoMagnitude::new(
                // note: we can simplify the equations because we're only finding magnitude!
                (a + b.conj()).norm() / 2.0,
                (a - b.conj()).norm() / 2.0,
            ));

        // Apply postprocessing
        let scale = 2.0 / window_size as f32;
        let frequencies = frequencies
            .map(|m| m * scale);

        // Produce a frequency-domain sample from the output of the FFT
        Some(InterpolatedFrequencySample::new(
            frequencies,
            SampleRate { 0: self.sample_rate as u32 },
        ))
    }
}