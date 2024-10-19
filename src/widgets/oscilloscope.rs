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
use gtk::subclass::prelude::ObjectSubclassIsExt;
use ringbuf::traits::Observer;

const BUFFER_SIZE: usize = 1024 * 16;

glib::wrapper! {
    pub struct Oscilloscope(ObjectSubclass<imp::Oscilloscope>)
        @extends gtk::GLArea, gtk::Widget;
}

impl Oscilloscope {
    pub fn new(sample_stream: HeapCons<StereoMagnitude>) -> Oscilloscope {
        let object = Object::builder().build();
        let imp = imp::Oscilloscope::from_obj(&object);
        imp.input_stream.replace(sample_stream);
        object.add_tick_callback(|oscilloscope, _| {
            if !oscilloscope.imp().input_stream.borrow().is_empty() {
                oscilloscope.queue_draw();
            }
            Continue
        });
        object
    }
}

mod imp {
    use std::cmp::min;
    use std::ops::Deref;
    use super::*;

    use glium::{index::PrimitiveType, program, uniform, Frame, Surface, BlendingFunction, Blend, Texture2d};
    use glium::Smooth::{Fastest, Nicest};
    use glium::texture::{MipmapsOption, UncompressedFloatFormat};
    use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter, SamplerWrapFunction};
    use gtk::{glib, prelude::*, subclass::prelude::*};
    use itertools::Itertools;
    use ringbuf::{HeapCons, HeapRb};
    use ringbuf::traits::Observer;
    use ringbuf_blocking::traits::Consumer;
    use crate::colorscheme::ColorScheme;
    use crate::fourier::{StereoMagnitude};

    #[derive(Properties)]
    #[properties(wrapper_type = super::Oscilloscope)]
    pub struct Oscilloscope {
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        pub input_stream: RefCell<HeapCons<StereoMagnitude>>,

        #[property(get, set)]
        pub palette: RefCell<ColorScheme>,

        context: RefCell<Option<Rc<glium::backend::Context>>>,
        program: RefCell<Option<glium::Program>>,
        texture: RefCell<Option<Texture2d>>,
        offset: Cell<usize>,
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
                texture: None.into(),
                offset: 0.into(),
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
                        uniform sampler2D tex;
                        void main() {
                            int index = gl_VertexID + int(ring_index);
                            vec2 uv = vec2(float(index) / float(num_samples), 0.0);
                            vec2 magnitude = texture(tex, uv).rg;
                            float x = 2.0 * (float(gl_VertexID) / float(num_samples)) - 1.0;
                            //gl_Position = vec4(x, uv.x, 0.0, 1.0);
                            gl_Position = vec4(x, magnitude[channel], 0.0, 1.0);
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

            let texture = Texture2d::empty_with_format(
                &context,
                UncompressedFloatFormat::F32F32,
                MipmapsOption::NoMipmap,
                BUFFER_SIZE as u32, 1
            ).unwrap();

            self.context.replace(Some(context));
            self.program.replace(Some(program));
            self.texture.replace(Some(texture));
        }

        fn unrealize(&self) {
            self.context.replace(None);
            self.program.replace(None);

            self.parent_unrealize();
        }
    }

    impl GLAreaImpl for Oscilloscope {
        fn render(&self, _context: &gdk::GLContext) -> glib::Propagation {
            let mut stream_binding = self.input_stream.borrow_mut();
            let stream = stream_binding.as_mut();
            let context_binding = self.context.borrow();
            let context = context_binding.as_ref().unwrap();
            let program_binding = self.program.borrow();
            let program = program_binding.as_ref().unwrap();
            let palette = self.palette.borrow();
            let (left_color, _) = palette.color_for((1.0, 0.0));
            let (right_color, _) = palette.color_for((0.0, 1.0));
            let bg_color = palette.background();
            let mut texture_binding = self.texture.borrow_mut();
            let texture = texture_binding.as_mut().unwrap();

            // Rebind textures that may have been clobbered by a bug elsewhere
            // (see: https://github.com/glium/glium/issues/2106)
            unsafe {
                // fixme: might not be necessary, a dummy texture might be enough to avoid the bug
                // (see: https://github.com/glium/glium/issues/2106)
                context.exec_with_context(|c| {
                    c.state.texture_units.iter_mut().for_each(|t| *t = Default::default());
                    epoxy::ActiveTexture(epoxy::TEXTURE0 + c.state.active_texture);
                });
            };

            let mut frame = Frame::new(
                context.clone(),
                context.get_framebuffer_dimensions(),
            );

            while !stream.is_empty() {
                let current_index = self.offset.get();
                let remaining_space = texture.width() as usize - current_index;
                let new_samples: Vec<_> = stream.pop_iter()
                    .take(remaining_space)
                    .collect();
                let block_size = new_samples.len();
                texture.write(glium::Rect {
                    left: current_index as u32,
                    bottom: 0,
                    width: block_size as u32,
                    height: 1,
                }, vec![new_samples]);
                self.offset.set((current_index + block_size) % texture.width() as usize);
            }

            let params = glium::DrawParameters {
                line_width: 2.0.into(),
                smooth: Fastest.into(),
                blend: Blend {
                    color: BlendingFunction::Max,
                    ..Default::default()
                },
                ..Default::default()
            };

            let sampler = texture.sampled()
                .wrap_function(SamplerWrapFunction::Repeat)
                .magnify_filter(MagnifySamplerFilter::Nearest)
                .minify_filter(MinifySamplerFilter::Nearest);

            frame.clear_color(bg_color.r as f32 / 255.0, bg_color.g as f32 / 255.0, bg_color.b as f32 / 255.0, 1.);
            frame.draw(
                glium::vertex::EmptyVertexAttributes { len: BUFFER_SIZE },
                &glium::index::NoIndices(PrimitiveType::LineStrip),
                program,
                &uniform! {
                    color: [left_color.r as f32 / 255.0, left_color.g as f32 / 255.0, left_color.b as f32 / 255.0],
                    num_samples: texture.width(),
                    ring_index: self.offset.get() as u32,
                    channel: 0u32,
                    tex: sampler,
                },
                &params,
            ).unwrap();
            frame.draw(
                glium::vertex::EmptyVertexAttributes { len: BUFFER_SIZE },
                &glium::index::NoIndices(PrimitiveType::LineStrip),
                &program,
                &uniform! {
                    color: [right_color.r as f32 / 255.0, right_color.g as f32 / 255.0, right_color.b as f32 / 255.0],
                    num_samples: texture.width(),
                    ring_index: self.offset.get() as u32,
                    channel: 1u32,
                    tex: sampler
                },
                &params,
            ).unwrap();

            frame.finish().unwrap();
            //println!("{:?}", start_time.elapsed());
            glib::Propagation::Stop
        }
    }

    impl Oscilloscope {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            // This visualizer doesn't actually depend on the sample rate
        }
    }
}