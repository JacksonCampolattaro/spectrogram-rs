use std::hash::Hash;
use std::ops::Range;
use adw::glib::Object;
use gtk::glib;
use gtk::prelude::WidgetExt;
use gtk::subclass::prelude::*;
use plotters::coord::ranged1d::{DefaultFormatting, KeyPointHint, ReversibleRanged};
use plotters::coord::{CoordTranslate, ReverseCoordTranslate};
use plotters::coord::types::RangedCoordf32;
use plotters::prelude::{Cartesian2d, LogCoord, LogScalable, Ranged};
use plotters_backend::BackendCoord;
use crate::fourier::FrequencySample;

glib::wrapper! {
    pub struct Spectrogram(ObjectSubclass<imp::Spectrogram>)
        @extends gtk::Widget;
}

impl Spectrogram {
    pub fn new() -> Spectrogram {
        Object::builder().build()
    }

    pub fn push_frequencies(&mut self, frequency_sample: FrequencySample) {
        let self_ = imp::Spectrogram::from_obj(self);
        let buffer = &self_.buffer;


        let cartesian_range: Cartesian2d<RangedCoordf32, RangedCoordf32> = Cartesian2d::new(
            self_.x_range.clone(),
            self_.y_range.clone(),
            (0..buffer.width(), 0..buffer.height()),
        );

        // Shift the buffer over by one pixel
        buffer.copy_area(
            1, 0,
            buffer.width() - 1, buffer.height(),
            buffer,
            0, 0,
        );

        let min_db = -70.0;
        let max_db = -0.0;

        // Write values to the right column
        for py in 1..buffer.height() {

            let (t, f0) = cartesian_range.reverse_translate((buffer.width() - 1, py - 1)).unwrap();
            let (t, f1) = cartesian_range.reverse_translate((buffer.width() - 1, py)).unwrap();

            let py= buffer.height() - py;

            let magnitude = frequency_sample.mean_magnitude_of_frequency_range(f0, f1);
            let magnitude = 10.0 * (magnitude + 1e-7).log10();
            let magnitude = ((magnitude - min_db) / (max_db - min_db)) as f64;

            buffer.put_pixel(
                (buffer.width() - 1) as u32, py as u32,
                (magnitude * 255.0) as u8,
                0,
                0,
                255,
            );
        }

        self.queue_draw();

        //println!("Updating buffer");

        // todo
    }
}

mod imp {
    use std::error::Error;
    use std::io;
    use std::ops::Range;
    use adw::gdk::gdk_pixbuf::Colorspace;
    use adw::glib::Object;

    use gtk::{gdk, glib, LevelBar, Orientation};
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;

    use gdk::Texture;

    use glib::prelude::*;
    use gtk::gdk::RGBA;
    use gtk::gdk_pixbuf::Pixbuf;
    use gtk::graphene::Point;
    use gtk::gsk::ColorStop;
    use plotters::element::Drawable;

    use plotters::prelude::*;
    use plotters_cairo::CairoBackend;
    use plotters::coord::{CoordTranslate, ReverseCoordTranslate};
    use plotters::coord::types::RangedCoordf32;
    use crate::fourier::FrequencySample;

    use crate::log_scaling::*;
    use crate::spectrogram::imp;
    use crate::spectrum_analyzer::SpectrumAnalyzer;

    pub struct Spectrogram {
        pub x_range: RangedCoordf32,
        pub y_range: RangedCoordf32,
        pub buffer: Pixbuf,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Spectrogram {
        const NAME: &'static str = "Spectrogram";
        type Type = super::Spectrogram;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            // todo: log scaling will require a custom range implementation
            // let y_range = ReversibleLogCoord((32.0..22050.0).log_scale().base(2.0).into());
            Self {
                x_range: (-10.0..0.0).into(),
                y_range: (32.0..22050.0).into(),
                buffer: Pixbuf::new(
                    Colorspace::Rgb,
                    false,
                    8,
                    1024, 1024,
                ).unwrap(),
            }
        }
    }

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

            // Start by drawing the plot bounds
            let pixel_range = {
                let cr = snapshot.append_cairo(&bounds.clone());
                let root = CairoBackend::new(
                    &cr,
                    (bounds.width() as u32, bounds.height() as u32),
                ).unwrap().into_drawing_area();


                root.fill(&WHITE).unwrap();

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
                    .disable_mesh()
                    .draw()
                    .unwrap();

                root.present().expect("Failed to present plot");

                let pixel_range = chart.plotting_area().get_pixel_range();
                gtk::graphene::Rect::new(
                    (pixel_range.0.start - 1) as f32,
                    (pixel_range.1.start) as f32,
                    (pixel_range.0.end - pixel_range.0.start) as f32,
                    (pixel_range.1.end - pixel_range.1.start) as f32,
                )
            };

            let texture = Texture::for_pixbuf(&self.buffer);

            // for px in 0..1024 {
            //     for py in 0..1024 {
            //         let (t, f) = cartesian_range.reverse_translate((px, py)).unwrap();
            //
            //         let red = t / -10.0;
            //         let blue = f / 22050.0;
            //
            //         self.buffer.put_pixel(
            //             px as u32, py as u32,
            //             (red * 255.0) as u8,
            //             0,
            //             (blue * 255.0) as u8,
            //             255,
            //         );
            //     }
            // }


            snapshot.append_texture(
                &texture,
                &pixel_range,
            );

            Ok(())
        }
    }
}
