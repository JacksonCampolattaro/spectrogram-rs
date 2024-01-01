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
    use cpal::default_host;
    use cpal::traits::{DeviceTrait, HostTrait};
    use itertools::Itertools;
    use super::*;

    pub struct AudioInputListModel {
        host: cpal::Host,
        devices: Vec<cpal::Device>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioInputListModel {
        const NAME: &'static str = "AudioInputListModel";
        type Type = super::AudioInputListModel;
        type Interfaces = (ListModel, );

        fn new() -> Self {
            let host = cpal::default_host();
            let devices = default_host().input_devices().unwrap()
                //.map(|device| { AudioDevice::from(device) })
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
                .map(|device| { AudioDevice::from(device).into() })
            // todo: this could be cleaned up
            //Some(AudioDevice::from(self.host.input_devices().unwrap().nth(position as usize).unwrap()).into())
        }
    }
}
