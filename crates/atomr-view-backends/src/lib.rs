// Backends are alpha-stage: most contain intentionally-stubbed
// command/event paths and typed scaffolding that isn't read yet.
// Re-tighten these lints once a backend graduates from stub state.
#![allow(unused, dead_code, clippy::single_match, clippy::arc_with_non_send_sync)]

use async_trait::async_trait;
use atomr_view_core::bridge::UiBridge;
use atomr_view_core::scene::{SceneDescription, ScenePatch};

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

pub mod bevy;
pub mod native_shell;
pub mod wasm;
pub mod winit_wgpu;
