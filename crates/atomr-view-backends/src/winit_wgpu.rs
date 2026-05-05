use crate::UiBackend;
use atomr_view_core::bridge::{UiBridge, BackendCommand, BackendEvent, InputEvent};
use atomr_view_core::scene::{SceneDescription, ScenePatch, NodeKind, PropertyValue};
use winit::{
    event::{Event, WindowEvent, ElementState},
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, Window, WindowId},
};
use std::collections::HashMap;
use egui_winit::State;
use egui_wgpu::Renderer;

pub struct WinitWgpuBackend;

struct WindowState {
    id: String,
    window: Window,
    egui_state: State,
    egui_renderer: Renderer,
    scene: Option<SceneDescription>,
    // wgpu specific fields (surface, device, queue, etc.)
}

impl UiBackend for WinitWgpuBackend {
    fn run(self, mut bridge: UiBridge) {
        let event_loop = EventLoop::new().unwrap();
        let mut windows: HashMap<WindowId, WindowState> = HashMap::new();
        let mut id_to_window: HashMap<String, WindowId> = HashMap::new();
        
        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            // Poll bridge for commands
            while let Ok(cmd) = bridge.cmd_rx.try_recv() {
                match cmd {
                    BackendCommand::CreateWindow { id, title } => {
                        let window = WindowBuilder::new()
                            .with_title(&title)
                            .build(elwt)
                            .unwrap();
                        let window_id = window.id();
                        
                        // In a real impl, we'd initialize wgpu here
                        // For now, placeholders for egui_state and egui_renderer
                        // let egui_state = State::new(...);
                        // let egui_renderer = Renderer::new(...);

                        id_to_window.insert(id.clone(), window_id);
                        // windows.insert(window_id, WindowState { ... });
                    }
                    BackendCommand::DestroyWindow { id } => {
                        if let Some(window_id) = id_to_window.remove(&id) {
                            windows.remove(&window_id);
                        }
                    }
                    BackendCommand::SetScene { window_id, scene } => {
                        if let Some(wid) = id_to_window.get(&window_id) {
                            if let Some(ws) = windows.get_mut(wid) {
                                ws.scene = Some(scene);
                            }
                        }
                    }
                    BackendCommand::ApplyPatches { window_id, patches: _ } => {
                        // Apply patches to ws.scene
                    }
                    BackendCommand::RequestRedraw { window_id } => {
                        if let Some(wid) = id_to_window.get(&window_id) {
                            if let Some(ws) = windows.get(wid) {
                                ws.window.request_redraw();
                            }
                        }
                    }
                    BackendCommand::OpenFilePicker { correlation_id: _, title: _ } => {
                        // Implement file picker
                    }

                }
            }

            match event {
                Event::WindowEvent { window_id, event, .. } => {
                    if let Some(ws) = windows.get_mut(&window_id) {
                        // let _ = ws.egui_state.on_window_event(&ws.window, &event);
                        
                        match event {
                            WindowEvent::CloseRequested => {
                                let _ = bridge.evt_tx.try_send(BackendEvent::WindowClosed { id: ws.id.clone() });
                                let id = ws.id.clone();
                                windows.remove(&window_id);
                                id_to_window.remove(&id);
                                if windows.is_empty() {
                                    elwt.exit();
                                }
                            }
                            WindowEvent::RedrawRequested => {
                                // Draw egui using ws.scene
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }).unwrap();
    }
}

// Helper to render the scene using egui
fn render_scene(ui: &mut egui::Ui, node: &atomr_view_core::scene::SceneNode) {
    match &node.kind {
        NodeKind::Container {} => {
            ui.vertical(|ui| {
                for child in &node.children {
                    render_scene(ui, child);
                }
            });
        }
        NodeKind::Text {} => {
            if let Some(PropertyValue::String { value }) = node.properties.0.get("text") {
                ui.label(value);
            }
        }
        NodeKind::Button {} => {
            if let Some(PropertyValue::String { value }) = node.properties.0.get("text") {
                if ui.button(value).clicked() {
                    // Send click event back to bridge
                }
            }
        }
        _ => {}
    }
}
