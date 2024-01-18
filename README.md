# egui-hook
My egui hook. Works with 0.25 and tailored for Geometry Dash.

# How to use

```rust
use egui_hook::{init_hook, set_render_fn};

fn render(ctx: &egui::Context) {
    egui::Window::new("hello").show(ctx, |ui| {
        let _ = ui.button("hello");
    });
}


unsafe extern "system" fn main_thread(_lp_param: *mut c_void) -> u32 {

    set_render_fn(render);
    init_hook();

    0
}


#[no_mangle]
extern "system" fn DllMain(dll_module: u32, call_reason: u32, _reserved: usize) -> windows::Win32::Foundation::BOOL
{
    match call_reason {
        1 => unsafe { // DLL_PROCESS_ATTACH
            windows::Win32::System::Threading::CreateThread(
                None,
                0,
                Some(main_thread),
                Some(dll_module as *const c_void),
                windows::Win32::System::Threading::THREAD_CREATION_FLAGS(0),
                None
            ).expect("Failed to create thread");
        },
        0 => (), // DLL_PROCESS_DETACH
        _ => ()
    }
    windows::Win32::Foundation::TRUE
}



```
