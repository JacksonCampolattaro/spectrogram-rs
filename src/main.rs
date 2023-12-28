mod spectrum_analyzer;
mod fourier;

use fourier::{FourierTransform, FrequencySample};

use spectrum_analyzer::SpectrumAnalyzer;

use gtk::prelude::*;
use gtk::{glib, ApplicationWindow};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use async_channel;

const APP_ID: &str = "nl.campolattaro.jackson.spectrogram";

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

    let (sender, receiver) = async_channel::bounded(64);

    let mut fft = FourierTransform::new(sender);

    // let mut fft_plan = R2CPlan32::aligned(
    //     &[FFT_WINDOW_SIZE],
    //     fftw::types::Flag::ESTIMATE,
    // ).unwrap();
    //
    // let mut sample_buffer = AlignedVec::new(FFT_WINDOW_SIZE);
    // sample_buffer.fill(0.0);
    // let mut frequency_buffer = AlignedVec::new(NUM_FREQUENCIES);
    //
    // let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
    //
    //     for chunk in data.chunks_exact(FFT_WINDOW_STRIDE) {
    //         sample_buffer.rotate_left(FFT_WINDOW_SIZE);
    //         sample_buffer[FFT_WINDOW_SIZE-FFT_WINDOW_STRIDE..].copy_from_slice(chunk);
    //
    //         fft_plan.r2c(&mut sample_buffer, &mut frequency_buffer).unwrap();
    //
    //         let scale = 2.0 / FFT_WINDOW_SIZE as f32;
    //         let frequency_magnitudes: Vec<_> = frequency_buffer.iter()
    //             .map(|c| c * scale)
    //             .map(|c| c.norm_sqr())
    //             .map(|v: f32| 10.0 * (v + EPSILON).log10())
    //             .collect();
    //
    //         // todo: it might be best to avoid send_blocking if possible
    //         sender.send_blocking(frequency_magnitudes).expect("Failed to send data");
    //     }
    // };

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
        while let Ok(frequency_sample) = receiver.recv().await {
            level_bars.set_frequencies(frequency_sample);
            input_stream.play().expect("Failed to start input stream");
        }
    });

    // Present window
    window.present();
}