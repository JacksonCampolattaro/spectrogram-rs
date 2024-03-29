use adw::glib::Object;
use std::error::Error;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use cpal::StreamConfig;
use async_channel::Receiver;
use ringbuf::HeapRb;
use crate::fourier::interpolated_frequency_sample::InterpolatedFrequencySample;

use gtk::{
    gdk,
    glib,
    glib::Properties,
    gsk::ScalingFilter,
    prelude::*,
    subclass::prelude::*,
};

use gdk::{
    Texture,
    gdk_pixbuf::*,
};

use plotters::prelude::*;
use plotters::coord::{
    ReverseCoordTranslate,
    types::RangedCoordf32,
};
use plotters_cairo::CairoBackend;
use ringbuf::HeapConsumer;

use crate::fourier::{FrequencySample, Frequency, StereoMagnitude, transform::StreamTransform};
use crate::log_scaling::*;
use crate::colorscheme::*;

const TEXTURE_WIDTH: i32 = 1024;
const TEXTURE_HEIGHT: i32 = 1024;

glib::wrapper! {
    pub struct Spectrogram(ObjectSubclass<imp::Spectrogram>)
        @extends gtk::Widget;
}

impl Spectrogram {
    pub fn new(sample_stream: HeapConsumer<StereoMagnitude>, stream_config: Arc<Mutex<Option<StreamConfig>>>) -> Spectrogram {
        let object = Object::builder().build();
        let imp = imp::Spectrogram::from_obj(&object);
        let (fft, frequency_stream) = StreamTransform::new(sample_stream, stream_config);
        imp.fft.replace(fft);
        imp.frequency_stream.replace(frequency_stream);

        object
    }
}

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::Spectrogram)]
    pub struct Spectrogram {
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
    impl ObjectSubclass for Spectrogram {
        const NAME: &'static str = "Spectrogram";
        type Type = super::Spectrogram;
        type ParentType = gtk::Widget;

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
    impl ObjectImpl for Spectrogram {}

    impl WidgetImpl for Spectrogram {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let width = self.obj().width() as u32;
            let height = self.obj().height() as u32;
            if width == 0 || height == 0 {
                return;
            }

            // Make sure there are no unprocessed audio samples
            self.fft.borrow().process();

            // Render the frequency stream to the buffer
            self.render();

            // Draw the plot
            let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            self.plot(snapshot, &bounds).unwrap();
        }
    }

    impl Spectrogram {
        fn render(&self) {
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

        fn plot(
            &self,
            snapshot: &gtk::Snapshot,
            bounds: &gtk::graphene::Rect,
        ) -> Result<(), Box<dyn Error>> {
            let start_time = std::time::Instant::now();
            let background_color = self.palette.borrow().background();
            let background_color = RGBColor(background_color.r, background_color.g, background_color.b);
            let foreground_color = self.palette.borrow().foreground();
            let foreground_color = RGBColor(foreground_color.r, foreground_color.g, foreground_color.b);

            // Start by drawing the plot bounds
            let pixel_range = {
                let cr = snapshot.append_cairo(&bounds.clone());
                let root = CairoBackend::new(
                    &cr,
                    (bounds.width() as u32, bounds.height() as u32),
                ).unwrap().into_drawing_area();

                root.fill(&background_color).unwrap();

                let mut chart = ChartBuilder::on(&root)
                    .margin(16)
                    .x_label_area_size(16)
                    .y_label_area_size(45)
                    .build_cartesian_2d(
                        self.x_range.clone(),
                        self.y_range.clone(),
                    )
                    .unwrap();


                chart
                    .configure_mesh()
                    .label_style(("sans-serif", 10, &foreground_color))
                    .axis_style(&foreground_color)
                    .disable_mesh()
                    .draw()
                    .unwrap();

                root.present().expect("Failed to present plot");

                let pixel_range = chart.plotting_area().get_pixel_range();
                gtk::graphene::Rect::new(
                    (pixel_range.0.start) as f32 - 0.5,
                    (pixel_range.1.start) as f32 - 0.5,
                    (pixel_range.0.end - pixel_range.0.start) as f32,
                    (pixel_range.1.end - pixel_range.1.start) as f32,
                )
            };

            // Draw the contents of the plot
            let texture = Texture::for_pixbuf(&self.buffer);
            snapshot.append_scaled_texture(
                &texture,
                ScalingFilter::Nearest,
                &pixel_range,
            );

            println!("Drew complete plot in {:?}", start_time.elapsed());
            Ok(())
        }
    }
}
