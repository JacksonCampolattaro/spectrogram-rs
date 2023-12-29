use fftw::plan::{R2CPlan, R2CPlan32};
use fftw::array::AlignedVec;
use fftw::types::c32;

use cpal::SampleRate;
use async_channel::{Sender};

const EPSILON: f32 = 1e-7;
const FFT_WINDOW_SIZE: usize = 4096;
const FFT_WINDOW_STRIDE: usize = 128;
const NUM_FREQUENCIES: usize = 1 + (FFT_WINDOW_SIZE / 2);

pub struct FrequencySample {
    pub magnitudes: Vec<f32>,
    pub sample_rate: SampleRate,
}

impl FrequencySample {
    pub fn period(&self) -> f32 {
        self.magnitudes.len() as f32 / self.sample_rate.0 as f32
    }
    pub fn magnitude_of_frequency(&self, frequency: f32) -> f32 {
        let period = self.magnitudes.len() as f32 / self.sample_rate.0 as f32;
        let index = frequency * period;
        let offset = index % 1.0;
        (self.magnitudes[index.ceil() as usize] * offset) + (self.magnitudes[index.floor() as usize] * (1.0 - offset))
    }
    pub fn mean_magnitude_of_frequency_range(&self, start_frequency: f32, end_frequency: f32) -> f32 {
        let period = self.magnitudes.len() as f32 / self.sample_rate.0 as f32;
        let start_index = start_frequency * period;
        let end_index = end_frequency * period;
        let mut sum = 0.0;

        sum = sum + self.magnitudes[start_index.floor() as usize] * (1.0 - start_index % 1.0);
        for i in (start_index.ceil() as usize)..(end_index.floor() as usize) {
            sum = sum + self.magnitudes[i];
        }
        sum = sum + self.magnitudes[end_index.ceil() as usize] * (start_index % 1.0);

        //println!("{}-{} --> {}-{} --> {}", start_frequency, end_frequency, start_index, end_index, sum);
        sum // / (end_frequency - start_frequency)
    }
    pub fn max_frequency(&self) -> f32 {
        self.sample_rate.0 as f32
    }
}

pub struct FourierTransform {
    plan: R2CPlan32,
    sample_buffer: AlignedVec<f32>,
    frequency_buffer: AlignedVec<c32>,
    sender: Sender<FrequencySample>,
}

impl FourierTransform {
    pub fn new(sender: Sender<FrequencySample>) -> Self {
        let mut sample_buffer = AlignedVec::new(FFT_WINDOW_SIZE);
        sample_buffer.fill(0.0);

        let frequency_buffer = AlignedVec::new(NUM_FREQUENCIES);

        let plan = R2CPlan32::aligned(
            &[FFT_WINDOW_SIZE],
            fftw::types::Flag::ESTIMATE,
        ).unwrap();

        FourierTransform {
            plan,
            sample_buffer,
            frequency_buffer,
            sender,
        }
    }

    pub fn apply(&mut self, samples: &[f32], sample_rate: SampleRate) -> FrequencySample {

        // todo: This assumes nice alignment between sample block size & stride
        for chunk in samples.chunks_exact(FFT_WINDOW_STRIDE) {
            self.sample_buffer.rotate_left(FFT_WINDOW_SIZE);
            self.sample_buffer[FFT_WINDOW_SIZE - FFT_WINDOW_STRIDE..].copy_from_slice(chunk);

            self.plan.r2c(&mut self.sample_buffer, &mut self.frequency_buffer).unwrap();

            let scale = 2.0 / FFT_WINDOW_SIZE as f32;
            let frequency_magnitudes: Vec<_> = self.frequency_buffer.iter()
                //.map(|c| c * scale)
                .map(|c| c.norm_sqr())
                // .map(|v: f32| 10.0 * (v + EPSILON).log10())
                .collect();

            let frequncy_sample = FrequencySample {
                magnitudes: frequency_magnitudes,
                sample_rate,
            };

            // todo: it might be best to avoid send_blocking if possible
            self.sender.send_blocking(frequncy_sample).expect("Failed to send data");
        }

        FrequencySample {
            magnitudes: Vec::from(samples),
            sample_rate,
        }
    }
}