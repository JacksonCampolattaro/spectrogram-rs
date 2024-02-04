use std::iter::zip;
use fftw::plan::{R2CPlan, R2CPlan32};
use fftw::array::AlignedVec;

use cpal::SampleRate;
use async_channel::Sender;
use ndarray::Axis;
use num_traits::FloatConst;
use crate::frequency_sample::StereoFrequencySample;

const FFT_WINDOW_SIZE: usize = 2048;
const PADDED_FFT_WINDOW_SIZE: usize = FFT_WINDOW_SIZE * 2;
const FFT_WINDOW_STRIDE: usize = 128;
const NUM_FREQUENCIES: usize = 1 + (PADDED_FFT_WINDOW_SIZE / 2);

pub struct FourierTransform {
    plan: R2CPlan32,
    sample_buffers: Vec<Vec<f32>>,
    sender: Sender<StereoFrequencySample>,
}

impl FourierTransform {
    pub fn new(sender: Sender<StereoFrequencySample>, channels: usize) -> Self {
        let sample_buffers = (0..channels)
            .map(|_| { vec![0.0; FFT_WINDOW_SIZE] })
            .collect();

        let plan = R2CPlan32::aligned(
            &[PADDED_FFT_WINDOW_SIZE],
            fftw::types::Flag::MEASURE,
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

            // todo: Maybe I should use something like this for stereo signals:
            // https://web.archive.org/web/20180312110051/http://www.engineeringproductivitytools.com/stuff/T0001/PT10.HTM

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

            // let frequency_sample = StereoFrequencySample {
            //     magnitudes: magnitudes[0].clone(),
            //     sample_rate,
            // };
            let frequency_sample = if magnitudes.len() == 1 {
                StereoFrequencySample::from_mono(magnitudes[0].clone(), sample_rate)
            } else {
                StereoFrequencySample::from_channels(
                    magnitudes[0].clone(), magnitudes[1].clone(),
                    sample_rate,
                )
            };

            // We don't care whether the sample actually goes through, so no .expect() here.
            self.sender.try_send(frequency_sample).ok();
        }
    }
}