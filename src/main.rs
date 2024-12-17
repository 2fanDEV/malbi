use engine::app::Application;
use winit::{
    dpi::{LogicalSize, PhysicalSize, Size},
    event_loop::EventLoop,
};
mod engine;

fn main() {
    let event_loop = EventLoop::builder()
        .build()
        .expect("Failed to create EventLoop");
    let mut engine = Application::new(LogicalSize::new(1920, 1080));
    event_loop.run_app(&mut engine).unwrap();
    drop(engine.base_config);
    println!("Exited (0)");
}
