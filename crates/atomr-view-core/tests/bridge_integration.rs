use atomr_core::prelude::*;
use atomr_view_core::bridge::{BackendCommand, BackendEvent, UiBridgeActor, UiBridgeMessage};
use atomr_view_core::scene::{NodeKind, PropertyMap, SceneDescription, SceneKey, SceneNode};
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_bridge_conflation() {
    let (cmd_tx, mut cmd_rx) = mpsc::channel(100);
    let (_evt_tx, evt_rx) = mpsc::channel(100);

    let config = Config::reference();
    let system = ActorSystem::create("test-system", config).await.unwrap();
    let evt_rx_shared = std::sync::Arc::new(std::sync::Mutex::new(Some(evt_rx)));
    let bridge_actor = system
        .actor_of(
            Props::create(move || {
                let mut rx_guard = evt_rx_shared.lock().unwrap();
                let rx = rx_guard.take().expect("Actor restarted but receiver already taken");
                UiBridgeActor::new(cmd_tx.clone(), rx)
            }),
            "bridge",
        )
        .unwrap();

    // Send a burst of SetScene commands
    for _ in 0..10 {
        let scene = SceneDescription {
            root: SceneNode {
                key: SceneKey::new(),
                kind: NodeKind::Text {},
                properties: PropertyMap::new(),
                children: vec![],
            },
        };
        bridge_actor.tell(UiBridgeMessage::Command(BackendCommand::SetScene {
            window_id: "main".to_string(),
            scene,
        }));
    }

    // Wait a bit for the actor to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Flush pending scenes
    let mut received_count = 0;
    while let Ok(cmd) = cmd_rx.try_recv() {
        if let BackendCommand::SetScene { .. } = cmd {
            received_count += 1;
        }
    }

    assert!(received_count > 0);
}

#[tokio::test]
async fn test_bridge_routing() {
    let (cmd_tx, _cmd_rx) = mpsc::channel(100);
    let (evt_tx, evt_rx) = mpsc::channel(100);

    let config = Config::reference();
    let system = ActorSystem::create("test-system", config).await.unwrap();
    let evt_rx_shared = std::sync::Arc::new(std::sync::Mutex::new(Some(evt_rx)));
    let bridge_actor = system
        .actor_of(
            Props::create(move || {
                let mut rx_guard = evt_rx_shared.lock().unwrap();
                let rx = rx_guard.take().expect("Actor restarted but receiver already taken");
                UiBridgeActor::new(cmd_tx.clone(), rx)
            }),
            "bridge",
        )
        .unwrap();

    bridge_actor.tell(UiBridgeMessage::StartEventLoop);

    evt_tx.send(BackendEvent::WindowClosed { id: "main".to_string() }).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
}
