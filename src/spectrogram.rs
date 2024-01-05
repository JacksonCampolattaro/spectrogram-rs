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

use crate::fourier::FrequencySample;
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

    pub fn push_frequencies(&self, frequency_sample: FrequencySample) {
        self.push_frequency_block(&[frequency_sample]);
    }

    pub fn push_frequency_block(&self, frequency_samples: &[FrequencySample]) {
        let self_ = imp::Spectrogram::from_obj(self);
        let buffer = &self_.buffer;

        let num_samples = frequency_samples.len() as i32;

        let cartesian_range: Cartesian2d<RangedCoordf32, LogCoordf64> = Cartesian2d::new(
            self_.x_range.clone(),
            self_.y_range.clone(),
            (0..buffer.width(), 0..buffer.height()),
        );

        // Shift the buffer over by n pixels
        buffer.copy_area(
            num_samples, 0,
            buffer.width() - num_samples, buffer.height(),
            buffer,
            0, 0,
        );

        let min_db = -70.0;
        let max_db = 32.0;

        // Write values to the right column
        for (px, frequency_sample) in frequency_samples.iter().enumerate() {
            for py in 0..buffer.height() {
                let (_, f0) = cartesian_range.reverse_translate((buffer.width() - 1, py)).unwrap();
                let (_, f1) = cartesian_range.reverse_translate((buffer.width() - 1, py + 1)).unwrap();

                let magnitude = frequency_sample.mean_magnitude_of_frequency_range(f0 as f32, f1 as f32);
                let magnitude = 20.0 * (magnitude + 1e-7).log10();
                let magnitude = ((magnitude - min_db) / (max_db - min_db)) as f64;

                let px = (buffer.width() - num_samples) + px as i32;
                let py = buffer.height() - py - 1;

                let color = self_.palette.borrow().get_gradient().eval_continuous(magnitude);
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
            let palette: RefCell<ColorScheme> = ColorScheme::new(colorous::MAGMA, "magma").into();
            let color = palette.borrow().get_gradient().eval_continuous(0.0);
            let color = u32::from_be_bytes([color.r, color.g, color.b, 255]);
            buffer.fill(color);
            Self {
                x_range: (-10.0..0.0).into(),
                y_range: (32.0..22050.0).reversible_log_scale().base(2.0).zero_point(0.0).into(),
                palette,
                buffer
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
            let background_color = self.palette.borrow().get_gradient().eval_continuous(0.0);
            let background_color = RGBColor(background_color.r, background_color.g, background_color.b);
            let foreground_color = self.palette.borrow().get_gradient().eval_continuous(1.0);
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
