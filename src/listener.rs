use std::io::{stdout, Write};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, glib::{self, PRIORITY_DEFAULT}, Button, Orientation};
use pipewire::{
    link::{Link, LinkChangeMask, LinkListener, LinkState},
    prelude::*,
    properties,
    registry::{GlobalObject, Registry},
    spa::{Direction, ForeignDict},
    types::ObjectType,
    Context, Core, MainLoop,
};

use crate::ui;

#[derive(Debug, Clone)]
pub enum Message {
    NodeAdded {
        name: String,
        id: u32,
    },
    NodeRemoved {
        id: u32,
    },
}

pub fn pipewire_main(sender: glib::Sender<Message>, receiver: pipewire::channel::Receiver<ui::Message>) {
    let mainloop = MainLoop::new().unwrap();
    let context = Context::new(&mainloop).unwrap();
    let core = context.connect(None).unwrap();
    let registry = core.get_registry().unwrap();

    let _receiver = receiver.attach(&mainloop, |message| match message {
        ui::Message::NodeSelected { id } => {
            println!("{}", id);
        }
    });

    let _listener = registry
        .add_listener_local()
        .global(glib::clone!(@strong sender => move |global| match global.type_ {
            ObjectType::Node => handle_node(global, &sender),
            _ => {}
        }))
        .global_remove(glib::clone!(@strong sender => move |id| handle_remove(id, &sender)))
        .register();

    mainloop.run();
}

fn handle_remove(id: u32, sender: &glib::Sender<Message>) {
    sender.send(Message::NodeRemoved { id }).expect("Failed to send message to ui");
}

fn handle_node(node: &GlobalObject<ForeignDict>, sender: &glib::Sender<Message>) {
    let props = node
        .props
        .as_ref()
        .expect("Node object is missing properties");

    let name = String::from(
        props
            .get("node.nick")
            .or_else(|| props.get("node.description"))
            .or_else(|| props.get("node.name"))
            .unwrap_or_default(),
    );

    let class_string = props.get("media.class").unwrap_or_default();

    if class_string.contains("Audio") {
        sender.send(Message::NodeAdded { name, id: node.id }).expect("Failed to send");
    }
}