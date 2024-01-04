use std::sync::Mutex;

use adw::ColorScheme;
use adw::glib::ControlFlow::Continue;
use adw::prelude::AdwApplicationExt;
use async_channel;
use cpal::traits::{DeviceTrait, StreamTrait};
use gtk::{
    DropDown,
    glib,
    Align,
    ApplicationWindow,
    // todo: this will be useful when I want the UI to match the current color scheme
    // style_context_add_provider_for_display
};
use gtk::prelude::*;

use fourier::FourierTransform;
use spectrogram::Spectrogram;

use crate::audio_device::AudioDevice;
use crate::audio_input_list_model::AudioInputListModel;
use crate::colorscheme::*;

mod spectrum_analyzer;
mod fourier;
mod spectrogram;
mod log_scaling;
mod audio_input_list_model;
mod audio_device;
mod colorscheme;

const APP_ID: &str = "nl.campolattaro.jackson.spectrogram";

fn main() -> glib::ExitCode {

    // Create a new application
    let app = adw::Application::builder().application_id(APP_ID).build();

    // Configuring styling
    app.style_manager().set_color_scheme(ColorScheme::PreferDark);

    // Setup fft & UI on startup
    app.connect_activate(build_ui);

    // Run the application
    app.run()
}

fn build_ui(app: &adw::Application) {
    let (sender, receiver) = async_channel::unbounded();
    let err_fn = |err| eprintln!("An error occurred on the input audio stream: {}", err);

    let stream = Mutex::new(None::<cpal::Stream>);
    let start_stream = move |device: &cpal::Device| {
        let config: cpal::StreamConfig = device.default_input_config().unwrap().into();
        let mut fft = FourierTransform::new(sender.clone(), config.channels as usize);
        println!(
            "Listening to device: {} ({}Hz, {}ch)",
            device.name().unwrap(),
            config.sample_rate.0,
            config.channels
        );
        // Attempt to stop any existing stream
        stream.lock().unwrap().as_ref().map(|stream| {
            stream.pause().expect("Failed to stop existing stream")
        });

        // Start a new stream with the chosen device
        stream.lock().unwrap().replace(device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                // todo: deinterleave data
                let deinterleaved = ndarray::Array::from_iter(data.iter().copied())
                    .into_shape((data.len() / config.channels as usize, config.channels as usize))
                    .expect("Failed to deinterleave stream").into();
                fft.apply(&deinterleaved, config.sample_rate);
            },
            err_fn,
            None,
        ).unwrap());

        // Start the newly created stream
        stream.lock().unwrap().as_ref().map(|stream| {
            stream.play().expect("Failed to start input stream");
        });
    };

    let visualizer = Spectrogram::new();

    let input_list = AudioInputListModel::new();
    let input_dropdown = DropDown::builder()
        .model(&input_list)
        .expression(gtk::PropertyExpression::new(
            AudioDevice::static_type(),
            None::<&gtk::Expression>,
            "name",
        ))
        .build();
    input_dropdown.connect_selected_item_notify(move |dropdown: &DropDown| {
        let binding = dropdown.selected_item().unwrap();
        let device = binding.dynamic_cast_ref::<AudioDevice>().unwrap();
        start_stream(device.get_device().as_ref());
    });
    input_dropdown.notify("selected-item");

    let colorscheme_list = default_color_schemes();
    let colorscheme_dropdown = DropDown::builder()
        .model(&colorscheme_list)
        .expression(gtk::PropertyExpression::new(
            AudioDevice::static_type(),
            None::<&gtk::Expression>,
            "name",
        ))
        .build();
    colorscheme_dropdown.bind_property("selected_item", &visualizer, "palette")
        .sync_create()
        .build();

    let toolbar = gtk::Box::builder()
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
    toolbar.append(&colorscheme_dropdown);

    // Only show the toolbar when you hover over it
    let hover_event_controller = gtk::EventControllerMotion::builder().build();
    hover_event_controller.bind_property("contains-pointer", &toolbar, "opacity")
        .transform_to(|_, v| { if v { Some(1.0) } else { Some(0.0) } })
        .sync_create()
        .build();
    toolbar.add_controller(hover_event_controller);

    let overlay = gtk::Overlay::builder()
        .child(&visualizer)
        .build();
    overlay.add_overlay(&toolbar);

    // create a window and set the title
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spectrogram")
        .default_height(300)
        .default_width(600)
        .decorated(true)
        .child(&overlay)
        .build();

    visualizer.add_tick_callback(move |visualizer, _| {
        let mut samples = Vec::new();

        // Consume any values in the pipeline
        while let Ok(frequency_sample) = receiver.try_recv() {
            samples.push(frequency_sample);
        }

        // Push the entire block at once
        visualizer.push_frequency_block(&samples);
        Continue
    });

    // Present window
    window.present();
}