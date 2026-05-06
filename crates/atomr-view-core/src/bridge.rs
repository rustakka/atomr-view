use crate::scene::{SceneDescription, SceneKey, ScenePatch};
use atomr_core::prelude::*;
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, uniffi::Record)]
pub struct CorrelationId {
    pub id: String,
}

#[pymethods]
impl CorrelationId {
    #[new]
    pub fn new() -> Self {
        Self { id: Uuid::new_v4().to_string() }
    }

    pub fn __repr__(&self) -> String {
        self.id.clone()
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum BackendCommand {
    CreateWindow { id: String, title: String },
    DestroyWindow { id: String },
    SetScene { window_id: String, scene: SceneDescription },
    ApplyPatches { window_id: String, patches: Vec<ScenePatch> },
    RequestRedraw { window_id: String },
    OpenFilePicker { correlation_id: CorrelationId, title: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum BackendEvent {
    WindowCreated { id: String },
    WindowClosed { id: String },
    Input { window_id: String, event: InputEvent },
    FrameTick { now_ms: u64 },
    FilePickerResult { correlation_id: CorrelationId, path: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum InputEvent {
    Click { key: SceneKey, x: f32, y: f32 },
    KeyDown { key_code: String },
    TextInput { text: String },
}

pub struct UiBridge {
    pub cmd_rx: mpsc::Receiver<BackendCommand>,
    pub evt_tx: mpsc::Sender<BackendEvent>,
}

pub struct UiBridgeActor {
    cmd_tx: mpsc::Sender<BackendCommand>,
    evt_rx: Option<mpsc::Receiver<BackendEvent>>,
    window_routes: HashMap<String, UntypedActorRef>,
    pending_scenes: HashMap<String, SceneDescription>,
}

impl UiBridgeActor {
    pub fn new(cmd_tx: mpsc::Sender<BackendCommand>, evt_rx: mpsc::Receiver<BackendEvent>) -> Self {
        Self { cmd_tx, evt_rx: Some(evt_rx), window_routes: HashMap::new(), pending_scenes: HashMap::new() }
    }

    async fn flush_pending_scenes(&mut self) {
        for (window_id, scene) in self.pending_scenes.drain() {
            let _ = self.cmd_tx.send(BackendCommand::SetScene { window_id, scene }).await;
        }
    }
}

#[async_trait]
impl Actor for UiBridgeActor {
    type Msg = UiBridgeMessage;

    async fn handle(&mut self, ctx: &mut Context<Self>, msg: UiBridgeMessage) {
        match msg {
            UiBridgeMessage::RegisterWindow { id, actor } => {
                self.window_routes.insert(id, actor);
            }
            UiBridgeMessage::Command(cmd) => {
                match cmd {
                    BackendCommand::SetScene { window_id, scene } => {
                        // Conflate: only keep the latest scene for this window
                        self.pending_scenes.insert(window_id, scene);
                        // In a real impl, we might use a timer to flush,
                        // or flush on next frame tick.
                        self.flush_pending_scenes().await;
                    }
                    _ => {
                        let _ = self.cmd_tx.send(cmd).await;
                    }
                }
            }
            UiBridgeMessage::StartEventLoop => {
                if let Some(mut evt_rx) = self.evt_rx.take() {
                    let myself = ctx.self_ref().clone();
                    tokio::spawn(async move {
                        while let Some(evt) = evt_rx.recv().await {
                            myself.tell(UiBridgeMessage::InternalEvent(evt));
                        }
                    });
                }
            }
            UiBridgeMessage::InternalEvent(evt) => {
                match evt {
                    BackendEvent::WindowClosed { id } => {
                        if let Some(_route) = self.window_routes.get(&id) {
                            // Forward to window actor
                            // route.tell(...)
                        }
                        self.window_routes.remove(&id);
                        self.pending_scenes.remove(&id);
                    }
                    BackendEvent::Input { window_id, event: _ } => {
                        if let Some(_route) = self.window_routes.get(&window_id) {
                            // route.tell(...)
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiBridgeMessage {
    RegisterWindow { id: String, actor: UntypedActorRef },
    Command(BackendCommand),
    StartEventLoop,
    InternalEvent(BackendEvent),
}
