use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use ringbuf::{HeapConsumer, HeapRb};
use async_channel::Receiver;
use plotters::coord::types::RangedCoordf32;
use std::cell::Cell;

use adw::{glib, glib::{Properties, Object}, gdk::gdk_pixbuf::{Pixbuf, Colorspace}, prelude::ObjectExt, subclass::prelude::{ObjectImpl, WidgetImpl, ObjectSubclass, DerivedObjectProperties}, gdk};
use adw;
use adw::gdk::Texture;
use adw::glib::ControlFlow::Continue;
use adw::prelude::BinExt;
use adw::subclass::prelude::ObjectSubclassExt;
use cpal::StreamConfig;
use gtk::{ContentFit, Picture};
use gtk::prelude::{WidgetExt, WidgetExtManual};

use crate::{
    colorscheme::ColorScheme,
    log_scaling::{LogCoordf64, IntoReversibleLogRange},
    fourier::Frequency,
    fourier::fft::FastFourierTransform,
    fourier::audio_transform::AudioStreamTransform,
};
use crate::fourier::StereoMagnitude;

const TEXTURE_WIDTH: i32 = 1024;
const TEXTURE_HEIGHT: i32 = 1024;

glib::wrapper! {
    pub struct SimpleSpectrogram(ObjectSubclass<imp::SimpleSpectrogram>)
        @extends gtk::Widget;
}

impl SimpleSpectrogram {
    pub fn new(sample_stream: HeapConsumer<StereoMagnitude>) -> SimpleSpectrogram {
        let object = Object::builder().build();
        let imp = imp::SimpleSpectrogram::from_obj(&object);
        imp.fft.borrow_mut().input_stream = sample_stream;
        object.add_tick_callback(|spectrogram, _| {
            // todo: only draw if there are unprocessed samples!
            spectrogram.queue_draw();
            Continue
        });
        object
    }
}

mod imp {
    use adw::gdk::RGBA;
    use gtk::graphene::Rect;
    use gtk::gsk::ScalingFilter;
    use gtk::prelude::SnapshotExt;
    use plotters::coord::ReverseCoordTranslate;
    use plotters::prelude::Cartesian2d;
    use crate::fourier::{FrequencySample, Period};
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::SimpleSpectrogram)]
    pub struct SimpleSpectrogram {

        // Appearance settings
        pub x_range: RangedCoordf32,
        pub y_range: LogCoordf64,
        #[property(get, set)]
        pub palette: RefCell<ColorScheme>,

        // Plot buffer
        pub buffer: Pixbuf,
        offset: Cell<usize>,

        // FFT parameters
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        // todo: period, stride, etc.
        pub fft: RefCell<AudioStreamTransform<FastFourierTransform>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SimpleSpectrogram {
        const NAME: &'static str = "SimpleSpectrogram";
        type Type = super::SimpleSpectrogram;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            let buffer = Pixbuf::new(
                Colorspace::Rgb,
                true,
                8,
                TEXTURE_WIDTH, TEXTURE_HEIGHT,
            );
            let palette = ColorScheme::new_mono(colorous::MAGMA, "magma");

            let (_, dummy_sample_stream) = HeapRb::new(1).split();

            let fft = AudioStreamTransform::new(
                dummy_sample_stream,
                FastFourierTransform::new(100 as Frequency, 1 as Period),
                2.0 / TEXTURE_WIDTH as f32, // todo: this should be defined as an elapsed time!
            );

            Self {
                x_range: (-10.0..0.0).into(),
                y_range: (32.0..22030.0).reversible_log_scale().base(2.0).zero_point(0.0).into(),
                palette: palette.into(),
                buffer: buffer.unwrap(),
                offset: 0.into(),
                fft: fft.into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SimpleSpectrogram {}

    impl WidgetImpl for SimpleSpectrogram {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let width = self.obj().width() as f32;
            let height = self.obj().height() as f32;
            if width == 0.0 || height == 0.0 { return; }

            // Render the frequency stream to the buffer
            let start_time = std::time::Instant::now();
            let buffer = &self.buffer;

            let cartesian_range: Cartesian2d<RangedCoordf32, LogCoordf64> = Cartesian2d::new(
                self.x_range.clone(),
                self.y_range.clone(),
                (0..buffer.width(), 0..buffer.height()),
            );

            for frequency_sample in self.fft.borrow_mut().process() {
                let px = self.offset.get();
                for py in 0..buffer.height() {
                    let (_, f0) = cartesian_range.reverse_translate((buffer.width() - 1, py)).unwrap();
                    let (_, f1) = cartesian_range.reverse_translate((buffer.width() - 1, py + 1)).unwrap();

                    let frequency_range = (f0 as Frequency)..(f1 as Frequency);

                    let magnitude = frequency_sample.magnitude_in(frequency_range);
                    // let magnitude = to_scaled_decibels(&magnitude);

                    let py = buffer.height() - py - 1;

                    let (color, alpha) = self.palette.borrow().color_for(magnitude);
                    buffer.put_pixel(
                        px as u32,
                        py as u32,
                        color.r,
                        color.g,
                        color.b,
                        (alpha * 255.0) as u8,
                    );
                }

                // Update the offset
                self.offset.set((px + 1) % self.buffer.width() as usize);
            }

            // Draw the background
            let window_bounds = Rect::new(0.0, 0.0, width, height);
            let background_color = self.palette.borrow().background();
            snapshot.append_color(
                &RGBA::new(
                    background_color.r as f32 / 255.0,
                    background_color.g as f32 / 255.0,
                    background_color.b as f32 / 255.0,
                    1.0,
                ),
                &window_bounds,
            );

            // Swap the sides of the oscilloscope buffer, turning it into a scrolling view
            let window_space_offset = width * (self.offset.get() as f32 / TEXTURE_WIDTH as f32);
            if (self.buffer.width() - self.offset.get() as i32) > 0 {
                // If the right side has nonzero width; place it on the left
                snapshot.append_scaled_texture(
                    &Texture::for_pixbuf(&self.buffer.new_subpixbuf(
                        self.offset.get() as i32, 0,
                        self.buffer.width() - self.offset.get() as i32, self.buffer.height(),
                    )),
                    ScalingFilter::Linear,
                    &Rect::new(
                        0.0, 0.0,
                        width - window_space_offset, height,
                    ),
                );
            };
            if self.offset.get() > 0 {
                // If the left side has nonzero width; place it on the right
                snapshot.append_scaled_texture(
                    &Texture::for_pixbuf(&self.buffer.new_subpixbuf(
                        0, 0,
                        self.offset.get() as i32, self.buffer.height(),
                    )),
                    ScalingFilter::Linear,
                    &Rect::new(
                        width - window_space_offset, 0.0,
                        window_space_offset, height,
                    ),
                );
            };
        }
    }

    impl SimpleSpectrogram {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            self.fft.borrow_mut().transform = FastFourierTransform::new(
                sample_rate as Frequency,
                0.05, // todo: this should be configurable!
            )
        }
    }
}