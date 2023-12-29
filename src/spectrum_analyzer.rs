use std::iter::{FromFn, zip};
use gtk::glib;
use glib::Object;
use gtk::prelude::*;
use gtk::subclass::prelude::ObjectSubclassExt;

use num_traits::{Float, Pow};

use crate::fourier::FrequencySample;

fn log_space(start: f32, end: f32, n: usize, base: f32) -> impl Iterator<Item=f32> + Clone {
    // println!("{}, {}", start, end);
    let start = start.log(base);
    let end = end.log(base);
    // println!("-> {}, {}", start, end);
    let step = (end - start) / n as f32;

    let mut i = 0;
    std::iter::from_fn(move || {
        if i > n { () }

        let linear_value = start + (step * i as f32);
        i = i + 1;
        // println!("{}, {}", linear_value, base.pow(linear_value));
        Some(base.pow(linear_value))
    })
}

glib::wrapper! {
    pub struct SpectrumAnalyzer(ObjectSubclass<imp::SpectrumAnalyzer>)
        @extends gtk::Box, gtk::Widget;
}

impl SpectrumAnalyzer {
    pub fn new() -> SpectrumAnalyzer {
        Object::builder().build()
    }

    pub fn push_frequencies(&mut self, frequency_sample: FrequencySample) {
        // let min = magnitudes.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap().clone();
        // let max = magnitudes.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap().clone();
        let min = -70.0;
        let max = -10.0;
        let self_ = imp::SpectrumAnalyzer::from_obj(self);

        let frequencies = log_space(
            32.0,
            frequency_sample.max_frequency().max(22050.0),
            self_.level_bars.len() + 1,
            10.0,
        );
        let frequency_ranges = zip(frequencies.clone(), frequencies.skip(1));

        for (bar, (frequency_start, frequency_end)) in self_.level_bars.iter().zip(frequency_ranges) {
            let magnitude = frequency_sample.mean_magnitude_of_frequency_range(frequency_start, frequency_end);
            let magnitude = 10.0 * (magnitude + 1e-7).log10();
            let magnitude = ((magnitude - min) / (max - min)) as f64;

            bar.set_value(magnitude.max(bar.value() * 0.99));
        }
    }
}

mod imp {
    use gtk::{glib, Orientation};
    use glib::prelude::*;
    use gtk::subclass::prelude::*;
    use gtk::{LevelBar};
    use gtk::prelude::{BoxExt, WidgetExt};

    #[derive(Default)]
    pub struct SpectrumAnalyzer {
        pub level_bars: Vec<LevelBar>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for SpectrumAnalyzer {
        const NAME: &'static str = "SpectrogramSpectrumAnalyzer";
        type Type = super::SpectrumAnalyzer;
        type ParentType = gtk::Box;

        fn new() -> Self {
            Self {
                level_bars: (0..128).map(
                    |_| LevelBar::builder()
                        .hexpand(true)
                        .vexpand(true)
                        .value(0.3)
                        .orientation(Orientation::Vertical)
                        .inverted(true)
                        .build()
                ).collect()
            }
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for SpectrumAnalyzer {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_spacing(1);
            self.obj().set_margin_bottom(4);
            self.obj().set_margin_top(4);
            self.obj().set_margin_start(4);
            self.obj().set_margin_end(4);
            for bar in self.level_bars.iter() { self.obj().append(bar) };
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for SpectrumAnalyzer {}

    // Trait shared by all boxes
    impl BoxImpl for SpectrumAnalyzer {}
}
