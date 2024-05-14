use std::cell::RefCell;
use std::rc::Rc;
use itertools::Itertools;
use cpal::{ChannelCount, InputCallbackInfo, SampleRate, SizedSample, Stream, StreamConfig};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gtk::{
    glib,
    glib::*,
    glib::property::*,
    subclass::prelude::*,
    gio::ListModel,
    prelude::*,
};
use ringbuf::{HeapRb, HeapProd, HeapCons, traits::{Producer, Split}};
use crate::fourier::StereoMagnitude;
use crate::devices::audio_device::AudioDevice;
use std::sync::{Arc, Mutex};


glib::wrapper! {
    pub struct AudioInputListModel(ObjectSubclass<imp::AudioInputListModel>)
        @implements ListModel;
}

impl AudioInputListModel {
    pub fn new() -> (AudioInputListModel, HeapCons<StereoMagnitude>) {
        let object = Object::builder().build();
        let imp = imp::AudioInputListModel::from_obj(&object);

        let (sender, receiver) = HeapRb::new(4096).split();
        imp.sender.set(sender);
        (object, receiver)
    }

    pub fn select(&self, device_index: u32) {
        let imp = imp::AudioInputListModel::from_obj(self);
        let mut stream = imp.stream.lock().unwrap();
        let mut config = imp.config.lock().unwrap();

        // If there's an existing stream, close it
        stream.take().map(|s: Stream| {
            s.pause().expect("Failed to stop a running stream")
        });

        // Set up a stream config and report which device was selected
        let device = imp.item(device_index).unwrap()
            .dynamic_cast_ref::<AudioDevice>().unwrap()
            .get_device();
        let c = device.as_ref().default_input_config().unwrap().config();
        *config = device.as_ref().default_input_config().unwrap().config().into();
        let channels = config.as_ref().unwrap().channels;
        let sample_rate = config.as_ref().unwrap().sample_rate;
        imp.sample_rate.replace(sample_rate.0);
        self.notify_sample_rate();
        println!(
            "Listening to device: {} ({}Hz, {}ch)",
            device.name().unwrap(),
            sample_rate.0,
            channels
        );

        // Create an input stream with the selected device
        let sender = Arc::clone(&imp.sender);
        *stream = device.build_input_stream(
            config.as_ref().unwrap(),
            move |data: &[f32], _| {
                if channels == 1 {
                    let mut mono_expanded = data.iter().map(|s| StereoMagnitude::new(*s, *s));
                    sender.lock().unwrap().push_iter(&mut mono_expanded);
                } else if channels == 2 {
                    let mut stereo_expanded = data.iter().tuples().map(|(l, r)| StereoMagnitude::new(*l, *r));
                    sender.lock().unwrap().push_iter(&mut stereo_expanded);
                } else {
                    eprintln!("{}-channel input not supported!", channels);
                }
            },
            |err| eprintln!("An error occurred on the input audio stream: {}", err),
            None,
        ).ok();

        // Start the newly created stream (usually not necessary)
        stream.as_ref().map(|s| s.play().expect("Failed to start input stream"));
    }

    pub fn current_stream(&self) -> Arc<Mutex<Option<Stream>>> {
        let imp = imp::AudioInputListModel::from_obj(self);
        Arc::clone(&imp.stream)
    }

    pub fn current_config(&self) -> Arc<Mutex<Option<StreamConfig>>> {
        let imp = imp::AudioInputListModel::from_obj(self);
        Arc::clone(&imp.config)
    }
}

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::AudioInputListModel)]
    pub struct AudioInputListModel {
        devices: Vec<Rc<cpal::Device>>,
        pub _host: cpal::Host,
        pub stream: Arc<Mutex<Option<Stream>>>,
        pub config: Arc<Mutex<Option<StreamConfig>>>,
        pub sender: Arc<Mutex<HeapProd<StereoMagnitude>>>,

        #[property(get)]
        pub sample_rate: RefCell<u32>,
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
                stream: Arc::new(None.into()),
                config: Arc::new(None.into()),
                sender: Arc::new(dummy_sender.into()),
                devices,
                sample_rate: 0.into()
            }
        }
    }

    #[glib::derived_properties]
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
