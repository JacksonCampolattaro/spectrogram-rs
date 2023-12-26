mod spectrum_analyzer;

use std::future::IntoFuture;
use spectrum_analyzer::SpectrumAnalyzer;

use gtk::prelude::*;
use gtk::{glib, ApplicationWindow};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use fftw::plan::{R2CPlan, R2CPlan32};
use fftw::array::AlignedVec;
use gtk::glib::clone;

const APP_ID: &str = "nl.campolattaro.jackson.spectrogram";
const EPSILON: f32 = 1e-7;
const FFT_WINDOW_SIZE: usize = 512;
const NUM_FREQUENCIES: usize = 1 + (FFT_WINDOW_SIZE / 2);

fn main() -> glib::ExitCode {

    // Create a new application
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);

    // Run the application
    let result = app.run();

    // Stop the consumer thread after the application closes
    // stop = true;
    // consumer_thread.join().expect("Couldn't join consumer thread");
    result
}

fn build_ui(app: &adw::Application) {
    let mut level_bars = SpectrumAnalyzer::new();

    let (sender, receiver) = async_channel::bounded(128);

    let mut fft_plan = R2CPlan32::aligned(
        &[FFT_WINDOW_SIZE],
        fftw::types::Flag::ESTIMATE,
    ).unwrap();

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let scale = 2.0 / FFT_WINDOW_SIZE as f32;
        let mut sample_buffer = AlignedVec::new(FFT_WINDOW_SIZE);
        for (ret, src) in sample_buffer.iter_mut().zip(data.iter().take(FFT_WINDOW_SIZE).copied()) {
            *ret = src;
        }
        let mut frequency_buffer = AlignedVec::new(NUM_FREQUENCIES);

        fft_plan.r2c(&mut sample_buffer, &mut frequency_buffer).unwrap();

        let frequency_magnitudes: Vec<_> = frequency_buffer.iter()
            .map(|c| c * scale)
            .map(|c| c.norm_sqr())
            .map(|v: f32| 10.0 * (v + EPSILON).log10())
            .collect();

        sender.send_blocking(frequency_magnitudes);
    };


    let err_fn = |err| eprintln!("An error occurred on the input audio stream: {}", err);
    let host = cpal::default_host();
    let device = host.default_input_device().unwrap();
    let config = device.default_input_config().unwrap().into();
    let input_stream = device.build_input_stream(
        &config,
        input_data_fn,
        err_fn,
        None,
    ).unwrap();
    input_stream.play().expect("Failed to start input stream");

    // create a window and set the title
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spectrogram")
        .default_height(100)
        .default_width(300)
        .decorated(true)
        .child(&level_bars)
        .build();

    glib::spawn_future_local(async move {
        while let Ok(frequency_magnitudes) = receiver.recv().await {
            level_bars.set_frequencies(frequency_magnitudes.as_slice());
            input_stream.play().expect("Failed to start input stream");
        }
    });

    // Present window
    window.present();
}