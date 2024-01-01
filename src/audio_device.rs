use std::cell::RefCell;
use cpal::Device;
use cpal::traits::DeviceTrait;
use gtk::{
    glib,
    glib::{
        Properties,
        GString,
        Object,
    },
    prelude::*,
    subclass::prelude::*,
};

glib::wrapper! {
    pub struct AudioDevice(ObjectSubclass<imp::AudioDevice>);
}

impl From<&Device> for AudioDevice {
    fn from(device: &Device) -> AudioDevice {
        Object::builder()
            .property("name", GString::from(device.name().unwrap()))
            .build()
    }
}

mod imp {
    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::AudioDevice)]
    pub struct AudioDevice {
        // todo
        #[property(get, set)]
        pub name: RefCell<GString>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioDevice {
        const NAME: &'static str = "AudioDevice";
        type Type = super::AudioDevice;

        fn new() -> Self {
            Self { name: GString::from("[uninitialized]").into() }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for AudioDevice {}
}
