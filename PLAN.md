# Implementation Plan: `atomr-view`

This plan outlines the implementation of an actor-based view management system in Rust with Python bindings, following the architecture described in `docs/rustakka-ui-architecture-v1.md` and the system design of `rustakka/atomr`.

## 1. Crate Structure

The project will be organized as a Cargo workspace:

- `crates/atomr-view-core`: The core logic, traits, and types.
- `crates/atomr-view-backends`: Implementation of various UI backends (e.g., `winit-wgpu`).
- `crates/atomr-view-python`: PyO3 bindings for exposing the system to Python.
- `examples/`: Rust and Python examples.

## 2. Core Components (`atomr-view-core`)

### 2.1 `UiBridge` & `UiBridgeActor`
- **`BackendCommand`**: Enum for commands sent from actors to the UI thread (e.g., `CreateWindow`, `UpdateScene`).
- **`BackendEvent`**: Enum for events sent from the UI thread to actors (e.g., `Input`, `WindowClosed`).
- **`UiBridge`**: Holds the bounded MPSC channels (`cmd_rx`, `evt_tx`).
- **`UiBridgeActor`**: A singleton actor that manages the bridge, routing events to the correct `WindowActor` or `InputRouter`.

### 2.2 `SceneDescription` & Reconciliation
- **`SceneDescription`**: A declarative tree of `SceneNode`s.
- **`SceneNode`**: Represents a semantic UI element (Text, Button, Container, etc.) with a stable `SceneKey`.
- **`ScenePatch`**: Incremental updates to the scene tree.
- **`Reconciler`**: Trait for backends to apply `SceneDescription` changes to their native UI representation.

### 2.3 Actors
- **`WindowActor`**: Manages a single OS window, its scene, and child regions.
- **`RegionActor`**: Manages a stateful subtree of a window, often backed by remote state.

## 3. Backends (`atomr-view-backends`)

### 3.1 `WinitWgpuBackend` (Default)
- Uses `winit` for windowing and `wgpu` for hardware-accelerated rendering.
- Integrates a widget library (e.g., `egui` or a custom one) that implements the `Reconciler` trait.

## 4. Python Bindings (`atomr-view-python`)

- **PyO3 Integration**:
    - Expose `ActorSystem` (via `atomr-core` bindings if available).
    - Wrap `WindowActor` and `RegionActor` for Python subclassing.
    - Expose `SceneDescription` and `SceneNode` as Python classes.
- **Async Support**:
    - Integration with Python's `asyncio` for non-blocking actor interaction.

## 5. Implementation Roadmap

### Phase 1: Foundation (Core)
- Implement `BackendCommand`, `BackendEvent`, and `SceneDescription` types.
- Build the `UiBridge` and `UiBridgeActor`.
- Define `WindowActor` and `RegionActor` traits.

### Phase 2: Reference Backend
- Implement `WinitWgpuBackend`.
- Create a basic `egui`-based reconciler.
- Verify with a minimal Rust example.

### Phase 3: Python Bindings
- Set up `pyo3` and `maturin`.
- Expose core types and actor interfaces.
- Verify with a minimal Python example.

### Phase 4: Advanced Features
- Persistence integration (SQLite).
- Support for multiple windows.
- Conflation/Deduplication strategies in the bridge.

### Phase 5: Bevy Backend Integration (Completed)
- **BevyBackend**: Implemented a backend that integrates with Bevy's main loop.
- **Bridge Systems**: Developed `PreUpdate` and `PostUpdate` systems for command draining and event forwarding.
- **EntityActor Mirror**: Implemented the `ActorBacked` component pattern for ECS-Actor synchronization.

### Phase 6: Wasm / WebDom Backend (Completed)
- **WebDomBackend**: Implemented a Wasm-specific backend that yield to `requestAnimationFrame`.
- **Cooperative Hooks**: Established hooks for a cooperative single-threaded actor executor.

### Phase 7: Mobile (NativeShell & uniffi) (Completed)
- **NativeShellBackend**: Developed an FFI-boundary backend using `uniffi`.
- **FFI Stability**: Refactored core types (`SceneKey`, `PropertyValue`, `BackendCommand`) to be FFI-safe (using `Vec<u8>`, `String`, and `u32`).
- **uniffi Integration**: Added `setup_scaffolding!` and derives for cross-language binding generation.
