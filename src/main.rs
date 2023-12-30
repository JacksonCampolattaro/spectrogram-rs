mod spectrum_analyzer;
mod fourier;
mod spectrogram;
mod log_scaling;

use adw::ColorScheme;
use adw::prelude::AdwApplicationExt;
use fourier::{FourierTransform, FrequencySample};

use spectrum_analyzer::SpectrumAnalyzer;

use gtk::prelude::*;
use gtk::{glib, ApplicationWindow};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use async_channel;
use crate::spectrogram::Spectrogram;

const APP_ID: &str = "nl.campolattaro.jackson.spectrogram";

fn main() -> glib::ExitCode {

    // Create a new application
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.style_manager().set_color_scheme(ColorScheme::PreferDark);
    app.connect_activate(build_ui);

    // Run the application
    let result = app.run();

    // Stop the consumer thread after the application closes
    // stop = true;
    // consumer_thread.join().expect("Couldn't join consumer thread");
    result
}

fn build_ui(app: &adw::Application) {

    let (sender, receiver) = async_channel::bounded(64);

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

    // create a window and set the title
    let mut visualizer = Spectrogram::new();
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spectrogram")
        .default_height(300)
        .default_width(600)
        .decorated(true)
        .child(&visualizer)
        .build();

    glib::spawn_future_local(async move {
        while let Ok(frequency_sample) = receiver.recv().await {
            visualizer.push_frequencies(frequency_sample);
            input_stream.play().expect("Failed to start input stream");
        }
    });

    // Present window
    window.present();
}