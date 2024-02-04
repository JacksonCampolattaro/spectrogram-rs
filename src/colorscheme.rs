use std::cell::{Cell, RefCell};
use std::iter::zip;
use gtk::{
    glib,
    glib::{
        Object,
        Properties,
    },
    gio::ListStore,
    prelude::*,
    subclass::prelude::*,
};
use colorous::*;
use crate::fourier::frequency_sample::StereoMagnitude;

const MIN_DB: f32 = -70.0;
const MAX_DB: f32 = 0.0;

glib::wrapper! {
    pub struct ColorScheme(ObjectSubclass<imp::ColorScheme>);
}

impl ColorScheme {
    pub fn new_mono(gradient: Gradient, name: &str) -> Self {
        let object = Object::builder().build();
        let imp = imp::ColorScheme::from_obj(&object);
        imp.gradient.replace(gradient);
        imp.name.replace(name.into());
        object
    }

    pub fn new_stereo(gradient: Gradient, background: Color, name: &str) -> Self {
        let object = Object::builder().build();
        let imp = imp::ColorScheme::from_obj(&object);
        imp.gradient.replace(gradient);
        imp.background.replace(background.into());
        imp.name.replace(name.into());
        object
    }

    pub fn background(&self) -> Color {
        let imp = imp::ColorScheme::from_obj(self);
        imp.background.get().unwrap_or(imp.gradient.get().eval_continuous(0.0))
    }

    pub fn foreground(&self) -> Color {
        let imp = imp::ColorScheme::from_obj(self);
        if imp.background.get().is_none() {
            imp.gradient.get().eval_continuous(1.0)
        } else {
            imp.gradient.get().eval_continuous(0.5)
        }
    }

    pub fn color_for(&self, magnitude: StereoMagnitude) -> Color {
        let imp = imp::ColorScheme::from_obj(self);
        let background = imp.background.get();

        let total_magnitude = magnitude.norm_sqr();
        let magnitude_db = 10.0 * (total_magnitude + 1e-7).log10();
        let magnitude_bounded = (magnitude_db - MIN_DB) / (MAX_DB - MIN_DB);

        if background.is_none() {
            imp.gradient.get().eval_continuous(magnitude_bounded as f64)
        } else {
            // If a background is provided, the foreground is based on a diverging gradient
            let distribution = magnitude.re as f64 / magnitude.l1_norm() as f64;
            //println!("{}, {} -> {}", magnitude.re, magnitude.im, distribution);
            let foreground = imp.gradient.get().eval_continuous(distribution);

            // We need to mix the foreground & background based on the mean magnitude
            let mut mixed = zip(foreground.as_array(), background.unwrap().as_array())
                .map(|(f, b)| {
                    ((f as f32 * magnitude_bounded) + (b as f32 * (1.0 - magnitude_bounded))) as u8
                });
            // todo
            Color {
                r: mixed.next().unwrap(),
                g: mixed.next().unwrap(),
                b: mixed.next().unwrap(),
            }
        }
    }
}


mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::ColorScheme)]
    pub struct ColorScheme {
        #[property(get)]
        pub name: RefCell<String>,
        pub gradient: Cell<Gradient>,
        pub background: Cell<Option<Color>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ColorScheme {
        const NAME: &'static str = "ColorScheme";
        type Type = super::ColorScheme;

        fn new() -> Self {
            Self {
                name: String::new().into(),
                gradient: colorous::GREYS.into(),
                background: None.into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ColorScheme {}
}

pub fn default_color_schemes() -> ListStore {
    let list = ListStore::builder()
        .item_type(ColorScheme::static_type())
        .build();
    list.extend_from_slice(&[
        ColorScheme::new_mono(MAGMA, "Magma"),
        ColorScheme::new_mono(VIRIDIS, "Viridis"),
        ColorScheme::new_stereo(RED_BLUE, Color { r: 0, g: 0, b: 0 }, "Red-Blue"),
        ColorScheme::new_mono(INFERNO, "Inferno"),
        ColorScheme::new_mono(PLASMA, "Plasma"),
        ColorScheme::new_mono(CIVIDIS, "Cividis"),
        ColorScheme::new_mono(CUBEHELIX, "Cube-helix"),
        ColorScheme::new_mono(TURBO, "Turbo"),
        ColorScheme::new_mono(COOL, "Cool"),
        ColorScheme::new_mono(REDS, "Reds"),
        ColorScheme::new_mono(BLUES, "Blues"),
        ColorScheme::new_mono(GREENS, "Greens"),
        ColorScheme::new_mono(GREYS, "Greys"),
        ColorScheme::new_mono(ORANGES, "Oranges"),
    ]);
    list
}