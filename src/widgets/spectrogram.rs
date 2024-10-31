use adw::glib::Object;
use std::error::Error;
use std::cell::RefCell;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use cpal::StreamConfig;
use async_channel::Receiver;
use ringbuf::HeapRb;
use crate::widgets::simple_spectrogram::SimpleSpectrogram;

use gtk::{
    gdk,
    glib,
    glib::Properties,
    prelude::*,
    subclass::prelude::*,
};

use gdk::{
    Texture,
    gdk_pixbuf::*,
};

use plotters::prelude::*;
use plotters_cairo::CairoBackend;
use ringbuf::{HeapCons, traits::Split};

use crate::fourier::{FrequencySample, Frequency, StereoMagnitude};
use crate::log_scaling::*;
use crate::colorscheme::*;

const TEXTURE_WIDTH: i32 = 1024;
const TEXTURE_HEIGHT: i32 = 1024;

glib::wrapper! {
    pub struct Spectrogram(ObjectSubclass<imp::Spectrogram>)
        @extends gtk::Fixed, gtk::Widget;
}

impl Spectrogram {
    pub fn new(sample_stream: HeapCons<StereoMagnitude>) -> Spectrogram {
        let object = Object::builder()
            // .property("spectrogram", SimpleSpectrogram::new(sample_stream))
            .build();
        let imp = imp::Spectrogram::from_obj(&object);
        // todo
        object
    }
}

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::Spectrogram)]
    pub struct Spectrogram {
        // #[property(name = "palette", set=Self::set_palette, type=ColorScheme)]
        #[property(name = "palette", set = Self::set_palette)]
        #[property(name = "sample-rate", set = Self::set_sample_rate)]
        // #[property(get, set)]
        pub spectrogram: RefCell<SimpleSpectrogram>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Spectrogram {
        const NAME: &'static str = "Spectrogram";
        type Type = super::Spectrogram;
        type ParentType = gtk::Fixed;

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

            Self {
                spectrogram: SimpleSpectrogram::new(dummy_sample_stream).into()
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Spectrogram {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().put(
                self.spectrogram.borrow().deref(),
                0.0, 0.0,
            )
            // todo
        }
    }

    impl WidgetImpl for Spectrogram {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let width = self.obj().width() as f32;
            let height = self.obj().height() as f32;
            if width == 0.0 || height == 0.0 {
                return;
            }

            // Draw the plot
            // let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            // self.plot(snapshot, &bounds).unwrap();

            // Draw the spectrogram itself
            // let s = self.spectrogram.borrow().deref();
            // self.obj().snapshot_child(&self.obj().spectrogram(), snapshot);
        }
    }

    impl FixedImpl for Spectrogram {}

    impl Spectrogram {
        pub fn set_palette(&self, palette: ColorScheme) {
            // self.spectrogram.borrow().set_palette(palette)
        }

        pub fn set_sample_rate(&self, sample_rate: u32) {
            // self.spectrogram.borrow().set_sample_rate(sample_rate)
        }

        // fn plot
        //     &self,
        //     snapshot: &gtk::Snapshot,
        //     bounds: &gtk::graphene::Rect,
        // ) -> Result<(), Box<dyn Error>> {
        //     let start_time = std::time::Instant::now();
        //     let background_color = self.palette.borrow().background();
        //     let background_color = RGBColor(background_color.r, background_color.g, background_color.b);
        //     let foreground_color = self.palette.borrow().foreground();
        //     let foreground_color = RGBColor(foreground_color.r, foreground_color.g, foreground_color.b);
        //
        //     // Start by drawing the plot bounds
        //     let pixel_range = {
        //         let cr = snapshot.append_cairo(&bounds.clone());
        //         let root = CairoBackend::new(
        //             &cr,
        //             (bounds.width() as u32, bounds.height() as u32),
        //         ).unwrap().into_drawing_area();
        //
        //         root.fill(&background_color).unwrap();
        //
        //         let mut chart = ChartBuilder::on(&root)
        //             .margin(16)
        //             .x_label_area_size(16)
        //             .y_label_area_size(45)
        //             .build_cartesian_2d(
        //                 self.x_range.clone(),
        //                 self.y_range.clone(),
        //             )
        //             .unwrap();
        //
        //
        //         chart
        //             .configure_mesh()
        //             .label_style(("sans-serif", 10, &foreground_color))
        //             .axis_style(&foreground_color)
        //             .disable_mesh()
        //             .draw()
        //             .unwrap();
        //
        //         root.present().expect("Failed to present plot");
        //
        //         let pixel_range = chart.plotting_area().get_pixel_range();
        //         gtk::graphene::Rect::new(
        //             (pixel_range.0.start) as f32 - 0.5,
        //             (pixel_range.1.start) as f32 - 0.5,
        //             (pixel_range.0.end - pixel_range.0.start) as f32,
        //             (pixel_range.1.end - pixel_range.1.start) as f32,
        //         )
        //     };
        //
        //     // Draw the contents of the plot
        //     let texture = Texture::for_pixbuf(&self.buffer);
        //     snapshot.append_scaled_texture(
        //         &texture,
        //         ScalingFilter::Nearest,
        //         &pixel_range,
        //     );
        //
        //     println!("Drew complete plot in {:?}", start_time.elapsed());
        //     Ok(())
        // }
    }
}
