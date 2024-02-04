use adw::glib::Object;
use std::error::Error;
use std::cell::RefCell;

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

use crate::fourier::frequency_sample::{Frequency, FrequencySample};
use crate::log_scaling::*;
use crate::colorscheme::*;

glib::wrapper! {
    pub struct Spectrogram(ObjectSubclass<imp::Spectrogram>)
        @extends gtk::Widget;
}

impl Spectrogram {
    pub fn new() -> Spectrogram {
        Object::builder().build()
    }

    pub fn push_frequency_samples<I>(&self, samples: I)
        where I: IntoIterator,
              I::IntoIter: ExactSizeIterator,
              I::Item: FrequencySample {
        let start_time = std::time::Instant::now();
        let self_ = imp::Spectrogram::from_obj(self);
        let buffer = &self_.buffer;
        let samples = samples.into_iter();

        let num_samples = samples.len();

        let cartesian_range: Cartesian2d<RangedCoordf32, LogCoordf64> = Cartesian2d::new(
            self_.x_range.clone(),
            self_.y_range.clone(),
            (0..buffer.width(), 0..buffer.height()),
        );

        // Shift the buffer over by n pixels
        buffer.copy_area(
            num_samples as i32, 0,
            buffer.width() - num_samples as i32, buffer.height(),
            buffer,
            0, 0,
        );

        for (px, sample) in samples.enumerate() {
            for py in 0..buffer.height() {
                let (_, f0) = cartesian_range.reverse_translate((buffer.width() - 1, py)).unwrap();
                let (_, f1) = cartesian_range.reverse_translate((buffer.width() - 1, py + 1)).unwrap();

                let frequency_range = (f0 as Frequency)..(f1 as Frequency);

                let magnitude = sample.magnitude_in(frequency_range);
                // let magnitude = to_scaled_decibels(&magnitude);

                let px = (buffer.width() - num_samples as i32) + px as i32;
                let py = buffer.height() - py - 1;

                let color = self_.palette.borrow().color_for(magnitude);
                // let color = self_.palette.borrow().get_gradient().eval_continuous(magnitude[0] as f64);
                buffer.put_pixel(
                    px as u32,
                    py as u32,
                    color.r,
                    color.g,
                    color.b,
                    255,
                );
            }
        }

        self.queue_draw();
        println!("Raster time: {:.2?}", start_time.elapsed());
    }
}

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::Spectrogram)]
    pub struct Spectrogram {
        pub x_range: RangedCoordf32,
        pub y_range: LogCoordf64,
        #[property(get, set)]
        pub palette: RefCell<ColorScheme>,
        pub buffer: Pixbuf,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Spectrogram {
        const NAME: &'static str = "Spectrogram";
        type Type = super::Spectrogram;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            let buffer = Pixbuf::new(
                Colorspace::Rgb,
                false,
                8,
                2048, 1024,
            ).unwrap();
            let palette: RefCell<ColorScheme> = ColorScheme::new_mono(colorous::MAGMA, "magma").into();
            let color = palette.borrow().background();
            let color = u32::from_be_bytes([color.r, color.g, color.b, 255]);
            buffer.fill(color);
            Self {
                x_range: (-10.0..0.0).into(),
                y_range: (32.0..22030.0).reversible_log_scale().base(2.0).zero_point(0.0).into(),
                palette,
                buffer,
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

            let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            self.plot(snapshot, &bounds).unwrap();
        }
    }

    impl Spectrogram {
        fn plot(
            &self,
            snapshot: &gtk::Snapshot,
            bounds: &gtk::graphene::Rect,
        ) -> Result<(), Box<dyn Error>> {
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
                ScalingFilter::Nearest, //
                &pixel_range,
            );

            Ok(())
        }
    }
}
