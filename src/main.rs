mod spectrum_analyzer;
mod fourier;
mod spectrogram;
mod log_scaling;
mod audio_input_list_model;
mod audio_device;

use std::borrow::Borrow;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use adw::{ColorScheme, gio};
use adw::glib::translate::{IntoGlibPtr, Stash, ToGlibPtr, UnsafeFrom};
use adw::glib::value::{FromValue, FromValueOptional, ToValueOptional, ValueType};
use adw::prelude::AdwApplicationExt;
use fourier::FourierTransform;

use spectrum_analyzer::SpectrumAnalyzer;
use spectrogram::Spectrogram;

use gtk::prelude::*;
use gtk::glib;
use gtk::{ApplicationWindow, Align};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use async_channel;
use crate::audio_device::AudioDevice;
use crate::audio_input_list_model::AudioInputListModel;

const APP_ID: &str = "nl.campolattaro.jackson.spectrogram";

fn main() -> glib::ExitCode {

    // Create a new application
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.style_manager().set_color_scheme(ColorScheme::PreferDark);
    app.connect_activate(build_ui);

    // Run the application
    app.run()
}

fn build_ui(app: &adw::Application) {
    let (sender, receiver) = async_channel::unbounded();

    let mut fft = FourierTransform::new(sender);

    let err_fn = |err| eprintln!("An error occurred on the input audio stream: {}", err);
    let host = cpal::default_host();
    let device = host.default_input_device().unwrap();
    let config = device.default_input_config().unwrap().into();
    let input_stream = device.build_input_stream(
        &config,
        move |data, _| { fft.apply(data, config.sample_rate); },
        err_fn,
        None,
    ).unwrap();
    input_stream.play().expect("Failed to start input stream");
    println!("Using device: {}", device.name().unwrap());

    let input_list = AudioInputListModel::new();

    let input_dropdown = gtk::DropDown::builder()
        .css_classes(["flat"])
        .model(&input_list)
        .expression(gtk::PropertyExpression::new(
            AudioDevice::static_type(),
            None::<&gtk::Expression>,
            "name",
        ))
        .build();


    let mut toolbar = gtk::Box::builder()
        .margin_start(8)
        .margin_end(8)
        .margin_top(8)
        .margin_bottom(8)
        .hexpand(false)
        .vexpand(false)
        .halign(Align::End)
        .valign(Align::Start)
        .css_classes(["osd", "toolbar"])
        .build();
    toolbar.append(&input_dropdown);

    // Only show the toolbar when you hover over it
    let hover_event_controller = gtk::EventControllerMotion::builder().build();
    hover_event_controller.bind_property("contains-pointer", &toolbar, "opacity")
        .transform_to(|b, v| { if v { Some(1.0) } else { Some(0.0) } })
        .sync_create()
        .build();
    toolbar.add_controller(hover_event_controller);

    // create a window and set the title
    let mut visualizer = Spectrogram::new();
    let mut overlay = gtk::Overlay::builder()
        .child(&visualizer)
        .build();
    overlay.add_overlay(&toolbar);
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spectrogram")
        .default_height(300)
        .default_width(600)
        .decorated(true)
        .child(&overlay)
        .build();

    glib::spawn_future_local(async move {
        // Wait for the next sample to arrive
        while let Ok(frequency_sample) = receiver.recv().await {
            let mut samples = vec![frequency_sample];

            // Consume any extra values in the pipeline
            while let Ok(frequency_sample) = receiver.try_recv() {
                samples.push(frequency_sample);
            }
            // Push the entire block at once
            visualizer.push_frequency_block(&samples);

            // Make sure the input stream continues
            input_stream.play().expect("Failed to start input stream");
        }
    });

    // Present window
    window.present();
}