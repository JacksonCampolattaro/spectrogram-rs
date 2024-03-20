use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use ringbuf::{HeapConsumer, HeapRb};
use async_channel::Receiver;
use plotters::coord::types::RangedCoordf32;

use adw::{glib, glib::{Properties, Object}, gdk::gdk_pixbuf::{Pixbuf, Colorspace}, prelude::ObjectExt, subclass::prelude::{ObjectImpl, WidgetImpl, BoxImpl, ObjectSubclass, DerivedObjectProperties}, gdk};
use adw;
use adw::gdk::Texture;
use adw::prelude::BinExt;
use adw::subclass::prelude::ObjectSubclassExt;
use cpal::StreamConfig;
use gtk::{ContentFit, Picture};
use gtk::prelude::{BoxExt, WidgetExt};

use crate::{
    colorscheme::ColorScheme,
    fourier::interpolated_frequency_sample::InterpolatedFrequencySample,
    fourier::transform::StreamTransform,
    log_scaling::{LogCoordf64, IntoReversibleLogRange},
    fourier::Frequency,
};
use crate::fourier::StereoMagnitude;
use crate::widgets::spectrogram::Spectrogram;

const TEXTURE_WIDTH: i32 = 1024;
const TEXTURE_HEIGHT: i32 = 1024;

glib::wrapper! {
    pub struct SimpleSpectrogram(ObjectSubclass<imp::SimpleSpectrogram>)
        @extends gtk::Box, gtk::Widget;
}

impl SimpleSpectrogram {
    pub fn new(sample_stream: HeapConsumer<StereoMagnitude>, stream_config: Arc<Mutex<Option<StreamConfig>>>) -> SimpleSpectrogram {
        let object = Object::builder()
            // .property("child", picture)
            // .property("child", gtk::Picture::new())
            .build();
        let imp = imp::SimpleSpectrogram::from_obj(&object);
        let (fft, frequency_stream) = StreamTransform::new(sample_stream, stream_config);
        imp.fft.replace(fft);
        imp.frequency_stream.replace(frequency_stream);

        let picture = Picture::builder()
            .paintable(&Texture::for_pixbuf(&imp.buffer))
            .content_fit(ContentFit::Fill)
            .hexpand(true)
            .build();
        object.append(&picture);

        object
    }

    pub fn update(&self) {
        let imp = imp::SimpleSpectrogram::from_obj(self);
        imp.fft.borrow().process();
        imp.render();
        self.queue_draw();
        self.queue_resize();
    }
}

mod imp {
    use adw::gdk::RGBA;
    use gtk::gsk::ScalingFilter;
    use gtk::prelude::SnapshotExt;
    use plotters::coord::ReverseCoordTranslate;
    use plotters::prelude::Cartesian2d;
    use crate::fourier::FrequencySample;
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

        // Preprocessing details
        pub fft: RefCell<StreamTransform>,
        pub frequency_stream: RefCell<Receiver<InterpolatedFrequencySample>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SimpleSpectrogram {
        const NAME: &'static str = "SimpleSpectrogram";
        type Type = super::SimpleSpectrogram;
        type ParentType = gtk::Box;

        fn new() -> Self {
            let buffer = Pixbuf::new(
                Colorspace::Rgb,
                true,
                8,
                TEXTURE_WIDTH, TEXTURE_HEIGHT,
            ).unwrap();
            let palette: RefCell<ColorScheme> = ColorScheme::new_mono(colorous::MAGMA, "magma").into();
            let color = palette.borrow().background();
            let color = u32::from_be_bytes([color.r, color.g, color.b, 255]);
            buffer.fill(color);

            let (_, dummy_sample_stream) = HeapRb::new(1).split();
            let (dummy_fft, dummy_frequency_stream) =
                StreamTransform::new(dummy_sample_stream, Arc::new(None.into()));

            Self {
                x_range: (-10.0..0.0).into(),
                y_range: (32.0..22030.0).reversible_log_scale().base(2.0).zero_point(0.0).into(),
                palette,
                buffer,
                fft: dummy_fft.into(),
                frequency_stream: dummy_frequency_stream.into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SimpleSpectrogram {}

    impl WidgetImpl for SimpleSpectrogram {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let width = self.obj().width() as u32;
            let height = self.obj().height() as u32;
            if width == 0 || height == 0 {
                return;
            }
            let bounds = gtk::graphene::Rect::new(
                0.0, 0.0,
                width as f32, height as f32,
            );

            // Make sure there are no unprocessed audio samples
            self.fft.borrow().process();

            // Render the frequency stream to the buffer
            self.render();

            let background_color = self.palette.borrow().background();
            snapshot.append_color(
                &RGBA::new(
                    background_color.r as f32 / 255.0,
                    background_color.g as f32 / 255.0,
                    background_color.b as f32 / 255.0,
                    1.0,
                ),
                &bounds
            );
            snapshot.append_scaled_texture(
                &Texture::for_pixbuf(&self.buffer),
                ScalingFilter::Nearest,
                &bounds,
            );
        }
    }

    impl BoxImpl for SimpleSpectrogram {
        // todo
    }

    impl SimpleSpectrogram {
        pub fn render(&self) {
            let start_time = std::time::Instant::now();
            let buffer = &self.buffer;
            let frequency_stream = self.frequency_stream.borrow();

            // Shift the buffer over to make room for new data
            let num_samples = frequency_stream.len();
            buffer.copy_area(
                num_samples as i32, 0,
                buffer.width() - num_samples as i32, buffer.height(),
                buffer,
                0, 0,
            );
            println!("Shifted buffer in {:.2?}", start_time.elapsed());

            let cartesian_range: Cartesian2d<RangedCoordf32, LogCoordf64> = Cartesian2d::new(
                self.x_range.clone(),
                self.y_range.clone(),
                (0..buffer.width(), 0..buffer.height()),
            );
            for px in 0..num_samples {
                // todo: should this really be blocking?
                let sample = frequency_stream.recv_blocking().unwrap();

                for py in 0..buffer.height() {
                    let (_, f0) = cartesian_range.reverse_translate((buffer.width() - 1, py)).unwrap();
                    let (_, f1) = cartesian_range.reverse_translate((buffer.width() - 1, py + 1)).unwrap();

                    let frequency_range = (f0 as Frequency)..(f1 as Frequency);

                    let magnitude = sample.magnitude_in(frequency_range);
                    // let magnitude = to_scaled_decibels(&magnitude);

                    let px = (buffer.width() - num_samples as i32) + px as i32;
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
            }
            println!("Rendered {} samples in {:.2?}", num_samples, start_time.elapsed());
        }
    }
}