use std::process::exit;

use winit::{
    application::ApplicationHandler,
    dpi::Size,
    event::WindowEvent,
    window::{Window, WindowAttributes},
};

use super::base_configuration::BaseConfig;
pub struct Application {
    pub base_config: Option<BaseConfig>,
    resolution: Size,
    window: Option<Window>,
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        println!("{:?}", self.resolution);
        self.window = Some(
            event_loop
                .create_window(WindowAttributes::default().with_inner_size(self.resolution))
                .expect("Failed to create window"),
        );
        println!("window created");

        let base_config_res = BaseConfig::init(self.window.as_mut().unwrap());
        match base_config_res {
            Ok(base) => {
                self.base_config = Some(base);
            }
            Err(_) => panic!(),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::Destroyed => {
                let _x = self.base_config.as_mut().unwrap();
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {}
            _ => {
                println!("{:?}", event);
            }
        }
    }
}

impl Application {
    pub fn new<S>(resolution: S) -> Self
    where
        S: Into<Size>,
    {
        Self {
            base_config: None,
            resolution: resolution.into(),
            window: None,
        }
    }
}
