use gtk::prelude::*;
use adw::prelude::*;
use gtk::Align;
use relm4::prelude::*;
use crate::colorscheme::{ColorScheme, default_color_schemes};

mod colorscheme;

pub type StereoMagnitude = (f32, f32);

struct App {
    counter: u8,
}

#[derive(Debug)]
enum Msg {
    Increment,
    Decrement,
}

#[relm4::component]
impl SimpleComponent for App {
    type Init = u8;
    type Input = Msg;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_title: Some("Simple app"),
            set_default_size: (750, 350),

            gtk::Overlay {
                set_hexpand: true,
                set_vexpand: true,

                #[name(overlay_revealer)]
                add_overlay = &gtk::Revealer {
                    set_vexpand: false,
                    set_valign: Align::Start,
                    set_transition_type: gtk::RevealerTransitionType::Crossfade,

                    add_controller = gtk::EventControllerMotion { connect_enter[overlay_revealer] => move |_, _, _| {
                        overlay_revealer.set_reveal_child(true);
                    }},

                    adw::HeaderBar {
                        set_vexpand: false,
                        set_valign: Align::Start,
                        set_css_classes: &["flat", "osd"],

                        pack_end = &gtk::DropDown {
                            #[wrap(Some)]
                            set_model = &default_color_schemes(),
                            #[wrap(Some)]
                            set_expression = gtk::PropertyExpression::new(
                                ColorScheme::static_type(),
                                None::<&gtk::Expression>,
                                "name"
                            ),
                        },

                        // todo: input selection

                    }
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 5,
                    set_margin_all: 5,

                    add_controller = gtk::EventControllerMotion { connect_enter[overlay_revealer] => move |_, _, _| {
                        overlay_revealer.set_reveal_child(false);
                    }},

                    gtk::Button {
                        set_label: "Increment",
                        connect_clicked => Msg::Increment,
                    },

                    gtk::Button {
                        set_label: "Decrement",
                        connect_clicked => Msg::Decrement,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: &format!("Counter: {}", model.counter),
                        set_margin_all: 5,
                    },

                },
            }

        }
    }

    // Initialize the component.
    fn init(
        counter: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = App { counter };

        // Insert the code generation of the view! macro here
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Msg::Increment => {
                self.counter = self.counter.wrapping_add(1);
            }
            Msg::Decrement => {
                self.counter = self.counter.wrapping_sub(1);
            }
        }
    }
}

fn main() {
    let app = RelmApp::new("relm4.example.simple");
    app.run::<App>(0);
}