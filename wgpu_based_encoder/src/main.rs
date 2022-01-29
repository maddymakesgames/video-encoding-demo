use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::state::State;

mod camera;
mod controller;
mod state;
mod texture;

pub const FRAME_RATE: usize = 60;

fn main() {
    env_logger::init();
    stream_encoder::init_encoder();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    let curr_size = window.inner_size();
    window.set_inner_size(PhysicalSize {
        width: 256 * (curr_size.width / 256),
        height: curr_size.height,
    });

    let mut state = pollster::block_on(State::new(&window));

    event_loop.run(move |event, _, control_flow| match event {
        Event::RedrawRequested(window_id) if window_id == window.id() => {
            state.update();
            match pollster::block_on(state.render()) {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    eprintln!("OUT OF MEMORY!!");
                    *control_flow = ControlFlow::Exit;
                    state.close();
                }
                Err(e) => eprintln!("{e}"),
            }
        }
        Event::MainEventsCleared => window.request_redraw(),
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => {
            if !state.input(event) {
                match event {
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    } => {
                        *control_flow = ControlFlow::Exit;
                        state.close();
                    }
                    WindowEvent::Resized(size) => state.resize(*size),
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        state.resize(**new_inner_size)
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    });
}
