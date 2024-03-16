use std::cell::RefCell;
use std::rc::Rc;
use itertools::Itertools;
use cpal::{ChannelCount, InputCallbackInfo, SampleRate, SizedSample, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gtk::{
    glib,
    glib::*,
    subclass::prelude::*,
    gio::ListModel,
};
use ringbuf::{HeapRb, HeapProducer, HeapConsumer};
use crate::fourier::StereoMagnitude;
use crate::devices::audio_device::AudioDevice;
use std::sync::{Arc, Mutex};


glib::wrapper! {
    pub struct AudioInputListModel(ObjectSubclass<imp::AudioInputListModel>)
        @implements ListModel;
}

impl AudioInputListModel {
    pub fn new() -> (AudioInputListModel, HeapConsumer<StereoMagnitude>) {
        let object = Object::builder().build();
        let imp = imp::AudioInputListModel::from_obj(&object);

        let (sender, receiver) = HeapRb::new(4096).split();
        imp._sender.set(sender);
        (object, receiver)
    }

    pub fn select(&self, device_index: u32) {
        let imp = imp::AudioInputListModel::from_obj(self);

        // If there's an existing stream, close it
        imp.stream.take().map(|s: cpal::Stream| {
            s.pause().expect("Failed to stop a running stream")
        });

        // Set up a stream config and report which device was selected
        let device = imp.item(device_index).unwrap()
            .dynamic_cast_ref::<AudioDevice>().unwrap()
            .get_device();
        let config: cpal::StreamConfig = device.as_ref().default_input_config().unwrap().into();
        println!(
            "Listening to device: {} ({}Hz, {}ch)",
            device.name().unwrap(),
            config.sample_rate.0,
            config.channels
        );

        // Create an input stream with the selected device
        let sender = Arc::clone(&imp._sender);
        imp.stream.replace(device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                //println!("Received {} samples", data.len());
                if config.channels == 1 {
                    let mut mono_expanded = data.iter().map(|s| StereoMagnitude::new(*s, *s));
                    sender.lock().unwrap().push_iter(&mut mono_expanded);
                } else if config.channels == 2 {
                    let mut stereo_expanded = data.iter().tuples().map(|(l, r)| StereoMagnitude::new(*l, *r));
                    sender.lock().unwrap().push_iter(&mut stereo_expanded);
                } else {
                    eprintln!("{}-channel input not supported!", config.channels);
                }
            },
            |err| eprintln!("An error occurred on the input audio stream: {}", err),
            None,
        ).ok());

        // Start the newly created stream (usually not necessary)
        imp.stream.borrow().as_ref().map(|s| s.play().expect("Failed to start input stream"));
    }
}

mod imp {
    use super::*;

    pub struct AudioInputListModel {
        pub _host: cpal::Host,
        pub stream: RefCell<Option<Stream>>,
        pub _sender: Arc<Mutex<HeapProducer<StereoMagnitude>>>,
        devices: Vec<Rc<cpal::Device>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioInputListModel {
        const NAME: &'static str = "AudioInputListModel";
        type Type = super::AudioInputListModel;
        type Interfaces = (ListModel, );

        fn new() -> Self {
            let _host = cpal::default_host();
            let default_device_name = _host.default_input_device().unwrap().name().unwrap();
            let devices = _host.input_devices().unwrap()
                .sorted_by_cached_key(|d| { return d.name().unwrap() != default_device_name; })
                .map(Rc::from)
                .collect();
            let (dummy_sender, _) = HeapRb::new(1).split();
            Self {
                _host,
                stream: None.into(),
                _sender: Arc::new(dummy_sender.into()),
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
        }
    }
}
