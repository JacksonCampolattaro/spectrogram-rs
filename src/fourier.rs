use std::iter::zip;
use fftw::plan::{R2CPlan, R2CPlan32};
use fftw::array::AlignedVec;
use fftw::types::c32;

use cpal::SampleRate;
use async_channel::{Sender};
use iter_num_tools::lin_space;
use itertools::Itertools;
use ndarray::Axis;
use num_traits::{Bounded, FloatConst};

const FFT_WINDOW_SIZE: usize = 2048;
const PADDED_FFT_WINDOW_SIZE: usize = FFT_WINDOW_SIZE * 2;
const FFT_WINDOW_STRIDE: usize = 128;
const NUM_FREQUENCIES: usize = 1 + (PADDED_FFT_WINDOW_SIZE / 2);

pub struct FrequencySample {
    pub magnitudes: Vec<f32>,
    pub sample_rate: SampleRate,
}

impl FrequencySample {
    pub fn period(&self) -> f32 {
        2.0 * self.magnitudes.len() as f32 / self.sample_rate.0 as f32
    }

    pub fn index_of_frequency(&self, frequency: f32) -> f32 {
        frequency * self.period()
    }

    pub fn frequency_of_index(&self, index: f32) -> f32 {
        index / self.period()
    }

    pub fn magnitude_of_frequency(&self, frequency: f32) -> f32 {
        let index = frequency * self.period();
        // Interpolation is done in frequency space, to account for log scaling
        let (floor, ceil) = (self.next_lower_frequency(frequency), self.next_higher_frequency(frequency));
        let offset = (frequency - floor) / (ceil - floor);
        // cosine interpolation for a smoother-looking plot
        let offset = (1.0 - f32::cos(offset * f32::PI())) / 2.0;
        (self.magnitudes[index.floor() as usize] * (1.0 - offset)) + (self.magnitudes[index.ceil() as usize] * offset)
    }

    fn next_lower_frequency(&self, frequency: f32) -> f32 {
        (frequency * self.period()).floor() / self.period()
    }

    fn next_higher_frequency(&self, frequency: f32) -> f32 {
        (frequency * self.period()).ceil() / self.period()
    }

    pub fn mean_magnitude_of_frequency_range(&self, start: f32, end: f32) -> f32 {
        assert!(start < end);
        let (start_index, end_index) = (self.index_of_frequency(start), self.index_of_frequency(end));
        let num_samples = ((end_index - start_index).floor() as usize).max(1);
        let sample_frequencies = lin_space(start..end, num_samples);
        let mean = sample_frequencies
            .map(|f| { self.magnitude_of_frequency(f) })
            .sum::<f32>() / num_samples as f32;
        // Note: this ensures higher frequencies are well represented, but it might not be energetically accurate
        mean * (end - start)
    }
    pub fn max_frequency(&self) -> f32 {
        (self.sample_rate.0 / 2) as f32
    }
}

pub struct FourierTransform {
    plan: R2CPlan32,
    sample_buffers: Vec<Vec<f32>>,
    sender: Sender<FrequencySample>,
}

impl FourierTransform {
    pub fn new(sender: Sender<FrequencySample>, channels: usize) -> Self {
        let mut sample_buffers = (0..channels)
            .map(|_| { vec![0.0; FFT_WINDOW_SIZE] })
            .collect();

        let plan = R2CPlan32::aligned(
            &[PADDED_FFT_WINDOW_SIZE],
            fftw::types::Flag::ESTIMATE,
        ).unwrap();

        FourierTransform {
            plan,
            sample_buffers,
            sender,
        }
    }

    pub fn apply(&mut self, samples: &ndarray::Array2<f32>, sample_rate: SampleRate) {
        // todo: I should use a channel/deque to accumulate samples before processing!
        let channels = samples.shape()[1];
        for chunk in samples.exact_chunks((FFT_WINDOW_STRIDE, channels)) {
            // Push a chunk of new data to each channel's buffer
            for (stream, buffer) in zip(chunk.axis_iter(Axis(1)), self.sample_buffers.iter_mut()) {
                buffer.rotate_left(FFT_WINDOW_STRIDE);
                buffer[FFT_WINDOW_SIZE - FFT_WINDOW_STRIDE..].copy_from_slice(stream.as_standard_layout().as_slice().unwrap());
            }

            // Apply the fft on the buffer
            // Writing this as a pipeline will make it easy to parallelize in the future
            let magnitudes: Vec<Vec<f32>> = self.sample_buffers.iter()
                // Scale the window data
                .map(|buffer| {
                    buffer.iter()
                        .enumerate()
                        .map(|(i, v)| {
                            // This function applies a Hann window
                            let scale = 0.5 * (1.0 - ((f32::TAU() * i as f32) / (FFT_WINDOW_SIZE as f32)).cos());
                            v * scale
                        })
                })
                // Next, write the data to a padded window buffer
                .map(|scaled_iter| {
                    let mut padded_buffer = AlignedVec::new(PADDED_FFT_WINDOW_SIZE);
                    padded_buffer.fill(0.0);
                    for (src, dest) in zip(scaled_iter, padded_buffer.iter_mut()) {
                        *dest = src;
                    };
                    padded_buffer
                })
                // Apply the fft to produce a frequency buffer
                .map(|mut window_buffer| {
                    let mut frequency_buffer = AlignedVec::new(NUM_FREQUENCIES);
                    self.plan.r2c(&mut window_buffer, &mut frequency_buffer).unwrap();
                    frequency_buffer
                })
                // Convert frequencies from complex to real magnitudes
                .map(|frequency_buffer| {
                    let scale = 2.0 / FFT_WINDOW_SIZE as f32;
                    frequency_buffer.iter()
                        .map(|c| c.norm_sqr())
                        .map(|c| c * scale)
                        // temporary measure, because we're only showing one channel for now
                        .map(|c| c * channels as f32)
                        .collect()
                })
                .collect();

            // todo: send more than one channel to visualization
            let frequency_sample = FrequencySample {
                magnitudes: magnitudes[0].clone(),
                sample_rate,
            };
            self.sender.send_blocking(frequency_sample).expect("Failed to send data");
        }
    }
}