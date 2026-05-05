use crate::UiBackend;
use atomr_view_core::bridge::UiBridge;

pub struct NativeShellBackend;

impl UiBackend for NativeShellBackend {
    fn run(self, _bridge: UiBridge) {
        // Bridge to SwiftUI/Compose via uniffi
    }
}
