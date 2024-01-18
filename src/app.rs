
use std::sync::{OnceLock};
use retour::static_detour;
use windows::core::s;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::OpenGL::wglDeleteContext;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::{CallWindowProcA, GWLP_WNDPROC, SetWindowLongPtrA, WNDPROC};
use crate::input::InputHandler;

type FnWglSwapBuffers = unsafe extern "stdcall" fn(isize) -> i32;
type FnRender = fn(&egui::Context);


static_detour! {
    static d_wglSwapBuffers: unsafe extern "stdcall" fn(isize) -> i32;
}


pub(crate) static mut CONTEXT: OnceLock<AppContext> = OnceLock::new();
pub(crate) static CONTEXT_HDC: OnceLock<HDC> = OnceLock::new();
pub(crate) static mut RENDER_FN: OnceLock<FnRender> = OnceLock::new();

pub(crate) struct AppContext {
    egui_ctx: egui::Context,
    painter: egui_glow::Painter,
    shapes: Vec<egui::epaint::ClippedShape>,
    textures_delta: egui::TexturesDelta,
    pub dimensions: [u32; 2],
    render_ctx: windows::Win32::Graphics::OpenGL::HGLRC,
    input_handler: InputHandler,
    o_wnd_proc: WNDPROC
}



pub(crate) unsafe extern "stdcall" fn h_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {

    if let Some(context) = CONTEXT.get_mut() {

        context.input_handler.handle_message(msg, wparam.0, lparam.0);


        let wants_egui_input = context.egui_ctx.wants_keyboard_input() || context.egui_ctx.wants_pointer_input();
        if wants_egui_input {
            return LRESULT(1);
        }

        return CallWindowProcA(context.o_wnd_proc, hwnd, msg, wparam, lparam);
    }
    LRESULT(1)
}


impl AppContext {
    fn destroy(&mut self) {
        unsafe {
            wglDeleteContext(self.render_ctx);
        }
        self.painter.destroy();
    }

    fn render(&mut self) {
        let egui::FullOutput {
            platform_output: _platform_output,
            textures_delta,
            shapes,
            pixels_per_point, ..
        } = self.egui_ctx.run(self.input_handler.collect_input(), |ctx| unsafe {
            (RENDER_FN.get_mut().unwrap())(ctx)
        });

        self.shapes = shapes;
        self.textures_delta.append(textures_delta);

        let shapes = std::mem::take(&mut self.shapes);
        let mut textures_delta = std::mem::take(&mut self.textures_delta);

        for (id, image_delta) in textures_delta.set {
            self.painter.set_texture(id, &image_delta);
        }


        let clipped_primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);
        self.painter.paint_primitives(
            self.dimensions,
            self.egui_ctx.pixels_per_point(),
            &clipped_primitives,
        );

        for id in textures_delta.free.drain(..) {
            self.painter.free_texture(id);
        }
    }

}

unsafe fn init_context() -> AppContext {

    let hdc = *CONTEXT_HDC.get().expect("failed to get hdc");

    gl_loader::init_gl();

    let window = windows::Win32::Graphics::Gdi::WindowFromDC(hdc);
    let mut dimensions = windows::Win32::Foundation::RECT::default();
    windows::Win32::UI::WindowsAndMessaging::GetClientRect(window, &mut dimensions)
        .expect("Failed to acquire window's dimensions");

    let old_context = windows::Win32::Graphics::OpenGL::wglGetCurrentContext();
    let new_context = windows::Win32::Graphics::OpenGL::wglCreateContext(hdc)
        .expect("Failed to create new context");

    windows::Win32::Graphics::OpenGL::wglMakeCurrent(hdc, new_context)
        .expect("Failed to make new context current");

    let glow_context = glow::Context::from_loader_function(|func| {
        gl_loader::get_proc_address(func) as *const _ // TODO: Get rid of the gl_loader dependency
    });
    let glow_context = std::sync::Arc::new(glow_context);

    let egui_context = egui::Context::default();
    let painter = egui_glow::Painter::new(glow_context, "", None)
        .expect("Failed to create renderer");

    windows::Win32::Graphics::OpenGL::wglMakeCurrent(hdc, old_context)
        .expect("Failed to make old context current");


    let o_wnd_proc = std::mem::transmute(SetWindowLongPtrA(window, GWLP_WNDPROC, h_wnd_proc as usize as _));


    println!("Context has been initialized");
    AppContext {
        egui_ctx: egui_context,
        render_ctx: new_context,
        painter,
        shapes: Default::default(),
        textures_delta: Default::default(),
        dimensions: [
            (dimensions.right - dimensions.left).try_into().unwrap(),
            (dimensions.bottom - dimensions.top).try_into().unwrap()
        ],
        input_handler: InputHandler::new(),
        o_wnd_proc,
    }
}

fn h_wgl_swap_buffers(hdc: isize) -> i32 {
    let hdc =  {
        CONTEXT_HDC.get_or_init(|| {
            let hdc = HDC(hdc);
            hdc
        });
        *CONTEXT_HDC.get().expect("failed to get hdc")
    };
    let context = unsafe {
        CONTEXT.get_or_init(|| {
            init_context()
        });
        CONTEXT.get_mut().expect("Failed to set rendering context")
    };

    unsafe {
        let old_context = windows::Win32::Graphics::OpenGL::wglGetCurrentContext();
        windows::Win32::Graphics::OpenGL::wglMakeCurrent(hdc, context.render_ctx)
            .expect("Failed to make new rendering context current");

        context.render();

        windows::Win32::Graphics::OpenGL::wglMakeCurrent(hdc, old_context)
            .expect("Failed to return to old context");

        d_wglSwapBuffers.call(hdc.0)
    }
}

pub fn set_render_fn(fun: FnRender) {
    unsafe {
        let _ = RENDER_FN.set(fun);
    }
}


pub fn init_hook() {
    unsafe {

        let opengl_handle = GetModuleHandleA(s!("opengl32.dll")).expect("Failed to get opengl handle");
        let wgl_swap_buffers_addr = GetProcAddress(
            opengl_handle,
            s!("wglSwapBuffers"),
        );
        let o_wgl_swap_buffers: FnWglSwapBuffers = std::mem::transmute(wgl_swap_buffers_addr);


        d_wglSwapBuffers
            .initialize(o_wgl_swap_buffers, h_wgl_swap_buffers)
            .expect("Failed to initialize the hook")
            .enable()
            .expect("Failed to enable the hook");
    }
}
