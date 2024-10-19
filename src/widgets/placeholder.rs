use std::cell::RefCell;
use ringbuf::{HeapCons, HeapRb, traits::{Consumer, Split}};
use adw::glib;
use adw::glib::{Object, Properties, ControlFlow::Continue};
use gtk::subclass::button::ButtonImpl;
use adw::subclass::prelude::{ObjectImpl, ObjectSubclass, ObjectSubclassExt, ObjectSubclassIsExt, WidgetImpl, WidgetImplExt, DerivedObjectProperties};
use gtk::prelude::{WidgetExtManual, ObjectExt, ButtonExt};
use itertools::Itertools;

use crate::fourier::StereoMagnitude;

glib::wrapper! {
    pub struct PlaceholderVisualizer(ObjectSubclass<imp::PlaceholderVisualizer>)
        @extends gtk::Button, gtk::Widget;
}

impl PlaceholderVisualizer {
    pub fn new(sample_stream: HeapCons<StereoMagnitude>) -> Self {
        let object = Object::builder().build();
        let imp = imp::PlaceholderVisualizer::from_obj(&object);
        object.add_tick_callback(|w, _| {
            let v = w.imp().input_stream.borrow_mut().pop_iter().collect_vec();
            w.set_label(format!("{}", v.len()).as_str());
            Continue
        });
        imp.input_stream.replace(sample_stream);
        object
    }
}

mod imp {
    use crate::colorscheme::ColorScheme;
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::PlaceholderVisualizer)]
    pub struct PlaceholderVisualizer {
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        pub input_stream: RefCell<HeapCons<StereoMagnitude>>,

        #[property(get, set)]
        pub palette: RefCell<ColorScheme>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaceholderVisualizer {
        const NAME: &'static str = "PlaceholderVisualizer";
        type Type = super::PlaceholderVisualizer;
        type ParentType = gtk::Button;

        fn new() -> Self {
            let (_, dummy_sample_stream) = HeapRb::new(1).split();
            Self {
                input_stream: dummy_sample_stream.into(),
                palette: ColorScheme::new_mono(colorous::MAGMA, "magma").into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for PlaceholderVisualizer {}

    impl WidgetImpl for PlaceholderVisualizer {}

    impl ButtonImpl for PlaceholderVisualizer {}

    impl PlaceholderVisualizer {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            // todo!()
        }
    }
}
