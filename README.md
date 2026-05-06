# `atomr-view` — Heavy Client UI for Federated `atomr` Systems

**Status:** Implementation Phase Complete / Testing Suite Active

`atomr-view` is an actor-based view management system designed for heavy
client applications (Desktop, Mobile, Wasm) that integrate with federated
`atomr` clusters. It treats the UI rendering surface as a supervised peer
with network-shaped failure modes, using the actor model to bridge the gap
between remote state authority and local pixels.

## Mental Model

In `atomr-view`, the UI is not a separate layer bridged by callbacks; it is a
subgraph of the local `ActorSystem`.

- **`WindowActor`**: Owns an OS window, its declarative scene, and child regions.
- **`RegionActor`**: Manages a stateful subtree of the UI, typically subscribed
  to a remote sharded entity or a local streaming feed.
- **`UiBridge`**: An asynchronous, backpressured conduit between the actor
  system and the rendering thread (e.g., `winit` + `wgpu`).
- **`SceneDescription`**: A declarative, keyed tree of semantic UI intent
  (Button, Text, etc.) that the backend reconciles against native widgets.

## Key Features

- **Location Transparency**: UI actors interact with cluster-sharded actors using
  the same `tell`/`ask`/`watch` primitives as local actors.
- **Backpressure Discipline**: The `UiBridge` uses conflation for state updates,
  ensuring the UI thread only ever processes the freshest state available.
- **Declarative Reconciliation**: Keyed tree-diffing computes minimal patches,
  preserving local UI state (like cursor position) across updates.
- **Multi-Backend Strategy**: Supports `winit` + `wgpu` + `egui` for desktop,
  with architectural hooks for Wasm (DOM) and Mobile (SwiftUI/Compose).
- **Python-First API**: Full integration via PyO3, allowing UI actors and
  complex scene manipulation to be authored in Python.

## Platform Backends

`atomr-view` supports multiple rendering surfaces via the `UiBackend` strategy:

- **Desktop (`winit` + `wgpu` + `egui`)**: The default reference implementation
  for high-performance desktop apps.
- **Bevy (`BevyBackend`)**: Integrates the actor system with Bevy's ECS. Use the
  `ActorBacked` component to mirror ECS entities with long-lived actors for
  persistence and netcode.
- **Wasm (`WebDomBackend`)**: Targets browser environments with a cooperative
  single-threaded scheduler that yields to `requestAnimationFrame`.
- **Mobile (`NativeShellBackend`)**: Bridges to SwiftUI (iOS) and Jetpack
  Compose (Android) via `uniffi`, maintaining native UI performance while
  keeping business logic in Rust actors.

## Quick Start

### Rust

```rust
use atomr_view_core::prelude::*;
use atomr_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sys = ActorSystem::create("App", Config::reference()).await?;
    
    // 1. Initialize the bridge and backend
    let (cmd_tx, cmd_rx) = mpsc::channel(100);
    let (evt_tx, evt_rx) = mpsc::channel(100);
    let bridge = UiBridge { cmd_rx, evt_tx };
    
    // 2. Start the bridge actor
    let bridge_actor = sys.actor_of(Props::create(move || {
        UiBridgeActor::new(cmd_tx.clone(), evt_rx)
    }), "ui-bridge")?;

    // 3. Define a window actor
    let main_window = sys.actor_of(Props::create(move || {
        WindowActor::new("main", bridge_actor.clone())
    }), "main-window")?;

    // 4. Run the backend (blocks main thread)
    WinitWgpuBackend.run(bridge);
    
    Ok(())
}
```

### Python

```python
import atomr_view as view

ui = view.PyUiSystem()
ui.create_window("main", "My Federated App")

# Update the scene declaratively
root = view.SceneNode(
    key=view.SceneKey(),
    kind=view.NodeKind.Container(),
    properties=view.PropertyMap({"layout": view.PropertyValue.String("vertical")}),
    children=[
        view.SceneNode(
            key=view.SceneKey(),
            kind=view.NodeKind.Text(),
            properties=view.PropertyMap({"text": view.PropertyValue.String("Hello from Python!")}),
            children=[]
        )
    ]
)
ui.set_scene("main", view.SceneDescription(root))
```

## Implementation Status

- [x] **Core Foundation**: `UiBridge`, `SceneDescription`, keyed diffing engine.
- [x] **Desktop Backend**: `winit` + `wgpu` + `egui` reference implementation.
- [x] **Python Bindings**: PyO3-based bridge with full scene manipulation support.
- [x] **Testing**: Property-based tests for diffing invariants and async integration tests.
- [x] **Bevy Integration**: `BevyBackend` bridging actors with ECS entities.
- [x] **Wasm Support**: `WebDomBackend` with cooperative cooperative scheduler hooks.
- [x] **Mobile Flow**: `NativeShellBackend` with `uniffi` bindings for native shells.

## Project Structure

- `crates/atomr-view-core`: Core actor logic and scene engine.
- `crates/atomr-view-backends`: Swappable rendering strategies.
- `crates/atomr-view-python`: PyO3 integration.
- `ai-skills/`: Skills for AI assistants working with `atomr-view`.

## License

Apache-2.0
