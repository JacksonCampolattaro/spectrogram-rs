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

const BUFFER_SIZE: usize = 4096 * 2;
const MIN_SAMPLES_PER_FRAME: usize = 128;

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
    use std::cmp::min;
    use std::ops::Deref;
    use super::*;

    use glium::{implement_vertex, index::PrimitiveType, program, uniform, Frame, Surface, VertexBuffer, uniforms::UniformBuffer, implement_uniform_block, BlendingFunction, Blend};
    use glium::Smooth::{Fastest, Nicest};
    use gtk::{glib, prelude::*, subclass::prelude::*};
    use itertools::Itertools;
    use num_traits::pow;
    use ringbuf::{HeapCons, HeapRb};
    use ringbuf::traits::Observer;
    use ringbuf_blocking::traits::Consumer;
    use crate::colorscheme::ColorScheme;
    use crate::fourier::fft::FastFourierTransform;
    use crate::fourier::{Frequency, StereoMagnitude};

    #[derive(Copy, Clone)]
    struct Vertex {
        magnitude: [f32; 2],
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
        buffer: RefCell<Option<VertexBuffer<Vertex>>>,
        ring_index: RefCell<usize>,
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
                buffer: None.into(),
                ring_index: 0.into(),
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
                let backend = GLAreaBackend::from(widget.clone().upcast::<gtk::GLArea>());
                let context = glium::backend::Context::new(backend, true, Default::default());
                context
            }.unwrap();

            let program = program!(
                &context,
                150 => {
                    vertex: "
                        #version 150
                        uniform vec3 color;
                        uniform uint num_samples;
                        uniform uint ring_index;
                        uniform uint channel;
                        in vec2 magnitude;
                        void main() {
                            float x = 2 * (float(gl_VertexID) / float(num_samples)) - 1;
                            float m = magnitude[channel];
                            // if (int(ring_index) == 0)
                            //     m = 1;
                            gl_Position = vec4(x, m, 0.0, 1.0);
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

            let buffer = VertexBuffer::dynamic(
                &context,
                &vec![Vertex { magnitude: [0f32, 0f32] }; BUFFER_SIZE],
            ).unwrap();

            self.context.replace(Some(context));
            self.program.replace(Some(program));
            self.buffer.replace(Some(buffer));
        }

        fn unrealize(&self) {
            self.context.replace(None);
            self.program.replace(None);

            self.parent_unrealize();
        }
    }

    impl GLAreaImpl for Oscilloscope {
        fn render(&self, _context: &gdk::GLContext) -> glib::Propagation {

            // let start_time = std::time::Instant::now();
            // let num_samples = self.input_stream.borrow().occupied_len().min(BUFFER_SIZE);
            // if num_samples < MIN_SAMPLES_PER_FRAME {
            //     return glib::Propagation::Proceed;
            // };

            let context_binding = self.context.borrow();
            let context = context_binding.as_ref().unwrap();
            let program_binding = self.program.borrow();
            let program = program_binding.as_ref().unwrap();
            let palette = self.palette.borrow();
            let (left_color, _) = palette.color_for(StereoMagnitude::new(1.0, 0.0));
            let (right_color, _) = palette.color_for(StereoMagnitude::new(0.0, 1.0));
            let bg_color = palette.background();
            let mut buffer_binding = self.buffer.borrow_mut();
            let buffer = buffer_binding.as_mut().unwrap();

            let mut frame = Frame::new(
                context.clone(),
                context.get_framebuffer_dimensions(),
            );

            // todo: this isn't a great way of doing this
            let buffer_size = buffer.len();
            {
                let mut write_map = buffer.map_write();
                for sample in self.input_stream.borrow_mut().pop_iter() {
                    let current_index = *self.ring_index.borrow();
                    write_map.set(current_index, Vertex { magnitude: [sample.re, sample.im] });
                    *self.ring_index.borrow_mut() = (current_index + 1) % buffer_size;
                }
            }
            // while !self.input_stream.borrow().is_empty() {
            //     let current_index: usize = *self.ring_index.borrow();
            //     let remaining_space = buffer_size - current_index;
            //     let new_samples: Vec<_> = self.input_stream.borrow_mut().pop_iter()
            //         .take(remaining_space)
            //         .map(|s| { Vertex { magnitude: [s.re, s.im] } })
            //         .collect();
            //     buffer.slice(current_index..(current_index + new_samples.len())).unwrap()
            //         .write(new_samples.as_slice());
            //     *self.ring_index.borrow_mut() = (current_index + new_samples.len()) % buffer_size;
            // }

            let params = glium::DrawParameters {
                line_width: 2.0.into(),
                smooth: Fastest.into(),
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
                &*buffer,
                &glium::index::NoIndices(PrimitiveType::LineStrip),
                program,
                &uniform! {
                    color: [left_color.r as f32 / 255.0, left_color.g as f32 / 255.0, left_color.b as f32 / 255.0],
                    num_samples: buffer_size as u32 - 1,
                    ring_index: *self.ring_index.borrow() as u32,
                    channel: 0u32
                },
                &params,
            ).unwrap();
            frame.draw(
                &*buffer,
                &glium::index::NoIndices(PrimitiveType::LineStrip),
                &program,
                &uniform! {
                    color: [right_color.r as f32 / 255.0, right_color.g as f32 / 255.0, right_color.b as f32 / 255.0],
                    num_samples: buffer_size as u32 - 1,
                    ring_index: *self.ring_index.borrow() as u32,
                    channel: 1u32
                },
                &params,
            ).unwrap();

            frame.finish().unwrap();
            //println!("{:?}", start_time.elapsed());
            glib::Propagation::Proceed
        }
    }

    impl Oscilloscope {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            // This visualizer doesn't actually depend on the sample rate
        }
    }
}