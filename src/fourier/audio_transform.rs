use crate::fourier::{Frequency, Period, StereoMagnitude};
use ringbuf::HeapConsumer;

pub trait AudioTransform {
    type Output;

    fn sample_rate(&self) -> Frequency;

    fn process<'a>(&mut self, samples: impl IntoIterator<Item=&'a StereoMagnitude>) -> Option<Self::Output>;
}


pub struct AudioStreamTransform<T: AudioTransform> {
    pub input_stream: HeapConsumer<StereoMagnitude>,
    pub transform: T,
    // todo: interior mutability may be necessary here!
    pub stride: Period,
}

impl<T: AudioTransform> AudioStreamTransform<T> {
    pub fn new(
        input_stream: HeapConsumer<StereoMagnitude>,
        transform: T,
        stride: Period,
    ) -> Self {
        Self {
            input_stream,
            transform,
            stride,
        }
    }

    pub fn process(&mut self) -> impl Iterator<Item=<T as AudioTransform>::Output> + '_ {
        let stride_samples = (self.stride * self.transform.sample_rate()) as usize;
        // println!("Processing {} samples with stride {}", self.input_stream.len(), stride_samples);
        std::iter::repeat_with(move || {
            let out = self.transform.process(&mut self.input_stream.iter());
            self.input_stream.skip(stride_samples);
            out
        }).take_while(|v| v.is_some()).flatten()
    }
}