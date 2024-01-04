use std::cell::{Cell, RefCell};
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
use colorous::{BLUES, CIVIDIS, COOL, CUBEHELIX, Gradient, GREENS, GREYS, INFERNO, MAGMA, ORANGES, PLASMA, REDS, SPECTRAL, TURBO, VIRIDIS};

glib::wrapper! {
    pub struct ColorScheme(ObjectSubclass<imp::ColorScheme>);
}

impl ColorScheme {
    pub fn new(gradient: Gradient, name: &str) -> Self {
        let object = Object::builder().build();
        let imp = imp::ColorScheme::from_obj(&object);
        imp.gradient.replace(gradient);
        imp.name.replace(name.into());
        object
    }

    pub fn get_gradient(&self) -> Gradient {
        let imp = imp::ColorScheme::from_obj(self);
        imp.gradient.get()
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
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ColorScheme {
        const NAME: &'static str = "ColorScheme";
        type Type = super::ColorScheme;

        fn new() -> Self {
            Self {
                name: String::new().into(),
                gradient: colorous::GREYS.into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ColorScheme {}
}

pub fn default_color_schemes() -> ListStore {
    let mut list = ListStore::builder()
        .item_type(ColorScheme::static_type())
        .build();
    list.extend_from_slice(&[
        ColorScheme::new(MAGMA, "Magma"),
        ColorScheme::new(VIRIDIS, "Viridis"),
        ColorScheme::new(INFERNO, "Inferno"),
        ColorScheme::new(PLASMA, "Plasma"),
        ColorScheme::new(CIVIDIS, "Cividis"),
        ColorScheme::new(CUBEHELIX, "Cube-helix"),
        ColorScheme::new(COOL, "Cool"),
        ColorScheme::new(REDS, "Reds"),
        ColorScheme::new(BLUES, "Blues"),
        ColorScheme::new(GREENS, "Greens"),
        ColorScheme::new(GREYS, "Greys"),
        ColorScheme::new(ORANGES, "Oranges"),
    ]);
    list
}