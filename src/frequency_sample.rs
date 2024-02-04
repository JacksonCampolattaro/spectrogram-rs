use std::array;
use std::iter::zip;
use std::ops::Range;
use cpal::SampleRate;
use iter_num_tools::lin_space;
use num_traits::FloatConst;

pub type StereoMagnitude = [f32; 2];
pub type Period = f32;
pub type Frequency = f32;

const MIN_DB: f32 = -70.0;
const MAX_DB: f32 = 32.0;

pub fn to_scaled_decibels(magnitude: &StereoMagnitude) -> StereoMagnitude {
    // todo: in the future, this should be split into to_decibels & a mapping to a display scale
    magnitude
        .map(|m| 20.0 * (m + 1e-7).log10())
        .map(|m| ((m - MIN_DB) / (MAX_DB - MIN_DB)))
}

pub trait FrequencySample {
    fn period(&self) -> Period;
    fn frequencies(&self) -> Range<Frequency>;
    fn magnitude_at(&self, frequency: &Frequency) -> StereoMagnitude;
    fn magnitude_in(&self, frequencies: Range<Frequency>) -> StereoMagnitude;
    // todo: this may be useful if we want precise timing information in the future
    //fn instant(&self) -> StreamInstant;
}


pub struct StereoFrequencySample {
    pub magnitudes: Vec<StereoMagnitude>,
    pub sample_rate: SampleRate,
}

impl StereoFrequencySample {
    pub fn from_channels(left: Vec<f32>, right: Vec<f32>, sample_rate: SampleRate) -> Self {
        let magnitudes = zip(left.iter(), right.iter())
            .map(|(l, r)| StereoMagnitude::from([*l, *r]))
            .collect();
        StereoFrequencySample {
            magnitudes,
            sample_rate,
        }
    }

    pub fn from_mono(magnitudes: Vec<f32>, sample_rate: SampleRate) -> Self {
        let magnitudes = magnitudes.iter()
            .map(|m| StereoMagnitude::from([*m, *m]))
            .collect();
        StereoFrequencySample {
            magnitudes,
            sample_rate,
        }
    }

    fn index_of(&self, frequency: &Frequency) -> f32 {
        let index = frequency * self.period();
        assert!(0.0 < index && index < (self.magnitudes.len() - 1) as f32);
        index as f32
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
}

impl FrequencySample for StereoFrequencySample {
    fn period(&self) -> Period {
        2.0 * self.magnitudes.len() as f32 / self.sample_rate.0 as f32
    }

    fn frequencies(&self) -> Range<Frequency> {
        0.0..(self.sample_rate.0 as Frequency / 2.0)
    }

    fn magnitude_at(&self, frequency: &Frequency) -> StereoMagnitude {
        let cell_indices = self.cell_indices_of(frequency);

        // Interpolation is done in frequency space, to account for log scaling
        let cell_frequencies = self.frequencies_of(&cell_indices);
        let offset = ((*frequency - cell_frequencies.start) / (cell_frequencies.end - cell_frequencies.start)) as f32;

        // cosine interpolation for a smoother-looking plot
        let offset = (1.0 - f32::cos(offset * f32::PI())) / 2.0;

        let low = self.magnitudes[cell_indices.start];
        let high = self.magnitudes[cell_indices.end];
        array::from_fn(|channel| {
            (low[channel] * (1.0 - offset)) + (high[channel] * offset)
        })
    }

    fn magnitude_in(&self, frequencies: Range<Frequency>) -> StereoMagnitude {

        // Determine the number of samples to take
        let indices = self.index_of(&frequencies.start)..self.index_of(&frequencies.end);
        let num_samples = ((indices.end - indices.start).floor() as usize).max(1);

        // Select some representative samples
        let sample_frequencies = lin_space(frequencies.clone(), num_samples);
        let sample_magnitudes = sample_frequencies
            .map(|f| self.magnitude_at(&f));

        // todo: this could use some cleaning up, not a fan of the clone()
        let mean_magnitude: StereoMagnitude = array::from_fn(|channel| {
            sample_magnitudes.clone()
                .map(|m| m[channel])
                .sum::<f32>() / num_samples as f32
        });
        mean_magnitude
    }
}
