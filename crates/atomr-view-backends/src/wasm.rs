use crate::UiBackend;
use atomr_view_core::bridge::UiBridge;

pub struct WebDomBackend;

impl UiBackend for WebDomBackend {
    fn run(self, _bridge: UiBridge) {
        // Wasm specific event loop using requestAnimationFrame
    }
}
