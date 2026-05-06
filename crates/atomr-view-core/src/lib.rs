pub mod actors;
pub mod bridge;
pub mod scene;

pub use bridge::{BackendCommand, BackendEvent, UiBridge, UiBridgeActor};
pub use scene::{SceneDescription, SceneKey, SceneNode, ScenePatch};

uniffi::setup_scaffolding!();
