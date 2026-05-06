use atomr_view_core::bridge::UiBridge;
use atomr_view_core::scene::{SceneDescription, ScenePatch};
use async_trait::async_trait;

uniffi::setup_scaffolding!();

#[async_trait]
pub trait UiBackend: Send + 'static {
    fn run(self, bridge: UiBridge);
}

pub trait Reconciler: 'static {
    type NativeTree;
    fn apply_full(&mut self, tree: &mut Self::NativeTree, scene: &SceneDescription);
    fn apply_patches(&mut self, tree: &mut Self::NativeTree, patches: &[ScenePatch]);
}

pub mod winit_wgpu;
pub mod wasm;
pub mod native_shell;
pub mod bevy;
