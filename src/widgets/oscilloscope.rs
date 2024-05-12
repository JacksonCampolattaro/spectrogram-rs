use adw::glib::ControlFlow::Continue;
use adw::glib::Object;
use adw::subclass::prelude::ObjectSubclassExt;
use gtk::{gdk, glib, prelude::*};
use ringbuf::HeapConsumer;
use crate::fourier::StereoMagnitude;

glib::wrapper! {
    pub struct Oscilloscope(ObjectSubclass<imp::Oscilloscope>)
        @extends gtk::GLArea, gtk::Widget;
}

const MAX_SAMPLES_PER_FRAME: usize = 1024;

impl Oscilloscope {
    pub fn new(sample_stream: HeapConsumer<StereoMagnitude>) -> Oscilloscope {
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

unsafe impl glium::backend::Backend for Oscilloscope {
    fn swap_buffers(&self) -> Result<(), glium::SwapBuffersError> {
        // We're supposed to draw (and hence swap buffers) only inside the `render()`
        // vfunc or signal, which means that GLArea will handle buffer swaps for
        // us.
        Ok(())
    }

    unsafe fn get_proc_address(&self, symbol: &str) -> *const std::ffi::c_void {
        epoxy::get_proc_addr(symbol)
    }

    fn get_framebuffer_dimensions(&self) -> (u32, u32) {
        let scale = self.scale_factor();
        let width = self.width();
        let height = self.height();
        ((width * scale) as u32, (height * scale) as u32)
    }

    fn resize(&self, size: (u32, u32)) {
        self.set_size_request(size.0 as i32, size.1 as i32);
    }

    fn is_current(&self) -> bool {
        match self.context() {
            Some(context) => gdk::GLContext::current() == Some(context),
            None => false,
        }
    }

    unsafe fn make_current(&self) {
        GLAreaExt::make_current(self);
    }
}

mod imp {
    use std::{cell::RefCell, rc::Rc};
    use std::cell::Cell;
    use std::iter::zip;
    use std::time::Instant;
    use adw::glib::Properties;
    use cpal::SampleRate;

    use glium::{implement_vertex, index::PrimitiveType, program, uniform, Frame, IndexBuffer, Surface, VertexBuffer, uniforms::UniformBuffer, implement_uniform_block};
    use glium::Smooth::Nicest;
    use gtk::{glib, prelude::*, subclass::prelude::*};
    use itertools::Itertools;
    use num_traits::pow;
    use ringbuf::{HeapConsumer, HeapRb};
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

    struct Renderer {
        context: Rc<glium::backend::Context>,
        index_buffer: IndexBuffer<u16>,
        program: glium::Program,
    }

    impl Renderer {
        fn new(context: Rc<glium::backend::Context>) -> Self {
            // The following code is based on glium's triangle example:
            // https://github.com/glium/glium/blob/2ff5a35f6b097889c154b42ad0233c6cdc6942f4/examples/triangle.rs
            let index_buffer = IndexBuffer::new(
                &context,
                PrimitiveType::LineStrip,
                (0u16..MAX_SAMPLES_PER_FRAME as u16).collect_vec().as_slice(),
            ).unwrap();
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

            Renderer {
                context,
                index_buffer,
                program,
            }
        }

        // Setting should be passed here
        fn draw(&mut self, input_stream: &mut HeapConsumer<StereoMagnitude>) {
            // let start = Instant::now();
            let mut frame = Frame::new(
                self.context.clone(),
                self.context.get_framebuffer_dimensions(),
            );

            let num_samples = input_stream.len().min(MAX_SAMPLES_PER_FRAME);
            let (left, right): (Vec<Vertex>, Vec<Vertex>) = input_stream.pop_iter()
                .take(num_samples)
                .map(|s| { (Vertex { magnitude: s.re }, Vertex { magnitude: s.im }) })
                .unzip();


            let params = glium::DrawParameters {
                line_width: 2.0.into(), // todo: not supported on M1 mac?
                smooth: Nicest.into(),
                ..Default::default()
            };

            // todo: reuse index buffer, only take a slice of the first num_samples
            let index_buffer = self.index_buffer.slice(0..MAX_SAMPLES_PER_FRAME).unwrap();

            frame.clear_color(0., 0., 0., 0.);
            frame.draw(
                &VertexBuffer::new(&self.context, left.as_slice()).unwrap(),
                &index_buffer,
                &self.program,
                &uniform! {
                    color: [0.25, 0.25, 1f32],
                    num_samples: num_samples as u32
                },
                &params,
            ).unwrap();
            frame.draw(
                &VertexBuffer::new(&self.context, right.as_slice()).unwrap(),
                &index_buffer,
                &self.program,
                &uniform! {
                    color: [1.0, 0.25, 0.25f32],
                    num_samples: num_samples as u32
                },
                &params,
            ).unwrap();

            frame.finish().unwrap();

            // println!("{:?} | {:?}", middle - start, Instant::now() - middle);
        }
    }

    #[derive(Properties)]
    #[properties(wrapper_type = super::Oscilloscope)]
    pub struct Oscilloscope {
        renderer: RefCell<Option<Renderer>>,
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        pub input_stream: RefCell<HeapConsumer<StereoMagnitude>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Oscilloscope {
        const NAME: &'static str = "Oscilloscope";
        type Type = super::Oscilloscope;
        type ParentType = gtk::GLArea;

        fn new() -> Self {
            let (_, dummy_sample_stream) = HeapRb::new(1).split();
            Self {
                renderer: None.into(),
                input_stream: dummy_sample_stream.into(),
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
                glium::backend::Context::new(widget.clone(), true, Default::default())
            }.unwrap();
            *self.renderer.borrow_mut() = Some(Renderer::new(context));
        }

        fn unrealize(&self) {
            *self.renderer.borrow_mut() = None;

            self.parent_unrealize();
        }
    }

    impl GLAreaImpl for Oscilloscope {
        fn render(&self, _context: &gtk::gdk::GLContext) -> glib::Propagation {

            // todo: move samples to buffer

            // let r = self.renderer.borrow_mut().as_mut().unwrap();
            // r.draw(&mut self.input_stream.borrow_mut());
            self.renderer.borrow_mut().as_mut().unwrap().draw(&mut self.input_stream.borrow_mut());
            // self.renderer.borrow().as_ref().unwrap().draw(&mut self.input_stream.borrow_mut());
            glib::Propagation::Stop
        }
    }

    impl Oscilloscope {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            // todo!()
        }
    }
}