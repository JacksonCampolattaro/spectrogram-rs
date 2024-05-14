use std::cell::RefCell;
use std::iter::zip;
use std::sync::{Arc, Mutex};
use async_channel::{Receiver, Sender};
use cpal::{SampleRate, StreamConfig};
use fftw::array::AlignedVec;
use fftw::types::{c32, Sign};
use fftw::plan::{C2CPlan, C2CPlan32};
use itertools::Itertools;
use num_traits::{FloatConst, Zero};
use ringbuf::{HeapCons, HeapRb, traits::Consumer};
use crate::fourier::{FFT_WINDOW_SIZE, FFT_WINDOW_STRIDE, NUM_FREQUENCIES, PADDED_FFT_WINDOW_SIZE, StereoMagnitude};
use crate::fourier::interpolated_frequency_sample::{InterpolatedFrequencySample};

use biquad::*;
use std::ops::Range;
use ringbuf::traits::Observer;

pub struct FilteredStreamTransform {
    receiver: RefCell<HeapCons<StereoMagnitude>>,
    stream_config: Arc<Mutex<Option<StreamConfig>>>,
    plan: RefCell<C2CPlan32>,
    sender: Sender<InterpolatedFrequencySample>,
}

impl FilteredStreamTransform {
    pub fn new(
        sample_stream: HeapCons<StereoMagnitude>,
        config: Arc<Mutex<Option<StreamConfig>>>,
        frequency_range: Range<Hertz<f32>>,
    ) -> (Self, Receiver<InterpolatedFrequencySample>) {
        let (frequency_sender, frequency_receiver) = async_channel::unbounded();
        let plan = C2CPlan32::aligned(
            &[PADDED_FFT_WINDOW_SIZE],
            Sign::Forward,
            fftw::types::Flag::MEASURE,
        ).unwrap();

        (
            Self {
                receiver: sample_stream.into(),
                stream_config: config,
                plan: plan.into(),
                sender: frequency_sender,
            },
            frequency_receiver
        )
    }

    pub fn process(&self) {
        let start_time = std::time::Instant::now();
        let mut sample_stream = self.receiver.borrow_mut();
        let mut fft_plan = self.plan.borrow_mut();

        while sample_stream.occupied_len() >= FFT_WINDOW_SIZE {

            // The next window-length samples provide our input buffer
            let frame = sample_stream.iter().take(FFT_WINDOW_SIZE);

            // Apply windowing and padding
            let frame = frame
                .enumerate()
                .map(|(i, v)| {
                    // This function applies a Hann window
                    let scale = 0.5 * (1.0 - ((f32::TAU() * i as f32) / (FFT_WINDOW_SIZE as f32)).cos());
                    v * scale
                });
            let frame = frame.pad_using(PADDED_FFT_WINDOW_SIZE, |_| c32::zero());

            // Write the processed data to the input buffer
            let mut sample_buffer = AlignedVec::new(PADDED_FFT_WINDOW_SIZE);
            for (src, dest) in zip(frame, sample_buffer.iter_mut()) { *dest = src; };

            // Perform the FFT
            // for a complex FFT, the output is the same size as the input
            let mut frequency_buffer = AlignedVec::new(PADDED_FFT_WINDOW_SIZE);
            fft_plan.c2c(&mut sample_buffer, &mut frequency_buffer).unwrap();

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

            // Send the results to the output channel in the form of a frequency sample
            // fixme: how should the actual current sample rate be communicated to the FFT?
            let stream_config = self.stream_config.lock().unwrap();
            let frequency_sample = InterpolatedFrequencySample::new(
                frequencies,
                stream_config.as_ref().unwrap().sample_rate,
            );
            self.sender.try_send(frequency_sample).ok();

            // Drop the oldest elements in the stream (shifting our window)
            sample_stream.skip(FFT_WINDOW_STRIDE);
        }
        println!("Performed FFT processing in {:.2?}", start_time.elapsed())
    }
}
