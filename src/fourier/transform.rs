use std::iter::zip;
use async_channel::Sender;
use cpal::SampleRate;
use fftw::array::AlignedVec;
use fftw::types::{c32, Sign};
use fftw::plan::{C2CPlan, C2CPlan32};
use itertools::Itertools;
use num_traits::{FloatConst, Zero};
use crate::fourier::{FFT_WINDOW_SIZE, FFT_WINDOW_STRIDE, NUM_FREQUENCIES, PADDED_FFT_WINDOW_SIZE};
use crate::fourier::frequency_sample::{StereoFrequencySample, StereoMagnitude};


pub struct ComplexStereoTransform {
    plan: C2CPlan32,
    buffer: Vec<c32>,
    sender: Sender<StereoFrequencySample>,
}

impl ComplexStereoTransform {
    pub fn new(sender: Sender<StereoFrequencySample>) -> Self {
        let plan = C2CPlan32::aligned(
            &[PADDED_FFT_WINDOW_SIZE],
            Sign::Forward,
            fftw::types::Flag::MEASURE,
        ).unwrap();

        let buffer = vec![c32::default(); FFT_WINDOW_SIZE];

        ComplexStereoTransform {
            plan,
            buffer,
            sender,
        }
    }

    pub fn apply(&mut self, samples: &[c32], sample_rate: SampleRate) {

        // Process the input stream in chunks
        for chunk in samples.chunks_exact(FFT_WINDOW_STRIDE) {
            let start_time = std::time::Instant::now();

            // Fill the buffer with new data
            self.buffer.rotate_left(FFT_WINDOW_STRIDE);
            self.buffer[FFT_WINDOW_SIZE - FFT_WINDOW_STRIDE..].copy_from_slice(chunk);

            // Apply windowing and padding
            let hann_windowed = self.buffer.iter()
                .enumerate()
                .map(|(i, v)| {
                    // This function applies a Hann window
                    let scale = 0.5 * (1.0 - ((f32::TAU() * i as f32) / (FFT_WINDOW_SIZE as f32)).cos());
                    v * scale
                });
            let padded = hann_windowed.pad_using(PADDED_FFT_WINDOW_SIZE, |_| c32::zero());

            // Write the processed data to the input buffer
            let mut padded_buffer = AlignedVec::new(PADDED_FFT_WINDOW_SIZE);
            for (src, dest) in zip(padded, padded_buffer.iter_mut()) { *dest = src; };

            // Perform the FFT
            // for a complex FFT, the output is the same size as the input
            let mut frequency_buffer = AlignedVec::new(PADDED_FFT_WINDOW_SIZE);
            self.plan.c2c(&mut padded_buffer, &mut frequency_buffer).unwrap();

            // Convert to stereo using the equation given in:
            // https://web.archive.org/web/20180312110051/http://www.engineeringproductivitytools.com/stuff/T0001/PT10.HTM
            let real_frequencies = frequency_buffer.iter().skip(1).take(NUM_FREQUENCIES);
            let imaginary_frequencies = frequency_buffer.iter().rev().take(NUM_FREQUENCIES);
            let frequencies = zip(real_frequencies, imaginary_frequencies)
                .map(|(a, b)| StereoMagnitude::new(
                    // note: we can simplify the equations because we're only finding magnitude!
                    (a + b.conj()).norm() / 2.0,
                    (a - b.conj()).norm() / 2.0,
                ));

            // Apply postprocessing
            let scale = 2.0 / FFT_WINDOW_SIZE as f32;
            let frequencies = frequencies
                .map(|m| m * scale);

            // Send the sample to be displayed
            let frequency_sample = StereoFrequencySample::new(frequencies, sample_rate);
            println!("FFT time: {:.2?}", start_time.elapsed());
            // We don't care whether the sample actually goes through, so no .expect() here.
            // (dropping data is best, since we want to keep latency low)
            self.sender.try_send(frequency_sample).ok();
        }

    }
}


// todo: In the future, this could be replaced with an filter on an iterator with no unsafe code
pub fn deinterleave_stereo(buffer: &[f32]) -> &[StereoMagnitude] {
    // Adapted from:
    // https://stackoverflow.com/questions/54185667/how-to-safely-reinterpret-vecf64-as-vecnum-complexcomplexf64-with-half-t
    // This is safe because:
    //   - StereoMagnitude is an fftw Conplex<f32>, which has the layout [f32; 2]
    //   - buffer is a slice, guaranteed to have contiguous memory layout
    //   - The slice is passed as a reference, so the ownership of the underlying memory is left unchanged
    unsafe {
        let ptr = buffer.as_ptr() as *const StereoMagnitude;
        let len = buffer.len();

        assert_eq!(len % 2, 0);

        std::slice::from_raw_parts(ptr, len / 2)
    }
}

pub fn mono_to_stereo(buffer: &[f32]) -> Vec<StereoMagnitude> {
    buffer.into_iter().map(|v| StereoMagnitude::new(*v, *v)).collect()
}
