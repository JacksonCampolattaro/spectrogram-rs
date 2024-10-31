use std::any::Any;
use std::{cell::RefCell, rc::Rc};
use std::cell::Cell;
use itertools::Itertools;

use gtk::{gdk, glib, prelude::*, subclass::prelude::*};

use adw::glib::{Properties, Object, ControlFlow::Continue, property::PropertySet};
use adw::subclass::prelude::ObjectSubclassExt;

use ringbuf::{HeapRb, HeapCons, traits::{Split, Observer}};
use ringbuf_blocking::traits::Consumer;

use crate::fourier::{Frequency, Period, StereoMagnitude, fft::FastFourierTransform, audio_transform::AudioStreamTransform};

use glium::{index::PrimitiveType, program, uniform, Frame, Surface, Blend, Smooth::Nicest};

use crate::colorscheme::ColorScheme;
use crate::widgets::glarea_backend::GLAreaBackend;

const VIEWPORT_FRAMES: usize = 2048;
const VIEWPORT_SECONDS: f32 = 2.5f32;
const FRAMES_PER_SECOND: f32 = VIEWPORT_FRAMES as f32 / VIEWPORT_SECONDS;

glib::wrapper! {
    pub struct GPUSpectrogram(ObjectSubclass<imp::GPUSpectrogram>)
        @extends gtk::GLArea, gtk::Widget;
}

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
    use std::ops::DerefMut;
    use glium::{implement_buffer_content, Rect};
    use glium::texture::Texture2d;
    use glium::texture::{MipmapsOption, UncompressedFloatFormat};
    use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter, SamplerWrapFunction};
    use crate::fourier::audio_transform::AudioTransform;
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::GPUSpectrogram)]
    pub struct GPUSpectrogram {
        // FFT parameters
        #[property(name = "sample-rate", set = Self::set_sample_rate, type = u32)]
        pub fft: RefCell<AudioStreamTransform<FastFourierTransform>>,

        #[property(set = Self::set_palette, type = ColorScheme)]
        pub palette: RefCell<ColorScheme>,

        context: RefCell<Option<Rc<glium::backend::Context>>>,
        program: RefCell<Option<glium::Program>>,
        palette_texture: RefCell<Option<Texture2d>>,
        fft_texture: RefCell<Option<Texture2d>>,
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
                1f32 / FRAMES_PER_SECOND,
            );
            Self {
                fft: fft.into(),
                palette: ColorScheme::new_mono(colorous::MAGMA, "magma").into(),
                context: None.into(),
                program: None.into(),
                palette_texture: None.into(),
                fft_texture: None.into(),
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
                        out vec2 uv;
                        void main() {
                                // Create one triangle that covers the viewport
                                vec2 vertices[3] = vec2[3](vec2(-1,-1), vec2(3,-1), vec2(-1, 3));
                                gl_Position = vec4(vertices[gl_VertexID], 0, 1);
                                uv = 0.5 * gl_Position.xy + vec2(0.5);
                        }
                    ",
                    fragment: "
                        #version 150
                        in vec2 uv;
                        uniform uint num_samples;
                        uniform uint offset;

                        uniform float min_frequency;
                        uniform float max_frequency;

                        uniform float min_db;
                        uniform float max_db;

                        uniform sampler2D fft;
                        uniform sampler2D palette;
                        out vec4 f_color;
                        void main() {

                            float min_frequency = 32;
                            float max_frequency = 22030;
                            float frequency_range = max_frequency - min_frequency;
                            float linear_frequency = uv.y * frequency_range + min_frequency;
                            float linear_frequency_mapped = linear_frequency / max_frequency;

                            float log_min_frequency = log(min_frequency);
                            float log_max_frequency = log(max_frequency);
                            float log_frequency_range = log_max_frequency - log_min_frequency;
                            float log_frequency = uv.y * log_frequency_range + log_min_frequency;
                            float log_frequency_mapped = exp(log_frequency) / max_frequency;


                            // Log-scale coordinates
                            vec2 coord = vec2(
                                // Time (with offset)
                                (uv.x * num_samples + offset) / num_samples,
                                // Frequency
                                log_frequency_mapped
                            );

                            // Get magnitude
                            vec2 magnitude = texture(fft, coord.yx).rg;

                            // Convert to decibels
                            float magnitude_power = dot(magnitude, magnitude);
                            float magnitude_log = 10 * log(magnitude_power + 1e-7) / log(10);
                            float magnitude_db = (magnitude_log - min_db) / (max_db - min_db);

                            // Determine left/right panning
                            float pan = magnitude.y / (magnitude.x + magnitude.y);

                            // Get the appropriate color for this magnitude
                            f_color = texture(palette, vec2(pan, magnitude_db));

                            // Debugging
                            //f_color = texture(palette, uv);

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

    impl GLAreaImpl for GPUSpectrogram {
        fn render(&self, _context: &gtk::gdk::GLContext) -> glib::Propagation {
            let context_binding = self.context.borrow();
            let context = context_binding.as_ref().unwrap();
            let program_binding = self.program.borrow();
            let program = program_binding.as_ref().unwrap();
            let palette = self.palette.borrow();
            let bg_color = palette.background();
            let mut fft = self.fft.borrow_mut();

            // Create fft texture if it's missing
            if self.fft_texture.borrow().is_none() {
                self.fft_texture.set(Texture2d::empty_with_format(
                    context,
                    UncompressedFloatFormat::F16F16,
                    MipmapsOption::AutoGeneratedMipmaps,
                    fft.transform.num_output_frequencies() as u32,
                    //fft.transform.sample_rate() as u32,
                    VIEWPORT_FRAMES as u32,
                ).unwrap().into());
            }
            let fft_texture_binding = self.fft_texture.borrow();
            let fft_texture = fft_texture_binding.as_ref().unwrap();

            // Create palette texture if it's missing
            if self.palette_texture.borrow().is_none() {
                self.palette_texture.set(Texture2d::with_format(
                    context,
                    self.palette.borrow().lookup_table(32),
                    UncompressedFloatFormat::F32F32F32F32,
                    MipmapsOption::NoMipmap, // todo: are mipmaps necessary?
                ).unwrap().into());
            }
            let palette_texture_binding = self.palette_texture.borrow_mut();
            let palette_texture = palette_texture_binding.as_ref().unwrap();

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

            // Copy over new data
            let mut stream = fft.process();
            loop {
                let current_index = self.offset.get();
                let remaining_space = fft_texture.height() as usize - current_index;

                let new_samples: Vec<_> = stream.by_ref().take(remaining_space).collect();
                if new_samples.is_empty() {
                    break;
                }

                let block_size = new_samples.len();
                let num_frequencies = new_samples[0].len();
                // todo: reshape the texture if the number of frequencies changed
                fft_texture.write(Rect {
                    left: 0,
                    bottom: current_index as u32,
                    width: num_frequencies as u32,
                    height: block_size as u32,
                }, new_samples);
                self.offset.set((current_index + block_size) % fft_texture.height() as usize);
            }

            let params = glium::DrawParameters {
                line_width: 2.0.into(),
                smooth: Nicest.into(),
                blend: Blend::alpha_blending(),
                ..Default::default()
            };

            let fft_sampler = fft_texture.sampled()
                .wrap_function(SamplerWrapFunction::Repeat)
                .magnify_filter(MagnifySamplerFilter::Linear)
                .minify_filter(MinifySamplerFilter::Linear);
            let palette_sampler = palette_texture.sampled()
                .wrap_function(SamplerWrapFunction::Clamp)
                .magnify_filter(MagnifySamplerFilter::Linear)
                .minify_filter(MinifySamplerFilter::Linear);

            let mut frame = Frame::new(
                context.clone(),
                context.get_framebuffer_dimensions(),
            );
            frame.clear_color(bg_color.r as f32 / 255.0, bg_color.g as f32 / 255.0, bg_color.b as f32 / 255.0, 1.0);
            frame.draw(
                glium::vertex::EmptyVertexAttributes { len: 3 },
                &glium::index::NoIndices(PrimitiveType::TrianglesList),
                program,
                &uniform! {
                    num_samples: fft_texture.height(),
                    offset: self.offset.get() as u32,
                    min_frequency: 32.0,
                    max_frequency: 22000.0,
                    min_db: -70f32,
                    max_db: -10f32,
                    fft: fft_sampler,
                    palette: palette_sampler,
                },
                &params,
            ).unwrap();
            frame.finish().unwrap();
            glib::Propagation::Proceed
        }
    }

    impl GPUSpectrogram {
        pub fn set_sample_rate(&self, sample_rate: u32) {
            self.fft.borrow_mut().transform = FastFourierTransform::new(
                sample_rate as Frequency,
                0.05, // todo: this should be configurable!
            );
            // Force reconstruction of the fft texture to account for the new sample rate
            self.fft_texture.set(None);
        }

        pub fn set_palette(&self, palette: ColorScheme) {
            self.palette.set(palette);
            // Force reconstruction of the palette texture
            self.palette_texture.set(None);
        }
    }
}
