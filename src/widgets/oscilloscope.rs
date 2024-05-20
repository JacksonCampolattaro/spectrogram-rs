use std::any::Any;
use adw::glib::ControlFlow::Continue;
use adw::glib::Object;
use adw::subclass::prelude::ObjectSubclassExt;
use gtk::{gdk, glib, prelude::*};
use ringbuf::{HeapCons, traits::Split};
use crate::fourier::StereoMagnitude;
use crate::widgets::glarea_backend::GLAreaBackend;
use std::{cell::RefCell, rc::Rc};
use std::cell::Cell;
use std::iter::zip;
use std::thread::sleep;
use std::time::{Duration, Instant};
use adw::glib::Properties;
use cpal::SampleRate;

glib::wrapper! {
    pub struct Oscilloscope(ObjectSubclass<imp::Oscilloscope>)
        @extends gtk::GLArea, gtk::Widget;
}

const MAX_SAMPLES_PER_FRAME: usize = 1024;

impl Oscilloscope {
    pub fn new(sample_stream: HeapCons<StereoMagnitude>) -> Oscilloscope {
        let object = Object::builder().build();
        let imp = imp::Oscilloscope::from_obj(&object);
        imp.input_stream.replace(sample_stream);
        object.add_tick_callback(|oscilloscope, _| {
            oscilloscope.queue_draw();
            Continue
        });
        object
    }
}

mod imp {
    use super::*;

    use glium::{implement_vertex, index::PrimitiveType, program, uniform, Frame, Surface, VertexBuffer, uniforms::UniformBuffer, implement_uniform_block, BlendingFunction, Blend};
    use glium::Smooth::Nicest;
    use gtk::{glib, prelude::*, subclass::prelude::*};
    use itertools::Itertools;
    use num_traits::pow;
    use ringbuf::{HeapCons, HeapRb};
    use ringbuf::traits::Observer;
    use ringbuf_blocking::traits::Consumer;
    use crate::colorscheme::ColorScheme;
    use crate::fourier::fft::FastFourierTransform;
    use crate::fourier::{Frequency, StereoMagnitude};
    use crate::widgets::oscilloscope::{MAX_SAMPLES_PER_FRAME};

    #[derive(Copy, Clone)]
    struct Vertex {
        magnitude: f32,
    }
    implement_vertex!(Vertex, magnitude);

    #[derive(Properties)]
    #[properties(wrapper_type = super::Oscilloscope)]
    pub struct Oscilloscope {
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        pub input_stream: RefCell<HeapCons<StereoMagnitude>>,

        #[property(get, set)]
        pub palette: RefCell<ColorScheme>,

        context: RefCell<Option<Rc<glium::backend::Context>>>,
        program: RefCell<Option<glium::Program>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Oscilloscope {
        const NAME: &'static str = "Oscilloscope";
        type Type = super::Oscilloscope;
        type ParentType = gtk::GLArea;

        fn new() -> Self {
            let (_, dummy_sample_stream) = HeapRb::new(1).split();
            Self {
                input_stream: dummy_sample_stream.into(),
                palette: ColorScheme::new_mono(colorous::MAGMA, "magma").into(),
                context: None.into(),
                program: None.into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Oscilloscope {}

    impl WidgetImpl for Oscilloscope {
        fn realize(&self) {
            self.obj().set_required_version(3, 2);

            self.parent_realize();

            let widget = self.obj();
            if widget.error().is_some() {
                return;
            }

            // SAFETY: we know the GdkGLContext exists as we checked for errors above, and
            // we haven't done any operations on it which could lead to glium's
            // state mismatch. (In theory, GTK doesn't do any state-breaking
            // operations on the context either.)
            //
            // We will also ensure glium's context does not outlive the GdkGLContext by
            // destroying it in `unrealize()`.
            let context = unsafe {
                let start = Instant::now();
                let backend = GLAreaBackend::from(widget.clone().upcast::<gtk::GLArea>());
                println!("backend: {:?}", start.elapsed());
                let context = glium::backend::Context::new(backend, true, Default::default());
                println!("total: {:?}", start.elapsed());
                context
            }.unwrap();

            let program = program!(
                &context,
                150 => {
                    vertex: "
                        #version 150
                        uniform vec3 color;
                        uniform uint num_samples;
                        in float magnitude;
                        void main() {
                            float x = 2 * (float(gl_VertexID) / float(num_samples)) - 1;
                            gl_Position = vec4(x, magnitude, 0.0, 1.0);
                        }
                    ",
                    fragment: "
                        #version 150
                        uniform vec3 color;
                        out vec4 f_color;
                        void main() {
                            f_color = vec4(color, 1.0);
                        }
                    "
                },
            ).unwrap();

            self.context.replace(Some(context));
            self.program.replace(Some(program));
        }

        fn unrealize(&self) {
            self.context.replace(None);
            self.program.replace(None);

            self.parent_unrealize();
        }
    }

    impl GLAreaImpl for Oscilloscope {
        fn render(&self, _context: &gtk::gdk::GLContext) -> glib::Propagation {
            let num_samples = self.input_stream.borrow().occupied_len().min(MAX_SAMPLES_PER_FRAME);
            if num_samples < MAX_SAMPLES_PER_FRAME * 75 / 100 {
                return glib::Propagation::Proceed;
            };

            let context_binding = self.context.borrow();
            let context = context_binding.as_ref().unwrap();
            let program_binding = self.program.borrow();
            let program = program_binding.as_ref().unwrap();
            let palette = self.palette.borrow();
            let (left_color, _) = palette.color_for(StereoMagnitude::new(1.0, 0.0));
            let (right_color, _) = palette.color_for(StereoMagnitude::new(0.0, 1.0));
            let bg_color = palette.background();

            let mut frame = Frame::new(
                context.clone(),
                context.get_framebuffer_dimensions(),
            );
            let (left, right): (Vec<Vertex>, Vec<Vertex>) = self.input_stream.borrow_mut().pop_iter()
                .take(num_samples)
                .map(|s| { (Vertex { magnitude: s.re }, Vertex { magnitude: s.im }) })
                .unzip();

            let params = glium::DrawParameters {
                line_width: 2.0.into(), // todo: not supported on M1 mac?
                smooth: Nicest.into(),
                blend: Blend {
                    color: BlendingFunction::Addition {
                        source: glium::LinearBlendingFactor::One,
                        destination: glium::LinearBlendingFactor::One,
                    },
                    ..Default::default()
                },
                ..Default::default()
            };

            frame.clear_color(bg_color.r as f32 / 255.0, bg_color.g as f32 / 255.0, bg_color.b as f32 / 255.0, 1.);
            frame.draw(
                &VertexBuffer::new(context, left.as_slice()).unwrap(),
                &glium::index::NoIndices(PrimitiveType::LineStrip),
                program,
                &uniform! {
                    color: [left_color.r as f32 / 255.0, left_color.g as f32 / 255.0, left_color.b as f32 / 255.0],
                    num_samples: num_samples as u32 - 1
                },
                &params,
            ).unwrap();
            frame.draw(
                &VertexBuffer::new(context, right.as_slice()).unwrap(),
                &glium::index::NoIndices(PrimitiveType::LineStrip),
                &program,
                &uniform! {
                    color: [right_color.r as f32 / 255.0, right_color.g as f32 / 255.0, right_color.b as f32 / 255.0],
                    num_samples: num_samples as u32 - 1
                },
                &params,
            ).unwrap();

            frame.finish().unwrap();
            glib::Propagation::Proceed
        }
    }

    impl Oscilloscope {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            // This visualizer doesn't actually depend on the sample rate
        }
    }
}