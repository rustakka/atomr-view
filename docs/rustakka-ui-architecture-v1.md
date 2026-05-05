# `atomr-view`: Heavy Client UI Architecture for Federated atomr Systems

**Status:** Draft / RFC v1
**Scope:** UI architecture for heavy clients — what runs inside a heavy-client `ActorSystem` to drive native desktop, game-engine, web, and mobile rendering surfaces. Where the federation document covers *how* heavy clients connect to the cluster, this document covers *what they do* once connected and how the UI itself is structured as actors integrated with the local actor system and, transitively, with the federated cluster.
**Companion documents:** [`rustakka-federation-architecture-v1.md`](./rustakka-federation-architecture-v1.md) (sections referenced as `Federation §N`), [`rustakka-inference-architecture-v4.md`](./rustakka-inference-architecture-v4.md) (`Inference §N`).
**Assumes:** `Federation §7` (heavy client model), `Federation §8` (`atomr-protocol`), and `Federation §9` (cross-cutting concerns) are stable. Heavy clients run a non-cluster `ActorSystem` and hold a `ClusterClient` configured against a regional receptionist. `rustakka-streams` is available client-side with the same operator algebra as on the cluster.

---

## 1. Motivation

Three forces push the UI into the actor system rather than leaving it as a separate concern bridged to the actor system through ad-hoc callbacks:

- **Remote state authority.** Most state worth rendering in a serious UI lives somewhere else — a sharded entity in the cluster, a peer DC, an external API, an on-device inference deployment registered with the model registry (`Federation §7.5`). The path from "remote state changes" to "pixels update" should not require gluing two unlike runtimes together with bespoke callback chains, hand-rolled debouncers, and ad-hoc reconnect logic. The actor model already solves the location-transparency problem; extending it to the UI keeps the same primitives in play end-to-end.
- **Lifecycle volatility.** Connections drop. Devices background. Phones reboot. Users navigate away and come back hours later. The discipline that makes federation work — supervised sessions (`Federation §7.2`), bounded mailboxes, backpressure (`Federation §7.4`), persisted session state (`Federation §7.6`), schema versioning (`Federation §9.1`) — is exactly the discipline that makes UI work under these conditions. Reusing it is cheaper than reinventing it.
- **Multiple rendering surfaces.** A serious system in 2026 has at least three of: native desktop (GTK on Linux, winit+wgpu cross-platform), game engines (Bevy), browser (wasm), iOS, and Android. They have different runtime shapes and different idiomatic UI models. They should not have different application architectures. One actor decomposition; many backends.

The strategic shape:

- **A `UiBackend` strategy** is the only platform-specific surface. Everything above it is shared Rust.
- **A `UiBridge`** marshals between the actor system's threading model and the rendering thread's affinity constraints, with the same bounded-mailbox + backpressure discipline that federation uses on the WAN edge.
- **`WindowActor`s and `RegionActor`s** decompose the UI at the grain where state is actually addressable — by remote actors, by supervision, by persistence. Stateless widgets are not actors; they are leaves of a `SceneDescription`.
- **Mobile is a first-class target via two flows**: a uniffi-bridged Rust core under a native UI shell (default for app-shaped applications), or pure-Rust UI via Bevy / winit+wgpu / Slint (default for games and creative tools).

The unifying mental model from the federation document holds: **the rendering backend is just another peer with network-shaped failure modes.** A slow GPU, a dropped frame, a backgrounded tab, a gesture system that loses events under pressure — quantitatively different from a WAN partition, qualitatively the same. The same actor decomposition, the same backpressure discipline, the same circuit breakers, pointed at one more place.

---

## 2. The Three Runtime Shapes

Before any architecture commitments, the three target runtime shapes must be named precisely. They are not three flavors of the same thing; they are three different ideas about who owns the schedule, where state lives, and what "the application" even is. The actor pattern accommodates all three, but how it sits inside each is genuinely different.

### 2.1 Windowed application (GTK, winit+wgpu, native desktop)

**Mental model: the `ActorSystem` is the program; windows are views the program offers.**

- A long-lived OS process you fully control. `main()` constructs the `ActorSystem` and the UI backend; both run until quit.
- The OS gives you a "main thread" where windowing must happen, plus permission to spawn arbitrary additional threads. The actor system runs a multi-threaded executor on its own threads; the bridge marshals to the main thread for backend calls.
- Multiple OS windows in a single process is normal (think VS Code, Slack). Each gets a `WindowActor`; they share the actor system, the `ClusterClient`, and the local state subgraph.
- Filesystem, raw sockets, OS clipboard, system tray, file pickers, native menus — all available. Each is exposed as an additional actor protocol (`ClipboardActor`, `FilesystemActor`, `TrayActor`).
- Lifecycle is boring: launched, runs, quits. Suspend/resume from laptop sleep is the only oddity, and most code does not need to care.

State authority lives in actors. The UI is one peripheral subsystem; the network is another; persistence is another.

### 2.2 Bevy application

**Mental model: the ECS `World` is the program's per-frame state; actors hold the cross-frame, cross-process, cross-network state.**

This is the runtime shape that inverts the most. Bevy is built around an ECS world plus a fixed-tick or variable-tick frame schedule. The frame *is* the heartbeat. `App::run()` wants the main thread and runs systems on a scheduler you do not replace.

Two consequences for the actor pattern:

- **Authority shifts.** Per-frame state — transforms, animation curves, particle positions, camera pose, UI layout — belongs in ECS components. The frame loop reads and mutates them at full speed; you do not want a mailbox in that path. Cross-frame state — player profile, save game, multiplayer roster, economy, conversation history, AI agent memory — belongs in actors. The bridge keeps them in sync where they overlap.
- **Integration is "Bevy embeds the actor system," not the other way around.** Bevy owns the main thread; the actor runtime runs on a separate task pool. Two Bevy systems form the bridge: one drains `cmd_rx` each frame and applies `SceneDescription` patches as ECS spawn/despawn/component-update commands; the other forwards `BackendEvent`s (input, window lifecycle, frame ticks) into actor mailboxes.

This split shows up cleanly in multiplayer games: the ECS handles client-side prediction and rendering at frame rate; actors handle netcode, server-authoritative state, persistence, matchmaking, AI behavior trees that span seconds rather than frames. An optional `EntityActor` mirror pattern (Section 8.3) lets specific ECS entities carry actor-backed identity for cases that need persistence or network sync.

Bevy supports multiple windows (each is an entity in the world). The mapping from `WindowActor` to Bevy window is direct — but there is one ECS world shared across windows, not one per window.

### 2.3 Wasm application (browser)

**Mental model: the actor system is a single-threaded cooperative scheduler living inside someone else's event loop, and its job is to play nicely and survive being suspended.**

Three constraints define this shape:

- **One thread, by default.** Web Workers exist but are isolated (no shared memory unless `SharedArrayBuffer` is enabled with COOP/COEP headers, which not every host serves). The actor system runs on a custom executor that yields to microtasks and `requestAnimationFrame`. There is no `std::thread`, no multi-threaded `tokio`. Mailbox dispatch is cooperative — every actor handler must finish or `await`; nothing can block.
- **No filesystem, no raw sockets.** Persistence is IndexedDB (or OPFS); networking is WebSocket and `fetch`. This is precisely why `Federation §7.2`'s `EdgeTransport` defaults to `WebSocketSession`: it survives the wasm constraint and falls out as a reasonable default everywhere else.
- **Lifecycle is hostile.** The browser may background-throttle timers, suspend the tab, evict the page from memory, even bfcache and resume seconds or hours later. Navigation away and back can trigger a fresh wasm module load. Anything not exfiltrated to IndexedDB before suspend is gone.

Architectural implications:

- The bridge collapses logically — there is only one thread. `cmd_rx`/`evt_tx` remain useful as decoupling primitives but cost nothing to traverse.
- The cluster-side `SessionActor` (`Federation §7.2`) does the heavy lifting: a wasm tab can disappear at any time and reappear minutes later, and clean session-resume from a stable `SessionId` makes that survivable.
- Bundle size becomes a real architectural constraint. The `<2MB gzipped` target from `Federation §10` (Phase 4 exit criterion) rules out kitchen-sink dependencies. Tree-shaking is mandatory.
- A wasm reload is roughly equivalent to "the device rebooted" from the cluster's standpoint. Designing for it is the same discipline as designing for a phone reboot.

### 2.4 Comparison

| Aspect | Windowed | Bevy | Wasm |
|---|---|---|---|
| Schedule owner | `ActorSystem` | Bevy frame loop | Browser event loop |
| State authority | Actors | ECS (per-frame) + actors (cross-frame) | Actors |
| Threading | Multi-threaded executor + main thread | Bevy task pools + main thread | Single thread (cooperative) |
| Bridge model | Two MPSC queues across threads | Two Bevy systems polling each frame | Logical only — no thread crossing |
| Multiple windows | Natural; multiple `WindowActor`s | Supported; entities in one shared world | One viewport; "windows" are routes / panels |
| Persistence | Filesystem, anything | Filesystem | IndexedDB / OPFS |
| Network | Anything | Anything | WebSocket, `fetch`, WebRTC |
| Lifecycle hazards | Sleep / wake (mild) | Sleep / wake (mild) | Background throttle, tab suspend, bfcache, reload |
| `EdgeTransport` default | `WebSocketSession` or `GrpcStreaming` | `WebSocketSession` or `GrpcStreaming` | `WebSocketSession` (forced) |
| `SessionStorage` default | `PersistedSession` | `PersistedSession` | `PersistedSession` (matters most here) |

The architectural shape (`UiBackend`, `UiBridge`, `WindowActor`, `SceneDescription`) holds across all three. Defaults tighten as you move right.

---

## 3. What Belongs in the Actor System and What Does Not

The first design discipline — and the one that trips most teams — is grain. The temptation is to make every widget an actor; the result is a system swamped by mailbox overhead, supervision noise, and zero benefit. The opposite temptation is to keep the UI entirely outside the actor system and bridge with callbacks; the result is two unlike runtimes with two unlike state models that must be reconciled by hand at every boundary.

The rule:

> An actor exists where state needs to **survive backend reload**, **be supervised**, **be addressable by other actors (local or remote)**, or **bridge a thread / process / network boundary**.

Concretely:

**In the actor system:**

- Window-level state (current scene, focus, modal stack).
- Region state with persistence, network backing, or remote subscriptions (a chat composer with draft autosave; a list backed by a sharded entity; a presence indicator subscribed to a remote stream).
- Long-lived background work (sync engines, background fetches, on-device inference jobs).
- Cross-window coordination (selection state, app-wide undo stack, shared theme).
- Anything death-watchable from the cluster.

**Not in the actor system:**

- Per-frame numeric state (transforms, animation curves, particle positions, layout math).
- Pure presentational widget state (button hover, scroll offset, hover highlight) — these are leaves of `SceneDescription` and reconciled by the backend.
- Data that exists only as an intermediate of rendering (text shaping, glyph atlases, GPU buffer contents).
- Anything that would require a mailbox round-trip per frame.

The grain is "addressable units of state with cross-frame or cross-boundary lifetime." Everything finer is presentation; everything coarser is the application. The actor system sits at the application layer; the backend handles presentation.

---

## 4. Actor Decomposition

The reference decomposition for a heavy-client `ActorSystem`. Path names are conventional; the structure is the load-bearing piece.

```
ActorSystem (heavy client, non-cluster member)
│
├── /user/ui
│   ├── /bridge                       UiBridgeActor — singleton per process,
│   │                                 bidirectional channel to the UiBackend
│   ├── /input-router                 Typed pub-sub for InputEvent;
│   │                                 windows subscribe to their own ID
│   ├── /windows
│   │   ├── /chat-main                WindowActor — owns scene + child regions
│   │   │   ├── /conversation-list    RegionActor — subscribes to remote shard
│   │   │   ├── /composer             RegionActor — draft state, persisted
│   │   │   └── /presence-indicator   RegionActor — remote stream consumer
│   │   └── /settings                 WindowActor
│   ├── /clipboard                    Singleton, OS clipboard shim
│   ├── /notifications                Singleton, OS-toast / system-tray shim
│   └── /theme                        Singleton, app-wide presentation policy
│
├── /user/local
│   ├── /cache                        LocalCacheActor — RAM + persistent cache
│   ├── /persistence                  PersistenceActor — sled / SQLite / IndexedDB
│   └── /inference                    OnDeviceInferenceActor
│                                     (registered with cluster model registry,
│                                      see Federation §7.5)
│
└── /user/cluster
    ├── /client                       ClusterClient (Federation §7.1)
    └── /session                      Local handle to the cluster-side
                                      SessionActor (the durable identity
                                      lives on the cluster; this is the
                                      client-side proxy / state mirror)
```

### 4.1 `UiBridgeActor`

The single point of contact between the actor system and the UI backend. Owns the bridge channels (Section 6). Routes inbound `BackendEvent`s to subscribers (windows for window-scoped events, the input router for input). Drains outbound `BackendCommand`s from windows. Exactly one instance per process.

### 4.2 `WindowActor`

One per logical window. Owns its `SceneDescription`, its child `RegionActor`s, and its subscriptions to remote actors. Receives input events for its window and either handles them directly (focus changes, modal dismissal) or forwards them to the relevant region. Issues backend commands via the bridge.

```rust
pub struct WindowActor {
    id:        WindowId,
    bridge:    ActorRef<UiBridgeProtocol>,
    scene:     Scene,
    regions:   HashMap<RegionId, ActorRef<RegionProtocol>>,
    cluster:   ActorRef<ClusterClientProtocol>,
    // Per-window state that is not itself a region.
    focus:     Option<RegionId>,
    modals:    Vec<ModalId>,
}

pub enum WindowMessage {
    // ── from the application ────────────────────────────────
    SetScene(SceneDescription),
    PatchScene(Vec<ScenePatch>),
    RegisterRegion { id: RegionId, actor: ActorRef<RegionProtocol> },
    SetFocus(Option<RegionId>),
    PushModal(ModalSpec),
    Close,

    // ── from the bridge (BackendEvent for this window) ──────
    Input(InputEvent),
    Lifecycle(LifecycleEvent),
    BackendError(BackendError),

    // ── from local actors or remote actors via ClusterClient ─
    RemoteUpdate(RemotePayload),
    LocalUpdate(LocalPayload),

    // ── supervision ────────────────────────────────────────
    Terminated(ActorRef<RegionProtocol>),  // a region died
}
```

### 4.3 `RegionActor`

A region is a subtree of a window with non-trivial state. Regions are the unit of:

- Remote subscription (one region subscribes to one sharded entity, typically).
- Draft persistence (composer text, form state).
- Streaming consumption (event logs, telemetry feeds).
- Optimistic UI (locally applied updates with rollback on remote rejection).

Regions render into a portion of their parent window's `SceneDescription`. They never call the backend directly; they emit scene patches addressed to their parent window.

### 4.4 `OnDeviceInferenceActor` and the local-first hook

A heavy client increasingly runs local inference. Per `Federation §7.5`, the device is a one-node deployment in the federated sense: it registers with the cluster's model registry, advertises capabilities and capacity, and the cluster's hybrid pipeline machinery (`Inference §9.1`) routes to it preferentially when available.

In the UI, a `RegionActor` that wants inference does not know whether the chosen deployment lives on-device or in a peer DC. It addresses the model registry; the routing layer chooses. This is the same `LeastLatencyWithHysteresis` (`Federation §5.1`) discipline applied to the local-versus-remote question.

### 4.5 What is *not* an actor

Restating Section 3 in the decomposition's terms:

- A button is not an actor. It is a node in the parent region's scene.
- A scrollbar position is not an actor. The backend tracks it; the region asks for it on demand.
- A text glyph cache is not an actor. It is renderer-internal state.
- A per-frame animation tween is not an actor. It is a scene-level animation spec the backend drives.

If you find yourself spawning an actor per list item, per cell, per pixel — stop. The grain is wrong.

---

## 5. The `UiBackend` Strategy Pattern

The `UiBackend` is the only platform-specific surface. Everything above it is shared Rust code.

```rust
pub trait UiBackend: Send + 'static {
    /// The backend's window handle type. Opaque to actors; passed back
    /// in BackendEvent::WindowCreated for actors that need it (rare).
    type WindowHandle: Send + 'static;

    /// Takes the main thread for the lifetime of the process. The bridge
    /// is how the actor system talks back to the backend; this method
    /// does not return until the application quits.
    fn run(self, bridge: UiBridge) -> !;
}

pub enum BackendCommand {
    CreateWindow      { id: WindowId, spec: WindowSpec },
    UpdateWindow      { id: WindowId, patch: WindowPatch },
    DestroyWindow     { id: WindowId },
    SetScene          { id: WindowId, scene: SceneDescription },
    ApplyPatches      { id: WindowId, patches: Vec<ScenePatch> },
    RequestRedraw     { id: WindowId },
    SetClipboard      { content: ClipboardContent },
    ShowNotification  { spec: NotificationSpec },
    OpenFilePicker    { spec: FilePickerSpec, correlation: CorrelationId },
}

pub enum BackendEvent {
    WindowCreated     { id: WindowId, native: NativeWindowInfo },
    WindowClosed      { id: WindowId, reason: CloseReason },
    Input             { id: WindowId, event: InputEvent },
    Lifecycle         { event: LifecycleEvent },
    FrameTick         { now: Instant, since_last: Duration },
    ClipboardChanged  { content: ClipboardContent },
    FilePickerResult  { correlation: CorrelationId, outcome: FilePickerOutcome },
    BackendError      { id: Option<WindowId>, error: BackendError },
}
```

### 5.1 Default and alternatives

```rust
pub struct WinitWgpuBackend;     // default — cross-platform native
pub struct GtkBackend;           // Linux desktop, deep GNOME integration
pub struct BevyBackend;          // game engines, ECS-shaped applications
pub struct WebDomBackend;        // wasm in browser, DOM reconciliation
pub struct NativeShellBackend;   // mobile — uniffi to SwiftUI / Compose
pub struct SlintBackend;         // declarative UI toolkit, mobile-capable
```

**Default:** `WinitWgpuBackend` paired with a retained-ish widget layer (egui or iced) or a custom renderer. Reasons:

- Cross-platform: Windows, macOS, Linux, iOS, Android, web.
- Does not lock the application into a specific UI framework's lifecycle model.
- Owns only the windowing and rendering primitives — leaves widget choice open.
- Active ecosystem; well-supported by `wgpu` for GPU access on every platform.

**Alternatives, with the situation each is right for:**

- `GtkBackend`: when deep GNOME integration matters (system styling, accessibility, settings schema, GIO file pickers) and the deployment target is Linux desktop. Not portable to mobile.
- `BevyBackend`: when the application is fundamentally a game or a real-time simulation. The ECS is the better state model for that workload (see Section 8.3).
- `WebDomBackend`: for wasm targets where reconciling against the DOM is preferable to drawing into a `<canvas>` — typically for content-heavy apps where browser accessibility, copy/paste, find-on-page, and screen readers matter.
- `NativeShellBackend`: for mobile applications where native look-and-feel is non-negotiable. See Section 9.
- `SlintBackend`: for embedded-class UI on capable hardware (kiosks, infotainment, industrial HMIs) with strong declarative-UI requirements.

### 5.2 What is *not* configurable

The `UiBackend` trait itself, the `BackendCommand` / `BackendEvent` envelopes, and the bridge protocol (Section 6) are **not** strategy-pluggable. They are the contract that makes the rest of the architecture portable. New backends implement the trait; they do not get to add new envelope variants. New variants require a versioned protocol bump under `Federation §9.1`'s schema discipline.

---

## 6. The `UiBridge` — Actor World ↔ UI Thread

The bridge is the load-bearing piece. The UI backend owns the main thread, so the actor system cannot `block_on` it. Conversely, GTK / Bevy / the browser cannot `await` on actor mailboxes from inside their event loops. The bridge resolves this with two bounded queues plus an explicit dispatcher actor.

```rust
pub struct UiBridge {
    /// Actor → backend. Drained by the backend each tick / frame.
    pub(crate) cmd_rx: BoundedReceiver<BackendCommand>,
    /// Backend → actor. Pushed by backend; dispatched by UiBridgeActor.
    pub(crate) evt_tx: BoundedSender<BackendEvent>,
}

/// Lives inside the atomr ActorSystem. Bridges events into the supervision
/// tree, exposes a typed ActorRef interface to the rest of the system.
pub struct UiBridgeActor {
    cmd_tx:        BoundedSender<BackendCommand>,
    evt_rx:        BoundedReceiver<BackendEvent>,
    window_routes: HashMap<WindowId, ActorRef<WindowMessage>>,
    input_router:  ActorRef<InputRouterProtocol>,
    correlations:  HashMap<CorrelationId, ActorRef<dyn Any + Send>>,
}
```

### 6.1 Backpressure discipline

Mirroring `Federation §7.4` exactly. Two strategies, both bounded, neither unbounded.

**Actor → backend (`cmd_rx`):**

| Command kind | Strategy | Rationale |
|---|---|---|
| `SetScene`, `ApplyPatches` | `conflate` keyed on `window_id` | Slow UI thread should see the latest scene, not every intermediate |
| `CreateWindow`, `DestroyWindow`, `OpenFilePicker` | `BackPressure` | Lifecycle events; never drop |
| `RequestRedraw` | `dedupe` per window | Multiple redraw requests in one frame collapse to one |
| `ShowNotification` | `BackPressure` w/ bounded queue | User-visible; matters; bound it |

**Backend → actor (`evt_tx`):**

| Event kind | Strategy | Rationale |
|---|---|---|
| `Input` | `BackPressure` | Dropping input is unforgivable; if the actor system is overrun, the user feels frame lag (correct signal) |
| `FrameTick` | `conflate` (latest only) | Intermediate ticks are uninteresting; latest wins |
| `Lifecycle` | `BackPressure` | Lifecycle is sacred; never drop |
| `WindowClosed` | `BackPressure` | Cleanup must run |
| `BackendError` | `BackPressure` w/ supervision escalation | Errors get to the supervisor |

The default that almost everything else inherits is `BackPressure` with a bounded queue. The conflation cases are explicit opt-ins. **Unbounded queues are disallowed at the bridge** — same rule as the receptionist's inbound mailbox in `Federation §3.1`.

### 6.2 Scheduling

The bridge's drain rate matches the backend's clock:

- **Windowed (winit / GTK):** drained on every event-loop iteration. With vsync, that is roughly 60–144 Hz; without, whenever an event arrives plus a periodic wake.
- **Bevy:** drained by a system in the `PreUpdate` schedule each frame.
- **Wasm:** drained from the `requestAnimationFrame` callback (~60 Hz when foregrounded, throttled or paused when backgrounded).

The actor system does not need to know which mode it is in. The bridge's `BackendCommand` queue is filled at whatever rate actors produce; the backend pulls when it pulls. Conflation absorbs mismatch.

### 6.3 No synchronous calls across the bridge

Critical discipline: **no command issued by an actor blocks waiting for a backend response.** Anything that needs a response (file picker, native dialog, capability query) issues a command with a `CorrelationId`; the backend emits a corresponding event when ready; `UiBridgeActor` routes it back to the originating actor.

This is the same `ask` pattern with explicit correlation that `ClusterClient` uses across the WAN. Same reasoning: a synchronous wait would block an actor on a frame budget, which couples actor-system throughput to UI-thread health. Decoupling is non-negotiable.

---

## 7. `SceneDescription` and Reconciliation

`SceneDescription` is the abstraction that lets one actor protocol drive GTK, Bevy, the DOM, and a `winit+wgpu` widget tree without a universal-widget abstraction.

The shape: a declarative tree of nodes with properties, addressed by stable paths. Producers (actors) emit either a full `SetScene` or incremental `ScenePatch`es. Consumers (backends) reconcile against their native representation — a retained widget tree, an ECS subworld, the DOM, a custom render graph.

```rust
pub struct SceneDescription {
    pub root: SceneNode,
}

pub struct SceneNode {
    pub key:        SceneKey,        // stable identity for diffing
    pub kind:       NodeKind,        // semantic role (text, image, container, input, …)
    pub properties: PropertyMap,     // typed attributes (text, color, layout, callbacks …)
    pub children:   Vec<SceneNode>,
}

pub enum ScenePatch {
    Insert    { parent: ScenePath, index: usize, node: SceneNode },
    Remove    { path: ScenePath },
    Move      { from: ScenePath, to: ScenePath },
    SetProp   { path: ScenePath, key: PropKey, value: PropValue },
    Replace   { path: ScenePath, with: SceneNode },
}
```

### 7.1 Why not a universal widget abstraction

Every cross-platform UI library that has tried to define a universal widget set has either leaked the platform's idioms ("looks fine on the platform it was designed for; uncanny everywhere else") or paved over them ("looks identically mediocre everywhere"). Both are bad outcomes.

`SceneDescription` stops at *semantic intent* (`NodeKind::TextInput`, `NodeKind::List`, `NodeKind::Container`) rather than concrete widgets. The backend chooses how to realize that intent natively. A `TextInput` becomes a `gtk::Entry`, a `bevy_ui::TextInput`, an `<input>`, or an iced `text_input` — each looks right on its platform.

### 7.2 Reconciliation strategy

```rust
pub trait Reconciler: 'static {
    type NativeTree;
    fn apply_full(&mut self, tree: &mut Self::NativeTree, scene: &SceneDescription);
    fn apply_patches(&mut self, tree: &mut Self::NativeTree, patches: &[ScenePatch]);
}
```

Each backend implements `Reconciler` for its native tree type. The actor side stays unaware — it produces patches; the backend reconciles.

**Default reconciliation algorithm**: keyed diff. `SceneKey` is the identity (an opaque value that is stable across frames for the same logical entity). Reconcilers match by key first, fall back to position, and prefer minimal mutation.

**Dismissed:** reconciling by position only (same trap as React without `key`s — moves become destroy-and-recreate, losing focus and animation state). Position-only reconciliation is fast and wrong; keyed reconciliation is the only acceptable default.

### 7.3 Property mapping

Properties are typed but the type set is open. A `Color` property maps to `gtk::CssProvider` rules in GTK, to a `bevy::prelude::Color` component in Bevy, to `style.color` in the DOM. Backends that do not understand a property ignore it (with a debug-build warning). Backends may extend the property set with backend-specific keys; portable code does not use them.

---

## 8. Backend Implementations

### 8.1 `WinitWgpuBackend` (default)

Owns a `winit::EventLoop` on the main thread. Each window is a `winit::Window` paired with a `wgpu::Surface`. `SceneDescription` is reconciled against an in-process retained widget tree (egui or iced) which does its own draw into the wgpu surface.

The bridge runs in the event-loop callback:

```rust
event_loop.run(move |event, target| {
    while let Some(cmd) = bridge.cmd_rx.try_recv() {
        backend.apply(cmd);
    }
    match event {
        Event::WindowEvent { window_id, event, .. } => {
            let _ = bridge.evt_tx.try_send(BackendEvent::Input { /* ... */ });
        }
        Event::AboutToWait => {
            // redraw any windows that requested it
        }
        // ...
    }
});
```

Cross-platform reach: Windows, macOS, Linux, iOS, Android, web (via `winit`'s wasm support, with caveats — see Section 8.4).

### 8.2 `GtkBackend`

Holds a `gtk4::Application` and runs `glib::MainContext::default().iteration()`. The bridge drains on each iteration via a `glib::source` that wakes when `cmd_rx` has data. Reconciliation maintains a `HashMap<ScenePath, gtk::Widget>` and applies diffs as widget construction, property setting, and tree mutation.

Input is captured via GTK signal handlers; each handler closure holds a clone of `evt_tx` and emits `BackendEvent::Input`. The non-`Send` GTK widget refs never leave the main thread; the actor side never sees them.

Trade-offs:

- Best-in-class GNOME desktop integration: native theming, accessibility, file pickers, settings schema.
- Linux-only at scale. (GTK technically runs on macOS and Windows; in practice, the experience is poor and the deployment story is painful.)
- Not viable on iOS or Android.

Use when the deployment is Linux desktop and native GNOME integration is a requirement.

### 8.3 `BevyBackend`

The most architecturally distinct backend. Bevy owns the main thread; the actor runtime runs on a separate task pool (Bevy's own `ComputeTaskPool` or a dedicated `tokio::runtime::Runtime`). Two Bevy systems form the bridge:

```rust
fn drain_actor_commands(
    mut commands: Commands,
    bridge: NonSend<UiBridgeReceiver>,
    mut scenes: Query<&mut SceneState>,
) {
    while let Ok(cmd) = bridge.cmd_rx.try_recv() {
        match cmd {
            BackendCommand::SetScene { id, scene } => apply_scene(&mut commands, id, scene),
            BackendCommand::ApplyPatches { id, patches } => apply_patches(&mut commands, id, patches),
            // ...
        }
    }
}

fn forward_backend_events(
    bridge: NonSend<UiBridgeSender>,
    mut keyboard: EventReader<KeyboardInput>,
    mut mouse:    EventReader<MouseButtonInput>,
    mut window:   EventReader<WindowEvent>,
) {
    for ev in keyboard.read() {
        let _ = bridge.evt_tx.try_send(BackendEvent::Input { /* ... */ });
    }
    // ...
}
```

These run in `PreUpdate` and `PostUpdate` respectively, framing every Bevy frame.

#### `EntityActor` — the optional mirror pattern

For entities that need persistent identity, network sync, or actor-style supervision, a mirror pattern lets one ECS entity be backed by one actor.

```rust
#[derive(Component)]
pub struct ActorBacked {
    pub actor:        ActorRef<EntityProtocol>,
    pub last_synced:  Tick,
}

fn sync_actor_to_components(
    actors: Res<ActorRegistry>,
    mut q:  Query<(&ActorBacked, &mut Transform, &mut Health)>,
) {
    for (backed, mut tf, mut hp) in &mut q {
        if let Some(snapshot) = actors.try_snapshot(&backed.actor) {
            *tf = snapshot.transform;
            *hp = snapshot.health;
        }
    }
}

fn sync_components_to_actor(
    actors:   Res<ActorRegistry>,
    q:        Query<(&ActorBacked, &Transform, &Health), Changed<Transform>>,
) {
    for (backed, tf, hp) in &q {
        backed.actor.tell(EntityMessage::ApplySnapshot { transform: *tf, health: *hp });
    }
}
```

This is the natural shape for multiplayer game state: per-frame physics and rendering live in ECS components; cross-frame, cross-process state (player profile, inventory persistence, server-authoritative position, AI behavior over time) lives in actors. The actor side handles netcode via the `ClusterClient`; the ECS stays focused on rendering and tight per-frame simulation.

The cost: a per-frame snapshot copy. Mitigations: only mirror entities that need it (it is opt-in via the `ActorBacked` marker component); use `Changed<T>` filters to avoid syncing untouched data; batch multi-component updates per actor.

**Dismissed:** mirroring *every* entity as an actor. The actor system would drown in mailbox churn for entities that have no cross-frame meaning. The mirror pattern is for entities with identity that outlives the frame.

### 8.4 `WebDomBackend` (wasm)

Single-threaded by platform constraint. The bridge degenerates to a `Rc<RefCell<VecDeque<…>>>` (no thread crossing, no atomic synchronization). `cmd_rx` drains from a `requestAnimationFrame` callback installed via `wasm-bindgen`.

Reconciliation against the DOM directly is feasible but hostile (manual `Document::create_element`, attribute setting, child mutation). The recommended pattern is to embed into an existing reactive framework (Yew, Leptos, Dioxus) and let it handle DOM reconciliation; the backend then maps `SceneDescription` to that framework's component tree.

Constraints:

- Bundle-size budget per `Federation §10` Phase 4: <2MB gzipped. Tree-shaking is mandatory; large dependencies cost real money in load time.
- All persistence is async (IndexedDB / OPFS). The local persistence actor uses one of these via `wasm-bindgen-futures`.
- Networking is `WebSocket` (for the `ClusterClient`'s session) and `fetch` (for one-off REST calls if needed). No raw TCP, no HTTP/2 streams in browser-native form.
- Lifecycle is hostile (`Section 2.3`). The cluster-side `SessionActor` is essential.

### 8.5 `NativeShellBackend` (mobile)

Covered in detail in Section 9. The backend implementation is thin — it forwards `BackendCommand`s across a uniffi boundary to a native UI layer (SwiftUI on iOS, Compose on Android), and forwards platform events back. The native shell does the rendering; the backend is just the FFI marshaller.

---

## 9. Mobile Architecture

Rust runs on both major mobile platforms. iOS targets: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`, packaged as a static library or `.xcframework` linked from Xcode. Android targets: `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`, `i686-linux-android`, built with `cargo-ndk` and dropped in `jniLibs/` for JVM `dlopen`.

The architectural choice on mobile is between two flows. Both are first-class; the choice depends on the application shape.

### 9.1 Flow A — Native shell + Rust core (default for app-shaped applications)

The dominant pattern for mobile applications that are broadly app-shaped: chat, productivity, finance, social, content. The native UI shell handles screens, navigation, gestures, and platform conventions; the Rust core handles state, networking, persistence, on-device inference, and federation.

```
┌─────────────────────────────────────────────────┐
│ MOBILE APP                                      │
│                                                 │
│ ┌─────────────────────────────────────────┐     │
│ │ Native UI layer                         │     │
│ │ (SwiftUI on iOS, Compose on Android)    │     │
│ │  - Screens, navigation, gestures        │     │
│ │  - Subscribes to state from the core    │     │
│ │  - Sends user actions into the core     │     │
│ │  - Owns accessibility, dark mode,       │     │
│ │    dynamic type, haptics, system fonts  │     │
│ └────────────────┬────────────────────────┘     │
│                  │ uniffi bindings              │
│ ┌────────────────▼────────────────────────┐     │
│ │ Rust core (.xcframework / .so)          │     │
│ │  ─────────────────────────────────      │     │
│ │  ActorSystem (single-threaded executor  │     │
│ │    or limited multi-threaded)           │     │
│ │  WindowActor / RegionActor tree         │     │
│ │  NativeShellBackend (UiBackend impl)    │     │
│ │  Local cache + persistence (SQLite)     │     │
│ │  ClusterClient + SessionActor proxy ────┼─────┼─── WSS ──► Receptionist
│ │  OnDeviceInferenceActor                 │     │
│ └─────────────────────────────────────────┘     │
└─────────────────────────────────────────────────┘
```

In the strategy pattern this is `NativeShellBackend`. `BackendCommand`s become Swift / Kotlin function calls or observable updates published to the native side; `BackendEvent`s come back through callbacks or async streams. The native side performs `SceneDescription` reconciliation against SwiftUI's `View` tree or Compose's `Composable` tree — and crucially, you do not write a custom renderer. The platform handles everything that should feel native (system fonts, accessibility, dark mode, dynamic type, haptics, native gestures).

#### FFI boundary discipline

`uniffi` is the recommended bridge generator for an architecture this size. It produces idiomatic Swift and Kotlin bindings from a single Rust definition, supports async functions, and handles the marshalling of strings, collections, and structured types.

```rust
#[uniffi::export]
impl RustakkaCore {
    pub fn new(config: ClientConfig) -> Arc<Self> { /* ... */ }
    pub fn connect(self: Arc<Self>) -> Result<(), ClientError> { /* ... */ }
    pub fn observe(self: Arc<Self>, cb: Box<dyn StateCallback>) { /* ... */ }
    pub fn send_action(self: Arc<Self>, action: UserAction) { /* ... */ }
}

#[uniffi::export(callback_interface)]
pub trait StateCallback: Send + Sync {
    fn on_scene_update(&self, window: WindowId, patch: ScenePatchPayload);
    fn on_lifecycle(&self, event: LifecyclePayload);
}
```

The discipline: **only the bridge protocol crosses the FFI**. The native side does not see actor refs, mailboxes, or runtime types. It sees `SceneDescription` updates and sends `UserAction` values back. The same shape that crosses a thread boundary crosses the FFI.

**Dismissed FFI choices:**

- Raw `jni` crate on Android: works, but tedious to maintain at architecture scale; uniffi generates the same bindings with less code.
- `cbindgen` + manual Swift / Kotlin wrappers: viable for tiny APIs; an architecture this size would re-invent uniffi badly.
- Embedding a JavaScript runtime to bridge: catastrophic — adds a third runtime, multiplies the lifecycle hazards, and produces worse type safety than a direct FFI.

Two viable alternatives for narrower cases:

- **`swift-bridge`** when the iOS-side ergonomics matter more than Android parity (more idiomatic Swift output than uniffi).
- **`flutter_rust_bridge`** when the team has already committed to Flutter as the UI layer.

### 9.2 Flow B — Pure Rust UI (default for games, AR/VR, creative tools)

For applications where you control the look anyway, `winit` and `wgpu` both run on iOS and Android, and Bevy supports both (iOS is stable; Android has more rough edges that improve with each release). You ship a single Rust binary; the OS sees a regular app whose `main` happens to be Rust.

Other pure-Rust UI options viable on mobile:

- **Slint** — declarative, lightweight, deliberate mobile story.
- **Dioxus** — React-like; mobile via native renderer or webview.
- **Tauri 2** — web frontend + Rust backend, full mobile support since 2024.

Each implements a different `UiBackend` (`WinitWgpuBackend`, `SlintBackend`, `BevyBackend`, etc.). The actor system sits behind all of them unchanged.

The cost of pure Rust on mobile is owning the platform-lifecycle adapters yourself: `applicationDidEnterBackground`, Android's `onPause` / `onStop`, low-memory warnings, audio session interruptions, scene phase transitions on iPad. Native shell flow gives you these for free; pure Rust requires owning them.

For games this is acceptable because you would own them anyway. For productivity apps it is usually a poor trade.

### 9.3 Recommendation

| Application shape | Default flow |
|---|---|
| Chat, productivity, finance, social, content | Flow A — `NativeShellBackend` + uniffi |
| Games, AR/VR, custom-rendered creative tools | Flow B — `BevyBackend` or `WinitWgpuBackend` |
| Mixed (game with chat, app with embedded 3D) | Flow A primary; embed a Bevy or wgpu sub-view inside a native screen |

The "mixed" case is the underrated option. SwiftUI and Compose both support hosting a custom view backed by Metal / Vulkan; you can run a Bevy or wgpu surface inside a native screen for the parts that need it. The actor system sees one `WindowActor` either way; the backend handles the multi-renderer composition.

### 9.4 Mobile lifecycle changes the actor design more than expected

Three things matter, and skipping any one of them produces an app that "works in the simulator" and breaks on real hardware:

**Suspend / Doze.** iOS suspends backgrounded apps within seconds; Android Doze and App Standby do similar things. The `ActorSystem`'s executor stops running. The `WebSocketSession` to the receptionist is killed by the OS within a minute or so. This is precisely what `Federation §7.2`'s `SessionActor` plus `Federation §7.6`'s `PersistedSession` are designed for: the *connection* dies, the *session* survives on the cluster side, the client resumes by `SessionId` on next foreground. **Treat every backgrounding as a clean disconnect; treat every foregrounding as a reconnect.** Do not fight it.

**Memory eviction.** Backgrounded apps may be killed entirely under memory pressure. Anything not persisted is lost. In-flight actor mailboxes vanish. The discipline: **the device-side `ActorSystem` is not a source of truth for anything that matters.** The cluster-side `SessionActor` (event-sourced) holds durable state; the device is a cache that can rebuild from session resume.

**Network volatility.** Cellular ↔ Wi-Fi ↔ airplane-mode ↔ tunnel-with-no-signal happens constantly. The reconnect-with-jitter policy from `Federation §7.2` (base 1s, max 60s, ±25% jitter) is mandatory, not optional, on mobile. The 30-second app-level heartbeat default is right for cellular; on Wi-Fi 10s is acceptable.

**Push notifications** belong in the architecture but not in the actor system on the device. When the app is suspended, the cluster cannot reach it directly; it pushes through APNs (iOS) or FCM (Android), the OS wakes the app, the app reconnects its session. APNs / FCM are not actor-shaped. They are modeled as a `NotificationGateway` bridge inside the cluster that the relevant `SessionActor` calls when it has a high-priority event for an offline client. The protocol is one-way: the push tells the device "reconnect"; actual content arrives over the resumed session, not in the push payload.

```rust
pub trait MobileLifecyclePolicy: Send + Sync + 'static {
    fn on_background(&self) -> BackgroundAction;
    fn on_foreground(&self) -> ForegroundAction;
    fn on_memory_warning(&self) -> MemoryWarningAction;
}

pub struct AggressiveSuspend;        // default — disconnect on background, reconnect on foreground
pub struct StaySilentlyConnected;    // alt — for short backgrounds (< 10s), e.g., picker dialogs
pub struct OfflineFirstHybrid;       // alt — for apps that should function fully offline
```

**Default:** `AggressiveSuspend`. Disconnects the session on background, reconnects on foreground. Aligns with platform expectations and battery budget.

**Alternatives:**

- `StaySilentlyConnected` for backgrounds known to be brief (file picker, share sheet, biometric prompt). Detected by the platform; configured per known transition.
- `OfflineFirstHybrid` for apps that genuinely must work offline. Adds a local write-ahead log that drains to the cluster on reconnect. Not the default because it adds significant complexity (conflict resolution, optimistic UI rollback) that most apps do not need.

**Dismissed:** keeping the WebSocket open through arbitrary backgrounds. Both platforms will kill it; pretending otherwise produces stale-connection bugs and wasted retries.

---

## 10. Cross-Cutting Concerns

### 10.1 Backpressure composition end-to-end

The architectural payoff of using one set of primitives across the federation, the bridge, and the rendering pipeline is that backpressure composes. A user on a slow cellular link with an underpowered GPU sees the freshest possible state at whatever rate the slowest stage allows, with no unbounded buffering at any stage.

The pipeline, in order:

```
Remote ConversationActor (cluster, 3 DCs away)
        │  rustakka-streams: groupedWithin + conflate
        ▼
Cluster-side SessionActor
        │  WebSocket, Federation §7.4 backpressure
        ▼
Heavy-client ClusterClient
        │  bounded mailbox; ask/tell with backpressure to local actors
        ▼
WindowActor
        │  emits BackendCommand::ApplyPatches
        ▼
UiBridge (cmd_rx, conflate keyed on window_id)
        │  drained per frame
        ▼
UiBackend reconciler
        │  applies patches to native tree
        ▼
GPU / OS compositor
        │  vsync
        ▼
User
```

Every arrow has bounded buffering and a defined overflow policy. The slowest arrow paces the whole pipeline. When the GPU stutters, conflation absorbs intermediate scene patches; when the WebSocket is slow, the cluster-side stream operator absorbs intermediate updates; when the cluster is slow, the receptionist's per-peer rate limit (`Federation §3.1`) provides backpressure to upstream producers. **No stage waits; no stage drops silently; no stage buffers without bound.**

### 10.2 Local-first capability

A heavy client increasingly runs its own inference. Per `Federation §7.5`, the device registers as a one-node deployment in the federated sense. The UI side of this is straightforward:

- `OnDeviceInferenceActor` registers with the cluster's model registry on connect, advertises capabilities (`{whisper-small, embeddings-bge-small, llama-1b}`), capacity (typically 1), and scope (device-only).
- `RegionActor`s that need inference address the model registry, not specific deployments. Routing is handled by the `LeastLatencyWithHysteresis` strategy from `Federation §5.1`. The device is preferred when available; remote deployments are tried otherwise.
- When the device is offline, the device-deployment is marked unreachable; remote deployments handle the load. When the device comes back, latency-based routing reconverges to local preference.

The same pipeline, the same routing logic, the same fallback machinery from `Inference §9` — pointed inward.

### 10.3 Persistence

```rust
pub trait LocalPersistence: Send + Sync + 'static {
    fn save<T: Persistable>(&self, key: PersistKey, value: &T) -> BoxFuture<'static, Result<(), PersistError>>;
    fn load<T: Persistable>(&self, key: PersistKey) -> BoxFuture<'static, Result<Option<T>, PersistError>>;
    fn delete(&self, key: PersistKey) -> BoxFuture<'static, Result<(), PersistError>>;
}

pub struct SqlitePersistence;        // default for desktop and mobile
pub struct IndexedDbPersistence;     // default for wasm
pub struct SledPersistence;          // alt — embedded KV, no SQL needed
pub struct InMemoryPersistence;      // dev / test only
```

**Default (desktop + mobile):** `SqlitePersistence`. Universally available, well-understood, transactional, queryable. The right choice for anything beyond pure KV access.

**Default (wasm):** `IndexedDbPersistence`. The browser-native option. OPFS is an alternative for larger blobs but has narrower browser support.

**Alternatives:**

- `SledPersistence` for embedded KV use cases where SQL is overkill.
- `InMemoryPersistence` strictly for development and tests.

**Dismissed:** writing to the filesystem with bespoke binary formats. Reinventing transactions, schema migration, and recovery is not worth the code saved. Use SQLite.

The `RegionActor`s that need persistence (composer drafts, form state, locally-cached query results) write through the `LocalPersistence` actor. The persistence actor batches writes to amortize disk syncs; default batch window 100ms, configurable per region.

### 10.4 Remote integration — concretely

The integration story between a `WindowActor` and the cluster is the demonstration that this whole design is worth the trouble. A chat window backed by a remote sharded `ConversationActor`:

```rust
// In ChatWindowActor::started()
let conv = self.cluster.ask(receptionist::resolve(
    "/user/conversations/sharded",
    ConversationKey(self.conv_id),
)).await?;

self.context.watch(&conv);                           // death-watch across the WAN
conv.tell(Subscribe { from_offset: self.last_offset }, self.self_ref());

// In ChatWindowActor::receive()
match msg {
    WindowMessage::RemoteUpdate(MessageAppended { msg, offset }) => {
        self.last_offset = offset;
        let patch = self.scene.append_message(&msg);
        self.bridge.tell(BackendCommand::ApplyPatches {
            id: self.id,
            patches: vec![patch],
        });
    }
    WindowMessage::Terminated(ref_) if ref_ == conv => {
        // Conversation entity died on the cluster side.
        // Show offline state, queue a re-subscribe with backoff.
        self.scene.set_banner(Banner::Reconnecting);
        self.context.schedule_once(Duration::from_secs(2), || ResubscribeConversation);
    }
    WindowMessage::Input(InputEvent::Submit { text }) => {
        // Optimistic local update + remote send.
        let temp_id = self.scene.append_pending_message(&text);
        conv.tell(AppendMessage { text, client_temp_id: temp_id });
    }
}
```

The window does not know that `conv` lives in another DC. Same `tell`, same `watch`, same `Terminated`. The optimistic local update with rollback-on-failure pattern composes naturally because the actor model gives exact-once-handler semantics on both the local and remote side. Backpressure on the inbound stream of `RemoteUpdate`s composes through the conflate operator at the bridge: a slow render does not let updates pile up in unbounded form.

This is the strategic payoff. The UI architecture *is* the federation architecture, applied one layer further out.

### 10.5 Observability

All metrics ride the same pipeline as `Federation §9.3`. UI-specific additions:

- **Frame budget metrics:** per-window p50/p95/p99 frame time, dropped-frame rate, scene-patch apply latency.
- **Bridge metrics:** `cmd_rx` and `evt_tx` queue depths, conflation rate (how often a queued command was dropped in favor of a newer one), backpressure events.
- **Reconciliation metrics:** patches per frame, full-tree replacements per minute (high values indicate poor key stability — a leading indicator of UI bugs).
- **Mobile lifecycle metrics:** background / foreground transition counts, time-to-reconnect after foreground, session resume success rate.

W3C Trace Context propagates from input event through actor chain to remote call to render. A slow user-visible action shows the full path from button press through the network and back.

### 10.6 Schema versioning at the FFI

The mobile flow's FFI boundary is a federation edge under `Federation §9.1`. The native UI layer and the Rust core may be on different versions (the Rust core ships with the app build; OTA updates of one without the other are possible in some deployment models). The discipline:

- The FFI surface is versioned. Native code declares a minimum supported core version.
- `BackendCommand` and `BackendEvent` envelopes carry a schema version.
- Adding new variants is minor; removing or changing semantics is major.
- Breaking changes require coordinated app-store releases of both layers.

For pure-Rust flows (Bevy, winit+wgpu), there is no FFI — the version skew problem reduces to "is this binary self-consistent," which the build system enforces.

---

## 11. Strategy Pattern Catalogue

Tabular summary of every architectural decision exposed as a strategy in this document, with defaults.

| Decision | Trait | Default | Fallback / Alt | Dismissed |
|---|---|---|---|---|
| UI backend | `UiBackend` | `WinitWgpuBackend` | `GtkBackend` (Linux desktop), `BevyBackend` (games), `WebDomBackend` (wasm), `NativeShellBackend` (mobile), `SlintBackend` | "One actor per widget" granularity; universal-widget abstraction |
| Mobile flow | (deployment-time choice) | Flow A — `NativeShellBackend` + uniffi | Flow B — pure Rust (`BevyBackend`, `WinitWgpuBackend`, `SlintBackend`) | GTK on mobile; embedded JS bridge |
| FFI binding generator | (build-time choice) | `uniffi` | `swift-bridge` (iOS-only ergonomics), `flutter_rust_bridge` (Flutter teams) | Raw `jni` at architecture scale; `cbindgen` + manual wrappers |
| Mobile lifecycle | `MobileLifecyclePolicy` | `AggressiveSuspend` | `StaySilentlyConnected` (brief backgrounds), `OfflineFirstHybrid` (offline-required) | Keeping WebSocket alive through arbitrary backgrounds |
| Reconciliation | `Reconciler` | Keyed diff | n/a — mandatory | Position-only diff |
| Bridge command queue | (per command kind) | `BackPressure` | `conflate` (scene patches), `dedupe` (redraws) | Unbounded queue |
| Bridge event queue | (per event kind) | `BackPressure` | `conflate` (frame ticks) | Unbounded queue; dropping input |
| Local persistence | `LocalPersistence` | `SqlitePersistence` (desktop/mobile), `IndexedDbPersistence` (wasm) | `SledPersistence`, `InMemoryPersistence` (dev/test) | Bespoke binary file formats |
| Wasm executor | (build-time) | Cooperative single-thread | Web Workers w/ `SharedArrayBuffer` (where COOP/COEP available) | Fake multi-threading via spinning |
| Remote subscription style | (per region) | Stream + conflate (state) or buffer (events) | `ask` polling for one-shots | Long-poll; raw HTTP retries |

Strategy traits are the only pluggable surface. Anything not in this table is not configurable; defaults are mandatory rather than tunable.

---

## 12. Implementation Roadmap

### Phase 1 — Bridge and contract

- `UiBackend`, `BackendCommand`, `BackendEvent` traits and envelopes.
- `UiBridge` channels with bounded mailboxes and per-kind backpressure policies.
- `UiBridgeActor` with subscription routing.
- `SceneDescription`, `ScenePatch`, keyed `Reconciler` trait.

**Exit criterion:** bridge passes a property-based test suite for backpressure semantics; round-trip latency from actor command to backend dispatch under 1ms on a quiet system.

### Phase 2 — Reference backend (`WinitWgpuBackend` + egui)

- `WinitWgpuBackend` with multi-window support.
- `egui` reconciler for `SceneDescription`.
- Reference application: federated chat client.
- Cross-platform CI: Windows, macOS, Linux.

**Exit criterion:** reference app sustains 60fps with a remote sharded `ConversationActor` producing 100 messages/sec; reconnect through cluster restart under 5s.

### Phase 3 — Bevy backend with `EntityActor` mirror

- `BevyBackend` with bridge systems in `PreUpdate` / `PostUpdate`.
- `ActorBacked` component + sync systems.
- Reference application: small multiplayer demo with server-authoritative state in actors.

**Exit criterion:** 100 mirrored entities sync at 60fps with no frame drops; multiplayer state survives a cluster failover.

### Phase 4 — Wasm + browser backend

- Custom single-thread executor for the actor system in wasm.
- `WebDomBackend` reconciler against Yew or Leptos.
- IndexedDB persistence.
- Bundle-size CI gate (<2MB gzipped).
- Reference application: in-browser chat client.

**Exit criterion:** browser app reconnects across tab suspend/resume; bfcache resume works without state loss; bundle size under target.

### Phase 5 — Mobile, Flow A

- `NativeShellBackend` with uniffi bindings.
- Reference iOS app: SwiftUI shell + Rust core.
- Reference Android app: Compose shell + Rust core.
- `MobileLifecyclePolicy` with `AggressiveSuspend` default.
- APNs / FCM gateway in cluster.

**Exit criterion:** app survives app-store-grade lifecycle testing (background → push → foreground → reconnect under 5s); battery usage in idle background under platform thresholds.

### Phase 6 — Mobile, Flow B + mixed mode

- Bevy iOS / Android target validation.
- Embedded-Rust-view inside SwiftUI / Compose (mixed mode).
- Reference application: an app with both a native settings screen and an embedded Bevy 3D view.

**Exit criterion:** mixed-mode app demonstrates a `WindowActor` driving a native screen and a Bevy view simultaneously, with consistent input handling and no actor-system contention.

### Phase 7 — Out of scope, documented

- Live AR with low-latency device-cluster round-trips (a separate architecture; the assumptions here do not hold).
- Game engines other than Bevy (Fyrox, Macroquad, etc. — a third-party `UiBackend` impl, not a default).
- TV / set-top-box platforms (tvOS, Android TV — viable via Flow A but with their own input model).
- Voice-only interfaces (no `SceneDescription`; a separate front-end shape).

---

## 13. Summary

| Question | Answer |
|---|---|
| What is the runtime-shape default? | The architecture works on three: windowed, Bevy, wasm. Pick by application shape, not preference. |
| What is the actor grain? | `WindowActor` per window; `RegionActor` per stateful or remotely-addressable subtree; no actors for stateless presentation. |
| What is the UI backend default? | `WinitWgpuBackend` for cross-platform native; alternatives per platform need. |
| How does the actor system talk to the UI thread? | `UiBridge`: bounded MPSC each direction, `conflate` for state snapshots, `BackPressure` for everything else. |
| How do remote actors update the UI? | They `tell` a `WindowActor` or `RegionActor` via `ClusterClient`. Same protocol as local actors. |
| How does Bevy fit? | Bevy embeds the actor system. Two bridge systems per frame. Optional `EntityActor` mirror for entities needing persistence or network sync. |
| What about wasm? | Single-threaded cooperative executor; `WebSocketSession` forced; bundle-size budget mandatory; cluster-side `SessionActor` essential for tab-suspend survival. |
| What is the mobile default? | Flow A — `NativeShellBackend` with uniffi to SwiftUI / Compose. Pure Rust (Flow B) for games and creative tools. |
| What is the mobile lifecycle default? | `AggressiveSuspend`: disconnect on background, reconnect on foreground, push notifications via APNs / FCM. |
| What stays out of the actor system? | Per-frame transforms, animation curves, layout math, individual widget state without remote backing. |
| What is the unifying mental model? | The rendering backend is an `EdgeTransport` pointed inward. Same bounded mailbox, same supervised session, same backpressure discipline as a WAN peer. |
| What is the strategic payoff? | One actor decomposition runs from a chat window in a browser tab to a multiplayer entity in a Bevy game to a SwiftUI screen on iOS. The UI is a federation edge. |

---

## Appendix A — Dismissed Alternatives

These options were considered and rejected. They are documented here so future readers do not re-litigate them.

### A.1 One actor per widget

Granularity trap. Mailbox overhead per button click, supervision noise per scroll event, and zero benefit because most widgets have no addressable state. Per-frame interactions become message storms. **Dismissed.** Use the actor grain from Section 4: addressable state with cross-frame or cross-boundary lifetime.

### A.2 A universal widget abstraction across GTK, Bevy, DOM

Tried by every cross-platform UI library and always leaks. Either the abstraction privileges one platform's idioms (uncanny everywhere else) or it pavés over them all (uniformly mediocre). **Dismissed.** Stop at `SceneDescription` as a declarative tree of *semantic intent*; let backends realize it natively.

### A.3 Running the actor system on the UI thread

Tempting because it removes the bridge. Disastrous in practice: any slow `ask`, any blocking I/O, any compute-heavy actor handler stutters the frame. Couples actor-system throughput to UI-thread health. **Dismissed.** The bridge is the cost of being honest about thread affinity.

### A.4 Synchronous "render the world" calls from actors

Variant of A.3. An actor that calls the backend and blocks waiting for the result holds a frame budget hostage. **Dismissed.** All actor → backend traffic is fire-and-forget commands; backend → actor responses arrive as separate `BackendEvent`s correlated by `CorrelationId`.

### A.5 Hosting cluster membership on the heavy client

Already dismissed in `Federation §A.4`; restated here for the UI context. A UI process is exactly the wrong place to be a cluster member: transient lifecycle, bad failure-detection signal, low trust, no business holding ddata replicas. **Dismissed.** Heavy clients are non-member `ActorSystem`s with `ClusterClient`.

### A.6 Position-only reconciliation

Reconciling scene trees without stable keys is fast and wrong. Insertions in the middle of a list become destroy-and-recreate of every following element, losing focus, losing animation state, losing scroll position, losing input continuity. The same trap React fell into without `key`s. **Dismissed.** Keyed reconciliation is the only acceptable default.

### A.7 Unbounded mailboxes anywhere on the bridge

Unbounded queues are a request to OOM under sustained mismatch. The bridge has bounded queues with explicit overflow policies (`BackPressure`, `conflate`, `dedupe`) per command and event kind. **Dismissed.** Same rule as the receptionist's inbound mailbox in `Federation §3.1`.

### A.8 GTK as a cross-platform mobile backend

GTK4 runs on Linux phones (Phosh, Librem 5) and technically can be coerced onto Windows or macOS, but it does not run on iOS or Android in any production-viable way, and the macOS / Windows experience is uncanny. **Dismissed.** `GtkBackend` is a Linux-desktop backend, full stop.

### A.9 Embedding a JavaScript runtime as the FFI bridge on mobile

Adding a JS runtime to bridge between Rust and native UI multiplies runtimes (now three: Rust + native + JS), multiplies lifecycle hazards, costs binary size, and produces worse type safety than a direct FFI. **Dismissed.** Use uniffi.

### A.10 Keeping the WebSocket open through arbitrary mobile backgrounds

Both iOS and Android will kill the connection within seconds-to-minutes regardless. Fighting the platform produces stale-connection bugs, wasted retries, and wakelock battery drain that gets the app penalized. **Dismissed.** Disconnect on background, reconnect on foreground, push for wakeups.

### A.11 Custom binary persistence formats

Re-inventing transactions, schema migration, crash recovery, and concurrent access for the savings of "no SQL dependency" is a category error. **Dismissed.** Use SQLite (desktop, mobile) or IndexedDB (wasm).

### A.12 Mirroring every Bevy entity as an actor

Variant of A.1 in Bevy clothing. Per-frame ECS entities (particles, projectiles, decoration) have no cross-frame meaning and no addressability requirement. Mirroring them as actors drowns the actor system in mailbox churn. **Dismissed.** The `EntityActor` mirror pattern is opt-in via `ActorBacked` for entities with cross-frame identity.

### A.13 A "single UI runtime" that abstracts over Bevy, GTK, and DOM at the framework level

Same fundamental error as A.2 at a larger scale. Frameworks that try to be "Bevy and GTK and DOM" end up being good at none. **Dismissed.** The shared layer is `SceneDescription` and the actor decomposition above it; the rendering layer is platform-native.

### A.14 Long-poll or REST polling instead of streaming subscriptions for UI state

UI state changes are the canonical use case for server-pushed updates with backpressure. Long-poll multiplies request overhead and produces poor latency; REST polling wastes bandwidth and cannot match a stream's ordering and exactly-once semantics. **Dismissed.** Subscriptions over the existing `ClusterClient` session.

### A.15 Per-frame ECS state in actors (Bevy)

The flip side of A.12. Putting `Transform` in an actor and updating it per frame from an ECS system means a mailbox round-trip for every entity every frame. The actor system is not built for that throughput, and it is the wrong layer anyway. **Dismissed.** Per-frame state is ECS; cross-frame state is actors; the mirror pattern bridges only entities that need both.
