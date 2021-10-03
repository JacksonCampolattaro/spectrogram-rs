// Thanks to [Helvum](https://gitlab.freedesktop.org/ryuukyu/helvum)
// For a good example of combining Gtk and Pipewire!

mod ui;
mod listener;

use std::io::{stdout, Write};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, glib::{self, PRIORITY_DEFAULT}, Button, Orientation, ComboBoxText, ComboBox};
use pipewire::{
    link::{Link, LinkChangeMask, LinkListener, LinkState},
    prelude::*,
    properties,
    registry::{GlobalObject, Registry},
    spa::{Direction, ForeignDict},
    types::ObjectType,
    Context, Core, MainLoop,
};
use pipewire::keys::MEDIA_TYPE;

use std::thread;

fn main() {

    // Create a new application
    let app = Application::builder()
        .application_id("org.gtk.example")
        .build();

    // Connect to "activate" signal of `app`
    app.connect_activate(move |app| ui::setup(app));

    // Run the application
    app.run();
}

