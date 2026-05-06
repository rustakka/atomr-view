use crate::UiBackend;
use atomr_view_core::bridge::{UiBridge, BackendCommand, BackendEvent};
use std::sync::Arc;

pub struct NativeShellBackend {
    pub callback: Arc<dyn NativeUiCallback>,
}

#[uniffi::export(callback_interface)]
pub trait NativeUiCallback: Send + Sync {
    fn on_command(&self, cmd: BackendCommand);
}

impl UiBackend for NativeShellBackend {
    fn run(self, mut bridge: UiBridge) {
        // This runs on its own thread, polling commands and pushing to FFI
        let callback = self.callback.clone();
        tokio::spawn(async move {
            while let Some(cmd) = bridge.cmd_rx.recv().await {
                callback.on_command(cmd);
            }
        });
        
        // Native shell owns the actual main loop elsewhere
    }
}
