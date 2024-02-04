use std::ops::Range;
use fftw::types::c32;

pub mod transform;
pub mod interpolated_frequency_sample;
mod fast_frequency_sample;

const FFT_WINDOW_SIZE: usize = 2048;
const PADDED_FFT_WINDOW_SIZE: usize = FFT_WINDOW_SIZE * 2;
const FFT_WINDOW_STRIDE: usize = 128;
const NUM_FREQUENCIES: usize = 1 + (PADDED_FFT_WINDOW_SIZE / 2);


pub type StereoMagnitude = c32;
pub type Period = f32;
pub type Frequency = f32;

pub trait FrequencySample {
    fn period(&self) -> Period;
    fn frequencies(&self) -> Range<Frequency>;
    fn magnitude_in(&self, frequencies: Range<Frequency>) -> StereoMagnitude;
    // todo: This is not actually so useful, magnitude_in() is usually appropriate
    //fn magnitude_at(&self, frequency: &Frequency) -> StereoMagnitude;
    // todo: this may be useful if we want precise timing information in the future
    //fn instant(&self) -> StreamInstant;
}
