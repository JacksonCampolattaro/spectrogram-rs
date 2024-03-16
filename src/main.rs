use std::sync::{Arc, Mutex};

use adw::ColorScheme;
use adw::glib::clone;
use adw::glib::ControlFlow::Continue;
use adw::prelude::AdwApplicationExt;
use async_channel;
use cpal::{ChannelCount, SampleRate};
use cpal::traits::{DeviceTrait, StreamTrait};
use gtk::{DropDown, glib, Align, RevealerTransitionType, Overlay};
use gtk::prelude::*;
use itertools::Itertools;

use fourier::transform::*;
use widgets::spectrogram::Spectrogram;
use devices::audio_device::AudioDevice;
use devices::audio_input_list_model::AudioInputListModel;

use crate::colorscheme::*;
use crate::fourier::interpolated_frequency_sample::InterpolatedFrequencySample;
use crate::fourier::StereoMagnitude;

mod fourier;
mod widgets;
mod devices;

mod log_scaling;
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

    // Set up an input list with its associated stream
    let (input_list, sample_receiver) = AudioInputListModel::new();

    // Set up an FFT to process samples coming from the input list
    // todo: this could be built into the visualizer
    let (mut fft, frequency_receiver) = StreamTransform::new(sample_receiver, input_list.current_config());

    // Create a visualizer for the data coming from the FFT
    let visualizer = Spectrogram::new();


    // Use a dropdown to select inputs
    let input_dropdown = DropDown::builder()
        .model(&input_list)
        .expression(gtk::PropertyExpression::new(
            AudioDevice::static_type(),
            None::<&gtk::Expression>,
            "name",
        ))
        .build();
    input_dropdown.connect_selected_item_notify(move |dropdown: &DropDown| {
        input_list.select(dropdown.selected());
    });
    input_dropdown.notify("selected-item");

    // Use a dropdown to select color schemes
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

    let toolbar = adw::HeaderBar::builder()
        .vexpand(false)
        .valign(Align::Start)
        .css_classes(["osd"])
        .build();
    toolbar.pack_end(&input_dropdown);
    toolbar.pack_end(&colorscheme_dropdown);

    // Only show the toolbar when you hover over it
    let revealer = gtk::Revealer::builder()
        .child(&toolbar)
        .transition_type(RevealerTransitionType::Crossfade)
        .vexpand(false)
        .valign(Align::Start)
        .build();
    // Show the toolbar when you hover over it
    let toolbar_hover_controller = gtk::EventControllerMotion::builder().build();
    toolbar_hover_controller.connect_enter(clone!(@weak revealer => move |_, _, _| {
        revealer.set_reveal_child(true);
    }));
    revealer.add_controller(toolbar_hover_controller);
    // Hide the toolbar when you hover over the visualizer
    let visualizer_hover_controller = gtk::EventControllerMotion::builder().build();
    visualizer_hover_controller.connect_enter(clone!(@weak revealer => move |_, _, _| {
        revealer.set_reveal_child(false);
    }));
    visualizer.add_controller(visualizer_hover_controller);

    // Use an overlay so the toolbar can overlap the content
    let overlay = Overlay::builder()
        .child(&visualizer)
        .build();
    overlay.add_overlay(&revealer);

    // create a window and set the title
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Spectrogram")
        .default_height(300)
        .default_width(600)
        .decorated(true)
        .content(&overlay)
        .build();

    visualizer.add_tick_callback(move |visualizer, _| {

        // Process the current backlog of samples
        fft.process();

        // Render any new fft results that were created
        // todo: Copying to a buffer shouldn't be necessary here
        let mut samples = Vec::new();
        // Consume any values in the pipeline
        while let Ok(frequency_sample) = frequency_receiver.try_recv() {
            samples.push(frequency_sample);
        }
        // Push the entire block at once
        visualizer.push_frequency_samples(samples);
        Continue
    });

    // Present window
    window.present();
}