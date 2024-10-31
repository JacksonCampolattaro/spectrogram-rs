use adw::gdk;
use gtk::GLArea;
use gtk::prelude::{GLAreaExt, WidgetExt};

pub struct GLAreaBackend(GLArea);

impl From<GLArea> for GLAreaBackend {
    fn from(value: GLArea) -> Self {
        Self { 0: value }
    }
}

unsafe impl glium::backend::Backend for GLAreaBackend {
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
        let scale = self.0.scale_factor();
        let width = self.0.width();
        let height = self.0.height();
        ((width * scale) as u32, (height * scale) as u32)
    }

    fn resize(&self, size: (u32, u32)) {
        self.0.set_size_request(size.0 as i32, size.1 as i32);
    }

    fn is_current(&self) -> bool {
        match self.0.context() {
            Some(context) => gdk::GLContext::current() == Some(context),
            None => false,
        }
    }

    unsafe fn make_current(&self) {
        GLAreaExt::make_current(&self.0);
    }
}
