use gtk::glib;
use glib::Object;
use gtk::prelude::*;
use gtk::subclass::prelude::ObjectSubclassExt;

glib::wrapper! {
    pub struct SpectrumAnalyzer(ObjectSubclass<imp::SpectrumAnalyzer>)
        @extends gtk::Box, gtk::Widget;
}

impl SpectrumAnalyzer {
    pub fn new() -> SpectrumAnalyzer {
        Object::builder().build()
    }

    pub fn set_frequencies(&mut self, magnitudes: &[f32]) {
        // let min = magnitudes.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap().clone();
        // let max = magnitudes.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap().clone();
        let min = -70.0;
        let max = 0.0;
        let self_ = imp::SpectrumAnalyzer::from_obj(self);
        for (i, bar) in &mut self_.level_bars.iter().enumerate() {
            // todo: this is currently a misnomer
            let frequency = (i as f32) / (self_.level_bars.len() as f32);
            let magnitude = magnitudes[(frequency * magnitudes.len() as f32).floor() as usize];
            // let magnitude = magnitudes[i];
            bar.set_value(((magnitude - min) / (max - min)) as f64);
        }
    }
}

mod imp {
    use gtk::{glib, Orientation};
    use glib::prelude::*;
    use gtk::subclass::prelude::*;
    use gtk::{LevelBar, Box};
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
                level_bars: (0..64).map(
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
