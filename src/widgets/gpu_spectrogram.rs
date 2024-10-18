use std::any::Any;
use std::{cell::RefCell, rc::Rc};
use std::cell::Cell;
use itertools::Itertools;

use gtk::{gdk, glib, prelude::*, subclass::prelude::*};

use adw::glib::{Properties, Object, ControlFlow::Continue};
use adw::subclass::prelude::ObjectSubclassExt;

use ringbuf::{HeapRb, HeapCons, traits::{Split, Observer}};
use ringbuf_blocking::traits::Consumer;

use crate::fourier::{Frequency, Period, StereoMagnitude, fft::FastFourierTransform, audio_transform::AudioStreamTransform};

use glium::{implement_vertex, index::PrimitiveType, program, uniform, Frame, Surface, VertexBuffer, uniforms::UniformBuffer, implement_uniform_block, BlendingFunction, Blend, Smooth::Nicest};

use crate::colorscheme::ColorScheme;
use crate::widgets::glarea_backend::GLAreaBackend;

const TEXTURE_WIDTH: i32 = 1024;
const TEXTURE_HEIGHT: i32 = 1024;

glib::wrapper! {
    pub struct GPUSpectrogram(ObjectSubclass<imp::GPUSpectrogram>)
        @extends gtk::GLArea, gtk::Widget;
}

const MAX_SAMPLES_PER_FRAME: usize = 1024;

impl GPUSpectrogram {
    pub fn new(sample_stream: HeapCons<StereoMagnitude>) -> GPUSpectrogram {
        let object = Object::builder().build();
        let imp = imp::GPUSpectrogram::from_obj(&object);
        imp.fft.borrow_mut().input_stream = sample_stream;
        object.add_tick_callback(|spectrogram, _| {
            // todo: only draw if there are unprocessed samples!
            spectrogram.queue_draw();
            Continue
        });
        object
    }
}

mod imp {
    use glium::{implement_buffer_content, Rect};
    use glium::texture::Dimensions::Texture2d;
    use glium::texture::{MipmapsOption, UncompressedFloatFormat};
    use super::*;

    #[derive(Copy, Clone)]
    struct Vertex {
        magnitude: f32,
    }
    implement_vertex!(Vertex, magnitude);

    #[derive(Properties)]
    #[properties(wrapper_type = super::GPUSpectrogram)]
    pub struct GPUSpectrogram {
        // FFT parameters
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        pub fft: RefCell<AudioStreamTransform<FastFourierTransform>>,

        #[property(get, set)]
        pub palette: RefCell<ColorScheme>,

        context: RefCell<Option<Rc<glium::backend::Context>>>,
        program: RefCell<Option<glium::Program>>,
        buffer: RefCell<Option<glium::texture::Texture2d>>,
        offset: Cell<usize>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GPUSpectrogram {
        const NAME: &'static str = "GPUSpectrogram";
        type Type = super::GPUSpectrogram;
        type ParentType = gtk::GLArea;

        fn new() -> Self {
            let (_, dummy_sample_stream) = HeapRb::new(1).split();
            let fft = AudioStreamTransform::new(
                dummy_sample_stream,
                FastFourierTransform::new(100 as Frequency, 1 as Period),
                2.0 / TEXTURE_WIDTH as f32,
            );
            Self {
                fft: fft.into(),
                palette: ColorScheme::new_mono(colorous::MAGMA, "magma").into(),
                context: None.into(),
                program: None.into(),
                buffer: None.into(),
                offset: 0.into(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for GPUSpectrogram {}

    impl WidgetImpl for GPUSpectrogram {
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

            let buffer = glium::texture::Texture2d::empty_with_format(
                &context,
                UncompressedFloatFormat::F16F16,
                MipmapsOption::AutoGeneratedMipmaps,
                TEXTURE_WIDTH as u32, TEXTURE_HEIGHT as u32,
            ).unwrap();
            // buffer.write(
            //     Rect{}
            // )

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

    impl GLAreaImpl for GPUSpectrogram {
        fn render(&self, _context: &gtk::gdk::GLContext) -> glib::Propagation {
            let context_binding = self.context.borrow();
            let context = context_binding.as_ref().unwrap();
            let program_binding = self.program.borrow();
            let program = program_binding.as_ref().unwrap();
            let palette = self.palette.borrow();
            let bg_color = palette.background();

            let mut frame = Frame::new(
                context.clone(),
                context.get_framebuffer_dimensions(),
            );
            // let (left, right): (Vec<Vertex>, Vec<Vertex>) = self.input_stream.borrow_mut().pop_iter()
            //     .take(num_samples)
            //     .map(|s| { (Vertex { magnitude: s.re }, Vertex { magnitude: s.im }) })
            //     .unzip();

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
            // frame.draw(
            //     &VertexBuffer::new(context, left.as_slice()).unwrap(),
            //     &glium::index::NoIndices(PrimitiveType::LineStrip),
            //     program,
            //     &uniform! {
            //         color: [left_color.r as f32 / 255.0, left_color.g as f32 / 255.0, left_color.b as f32 / 255.0],
            //         num_samples: num_samples as u32 - 1
            //     },
            //     &params,
            // ).unwrap();
            // frame.draw(
            //     &VertexBuffer::new(context, right.as_slice()).unwrap(),
            //     &glium::index::NoIndices(PrimitiveType::LineStrip),
            //     &program,
            //     &uniform! {
            //         color: [right_color.r as f32 / 255.0, right_color.g as f32 / 255.0, right_color.b as f32 / 255.0],
            //         num_samples: num_samples as u32 - 1
            //     },
            //     &params,
            // ).unwrap();
            frame.finish().unwrap();
            glib::Propagation::Proceed
        }
    }

    impl GPUSpectrogram {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            self.fft.borrow_mut().transform = FastFourierTransform::new(
                sample_rate as Frequency,
                0.05, // todo: this should be configurable!
            )
        }
    }
}
