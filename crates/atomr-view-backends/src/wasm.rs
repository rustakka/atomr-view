use crate::UiBackend;
use atomr_view_core::bridge::{UiBridge, BackendCommand, BackendEvent};
use wasm_bindgen::prelude::*;
use web_sys::{window, Document, Element};

pub struct WebDomBackend;

impl UiBackend for WebDomBackend {
    fn run(self, mut bridge: UiBridge) {
        let window = window().expect("no global `window` exists");
        let document = window.document().expect("should have a document on window");
        let body = document.body().expect("document should have a body");

        // Set up the cooperative executor tick
        let f = std::sync::Arc::new(std::sync::Mutex::new(None));
        let g = f.clone();

        *g.lock().unwrap() = Some(Closure::wrap(Box::new(move || {
            // 1. Drain actor commands and mutate DOM
            while let Ok(cmd) = bridge.cmd_rx.try_recv() {
                match cmd {
                    BackendCommand::SetScene { .. } => {
                        // Reconcile DOM
                    }
                    _ => {}
                }
            }

            // 2. Request next frame
            request_animation_frame(f.lock().unwrap().as_ref().unwrap());
        }) as Box<dyn FnMut()>));

        request_animation_frame(g.lock().unwrap().as_ref().unwrap());
    }
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window().unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}
