use std::cell::RefCell;
use std::rc::Rc;

use cpal::default_host;
use cpal::Device;
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use gtk::{
    glib,
    glib::{
        Object,
        Properties,
    },
    prelude::*,
    subclass::prelude::*,
};

glib::wrapper! {
    pub struct AudioDevice(ObjectSubclass<imp::AudioDevice>);
}

impl AudioDevice {
    pub fn get_device(&self) -> Rc<Device> {
        let imp = imp::AudioDevice::from_obj(self);
        imp.device.borrow().clone()
    }
}

impl From<Rc<Device>> for AudioDevice {
    fn from(device: Rc<Device>) -> AudioDevice {
        let object = Object::builder().build();
        let imp = imp::AudioDevice::from_obj(&object);
        imp.device.replace(device);
        object
    }
}

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::AudioDevice)]
    pub struct AudioDevice {
        #[property(name = "name", get = Self::get_name, type = String)]
        pub device: RefCell<Rc<Device>>,
    }

    impl AudioDevice {
        fn get_name(&self) -> String {
            self.device.borrow().name().unwrap()
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioDevice {
        const NAME: &'static str = "AudioDevice";
        type Type = super::AudioDevice;

        fn new() -> Self {
            Self {
                device: Rc::new(default_host().default_input_device().unwrap()).into()
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for AudioDevice {}
}
