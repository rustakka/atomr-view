---
name: atomr-view-design
description: Use when authoring or modifying UI actors (WindowActor, RegionActor) in a project using atomr-view. Covers scene manipulation, input event handling, and bridge communication. Triggers on writing or editing WindowActor/RegionActor logic or scene updates.
---

# Designing atomr-view Actors

This skill helps you write idiomatic UI actors against `atomr-view`.

## Mental Model

UI actors are standard `atomr` actors that specialize in managing a
declarative `SceneDescription`.

1. **`WindowActor`**: The top-level UI actor for an OS window. It owns
   the bridge reference and is responsible for forwarding the full scene.
2. **`RegionActor`**: A child actor managing a specific part of the UI.
   It typically sends scene fragments or updates to its parent `WindowActor`.

## Scene Manipulation

Scenes are declarative trees of `SceneNode`s. Always use stable `SceneKey`s
to ensure the reconciler can match nodes across updates.

```rust
use atomr_view_core::prelude::*;

fn build_scene() -> SceneDescription {
    let root = SceneNode {
        key: SceneKey::new(),
        kind: NodeKind::Container {},
        properties: PropertyMap::default(),
        children: vec![
            SceneNode {
                key: SceneKey::new(),
                kind: NodeKind::Text {},
                properties: PropertyMap::new()
                    .with("text", PropertyValue::String { value: "Hello".to_string() }),
                children: vec![],
            }
        ],
    };
    SceneDescription { root }
}
```

## Handling Input

Input events are routed from the `UiBridgeActor` back to the relevant
`WindowActor`.

```rust
async fn handle(&mut self, ctx: &mut Context<Self>, msg: WindowMessage) {
    match msg {
        WindowMessage::BackendEvent(BackendEvent::Input { event, .. }) => {
            match event {
                InputEvent::Click { key, .. } => {
                    // Logic for clicking node with 'key'
                }
                _ => {}
            }
        }
        _ => {}
    }
}
```

## Best Practices

- **Stable Keys**: Never regenerate `SceneKey`s for existing nodes.
- **Async Bridges**: Never block the UI thread. Use `CorrelationId` for
  long-running OS operations (like file pickers).
- **Conflation**: Trust the bridge to handle backpressure. Send scenes
  whenever state changes; the bridge will drop intermediate frames.
