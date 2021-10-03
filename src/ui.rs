use std::io::{stdout, Write};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, glib::{self, PRIORITY_DEFAULT}, Button, Orientation, ComboBoxText, ComboBox};

use crate::listener;

#[derive(Debug, Clone)]
pub enum Message {
    NodeSelected {
        id: u32,
    },
}

pub fn setup(app: &Application) {

    // Thread-safe channel for signalling from audio listener to UI
    let (gtk_sender, gtk_receiver) = glib::MainContext::channel(PRIORITY_DEFAULT);

    // Thread-safe channel for signalling from UI to audio listener
    let (pw_sender, pw_receiver) = pipewire::channel::channel();

    // Start the audio listener on its own thread
    let pipewire_thread = std::thread::spawn(|| listener::pipewire_main(gtk_sender, pw_receiver));

    // Create a window and set the title
    let window = ApplicationWindow::builder()
        .application(app)
        .title("spectrogram-rs")
        .build();

    let gtk_box = gtk::Box::new(Orientation::Vertical, 0);
    window.set_child(Some(&gtk_box));

    let list_store = gtk::ListStore::new(&[glib::types::Type::STRING, glib::types::Type::U32]);

    let combo_box =
        ComboBox::builder().name("Audio Device").entry_text_column(0).model(&list_store).build();
    gtk_box.append(&combo_box);

    // Create a cell to render this value
    let cell = gtk::CellRendererText::new();
    combo_box.pack_end(&cell, true);
    combo_box.add_attribute(&cell, "text", 0);

    // When the user chooses an audio device, the listener should be notified
    combo_box.connect_active_notify(move |combo_box| match combo_box.active_iter() {
        Some(iter) => {

            // Determine what ID is associated with the active selection
            let id = combo_box.model().expect("Missing tree model")
                .get(&iter, 1).get::<u32>().expect("Missing ID in tree model");

            // Notify the listener of the new choice
            pw_sender.send(Message::NodeSelected { id }).expect("Failed to send message to listener");
        }
        _ => {}
    });

    // When the listener discovers another audio device, it should be added as an option
    gtk_receiver.attach(None, move |message| match message {
        listener::Message::NodeAdded { name, id } => {

            // Add a new value to the list
            list_store.insert_with_values(None, &[(0, &name), (1, &id)]);
            combo_box.set_active(Some(0));

            Continue(true)
        }
        listener::Message::NodeRemoved { id } => {
            let mut iter = list_store.iter_first().expect("List store is missing an iterator");
            while list_store.iter_is_valid(&iter) {
                let current_id = list_store.get(&iter, 1)
                    .get::<u32>().expect("Missing ID in tree model");
                if id == current_id {
                    list_store.remove(&iter);
                    break;
                }

                list_store.iter_next(&iter);
            }
            Continue(true)
        }
    });

    // Present window to the user
    window.present();
}
