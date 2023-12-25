use cpal::traits::{DeviceTrait, HostTrait};
use gtk::prelude::*;
use gtk::{glib, Box, LevelBar, Orientation};
use ringbuf::HeapRb;
use ringbuf_blocking::wrap::{BlockingWrap, BlockingCons};
use std::thread;

const APP_ID: &str = "nl.campolattaro.jackson.spectrogram";

fn main() -> glib::ExitCode {

    // Create a new application
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);

    let err_fn = |err| eprintln!("An error occurred on the input audio stream: {}", err);

    let host = cpal::default_host();
    let device = host.default_input_device().unwrap();
    let config = device.default_input_config().unwrap().into();

    let mut buffer: HeapRb<f32> = HeapRb::new(512 * 64);
    let (mut buffer_producer, mut buffer_consumer) = buffer.split();
    //let mut blocking_buffer_consumer = BlockingCons::new(&mut buffer_consumer.into_rb_ref()).into();

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut data_iter = data.into_iter().copied();
        buffer_producer.push_iter(&mut data_iter);
        if data_iter.next().is_some() {
            //println!("Unable to write all data to the queue; maybe the consumer is too slow");
        }
    };
    let input_stream = device.build_input_stream(
        &config,
        input_data_fn,
        err_fn,
        None,
    ).unwrap();


    let mut running_amplitude: f32 = 0.0;
    let mut stop = false;
    let consume_data_function = move || {
        while !stop {
            if buffer_consumer.len() > 128 {
                let rms = (
                    buffer_consumer.pop_iter()
                        .take(128)
                        .map(|x| { x * x })
                        .reduce(|a, b| { a + b })
                        .unwrap() / 128.0
                ).sqrt();

                running_amplitude = f32::max(running_amplitude * 0.99, rms);
                println!("{}", "*".repeat((64.0 * running_amplitude) as usize));
            }
        }
    };
    let consumer_thread = thread::spawn(consume_data_function);

    // Run the application
    let result = app.run();

    // Stop the consumer thread after the application closes
    stop = true;
    consumer_thread.join().expect("Couldn't join consumer thread");
    result
}

fn build_ui(app: &adw::Application) {
    let levelbar_box = Box::builder()
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(4)
        .margin_end(4)
        .spacing(1)
        .build();

    for _ in 0..32 {
        let levelbar = LevelBar::builder()
            .hexpand(true)
            .vexpand(true)
            .value(0.3)
            .orientation(Orientation::Vertical)
            .inverted(true)
            .build();
        levelbar_box.append(&levelbar);
    }

    // create a window and set the title
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Spectrogram")
        .default_height(100)
        .default_width(300)
        .decorated(true)
        .content(&levelbar_box)
        .build();

    // Present window
    //window.present();
}