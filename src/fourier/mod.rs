pub mod frequency_sample;
pub mod transform;

const FFT_WINDOW_SIZE: usize = 2048;
const PADDED_FFT_WINDOW_SIZE: usize = FFT_WINDOW_SIZE * 2;
const FFT_WINDOW_STRIDE: usize = 128;
const NUM_FREQUENCIES: usize = 1 + (PADDED_FFT_WINDOW_SIZE / 2);