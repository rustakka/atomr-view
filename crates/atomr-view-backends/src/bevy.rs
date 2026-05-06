use crate::UiBackend;
use atomr_view_core::bridge::{UiBridge, BackendCommand, BackendEvent, InputEvent};
use atomr_view_core::scene::{SceneDescription, ScenePatch, NodeKind, PropertyValue};
use bevy::prelude::*;
use bevy::app::AppExit;

pub struct BevyBackend;

#[derive(Resource)]
pub struct UiBridgeReceiver(pub tokio::sync::mpsc::Receiver<BackendCommand>);

#[derive(Resource)]
pub struct UiBridgeSender(pub tokio::sync::mpsc::Sender<BackendEvent>);

impl UiBackend for BevyBackend {
    fn run(self, bridge: UiBridge) {
        let mut app = App::new();
        app.add_plugins(DefaultPlugins)
           .insert_resource(UiBridgeReceiver(bridge.cmd_rx))
           .insert_resource(UiBridgeSender(bridge.evt_tx))
           .add_systems(PreUpdate, drain_actor_commands)
           .add_systems(PostUpdate, forward_backend_events)
           .run();
    }
}

fn drain_actor_commands(
    mut commands: Commands,
    mut bridge: ResMut<UiBridgeReceiver>,
    mut exit: EventWriter<AppExit>,
) {
    while let Ok(cmd) = bridge.0.try_recv() {
        match cmd {
            BackendCommand::CreateWindow { .. } => {
                // Bevy already has a window, but we could spawn additional ones
            }
            BackendCommand::DestroyWindow { .. } => {
                exit.send(AppExit);
            }
            BackendCommand::SetScene { .. } => {
                // Update Bevy UI entities
            }
            BackendCommand::ApplyPatches { .. } => {
                // Apply patches to Bevy UI entities
            }
            BackendCommand::RequestRedraw { .. } => {}
            BackendCommand::OpenFilePicker { .. } => {}
        }
    }
}

fn forward_backend_events(
    bridge: Res<UiBridgeSender>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        // Send event to actor system
    }
    // ... forward more events
}

#[derive(Component)]
pub struct ActorBacked {
    pub actor_id: String, // Simplified for now
}
