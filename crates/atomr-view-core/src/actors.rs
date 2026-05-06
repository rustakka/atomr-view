use crate::bridge::{BackendCommand, BackendEvent, UiBridgeMessage};
use crate::scene::{SceneDescription, ScenePatch};
use atomr_core::prelude::*;

pub struct WindowActor {
    pub id: String,
    pub bridge: ActorRef<UiBridgeMessage>, // UiBridgeActor Ref
    pub scene: SceneDescription,
}

#[async_trait]
impl Actor for WindowActor {
    type Msg = WindowMessage;

    async fn handle(&mut self, ctx: &mut Context<Self>, msg: WindowMessage) {
        match msg {
            WindowMessage::UpdateScene(scene) => {
                self.scene = scene.clone();
                self.bridge.tell(UiBridgeMessage::Command(BackendCommand::SetScene {
                    window_id: self.id.clone(),
                    scene,
                }));
            }
            WindowMessage::PatchScene(patches) => {
                // In a real impl, we'd apply patches to `self.scene` here
                self.bridge.tell(UiBridgeMessage::Command(BackendCommand::ApplyPatches {
                    window_id: self.id.clone(),
                    patches,
                }));
            }
            WindowMessage::BackendEvent(evt) => {
                match evt {
                    BackendEvent::WindowClosed { .. } => {
                        ctx.stop_self();
                    }
                    BackendEvent::Input { event: _, .. } => {
                        // Handle input, maybe forward to regions
                    }
                    _ => {}
                }
            }
        }
    }
}

pub enum WindowMessage {
    UpdateScene(SceneDescription),
    PatchScene(Vec<ScenePatch>),
    BackendEvent(BackendEvent),
}

pub struct RegionActor {
    pub id: String,
    pub parent_window: ActorRef<WindowMessage>, // WindowActor Ref
}

#[async_trait]
impl Actor for RegionActor {
    type Msg = RegionMessage;

    async fn handle(&mut self, _ctx: &mut Context<Self>, msg: RegionMessage) {
        match msg {
            RegionMessage::StateUpdate => {
                // Compute new scene fragment and send to parent_window
            }
        }
    }
}

pub enum RegionMessage {
    StateUpdate,
}

pub struct PersistenceActor;

#[async_trait]
impl Actor for PersistenceActor {
    type Msg = PersistenceMessage;

    async fn handle(&mut self, _ctx: &mut Context<Self>, _msg: PersistenceMessage) {
        // Integrate with atomr-persistence
    }
}

pub enum PersistenceMessage {
    Save(String, String),
    Load(String),
}

pub struct InferenceActor;

#[async_trait]
impl Actor for InferenceActor {
    type Msg = InferenceMessage;

    async fn handle(&mut self, _ctx: &mut Context<Self>, _msg: InferenceMessage) {
        // Integrate with atomr-infer
    }
}

pub enum InferenceMessage {
    Predict(String),
}
