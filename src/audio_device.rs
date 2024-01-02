use std::cell::{
    Cell,
    RefCell,
};
use std::ops::Deref;
use std::rc::Rc;
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

impl AudioDevice {
    pub fn get_device(&self) -> Rc<Device> {
        let mut imp = imp::AudioDevice::from_obj(self);
        imp.device.borrow().clone()
    }
}

impl From<Rc<Device>> for AudioDevice {
    fn from(device: Rc<Device>) -> AudioDevice {
        let object = Object::builder().build();
        let mut imp = imp::AudioDevice::from_obj(&object);
        imp.device.replace(device);
        object
    }
}

mod imp {
    use std::cell::Ref;
    use std::marker::PhantomData;
    use cpal::default_host;
    use cpal::traits::HostTrait;
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
