use gtk::{
    *,
    glib,
    glib::*,
    prelude::*,
    subclass::prelude::*,
    gio::ListModel,
};

use crate::audio_device::AudioDevice;


glib::wrapper! {
    pub struct AudioInputListModel(ObjectSubclass<imp::AudioInputListModel>)
        @implements ListModel;
}

impl AudioInputListModel {
    pub fn new() -> AudioInputListModel {
        Object::builder().build()
    }
}

mod imp {
    use cpal::traits::{DeviceTrait, HostTrait};
    use itertools::Itertools;
    use std::rc::Rc;
    use super::*;

    pub struct AudioInputListModel {
        host: cpal::Host,
        devices: Vec<Rc<cpal::Device>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioInputListModel {
        const NAME: &'static str = "AudioInputListModel";
        type Type = super::AudioInputListModel;
        type Interfaces = (ListModel, );

        fn new() -> Self {
            let host = cpal::default_host();
            let default_device_name = host.default_input_device().unwrap().name().unwrap();
            let devices = host.input_devices().unwrap()
                .sorted_by_cached_key(|d| { return d.name().unwrap() != default_device_name; })
                .map(Rc::from)
                .collect();
            Self {
                host,
                devices,
            }
        }
    }

    impl ObjectImpl for AudioInputListModel {}

    impl ListModelImpl for AudioInputListModel {
        fn item_type(&self) -> Type {
            AudioDevice::static_type()
        }

        fn n_items(&self) -> u32 {
            self.devices.len() as u32
        }

        fn item(&self, position: u32) -> Option<Object> {
            self.devices.iter()
                .nth(position as usize)
                .map(|device| { AudioDevice::from(device.clone()).into() })
            // todo: this could be cleaned up
            //Some(AudioDevice::from(self.host.input_devices().unwrap().nth(position as usize).unwrap()).into())
        }
    }
}
