---
name: atomr-view-python
description: Use when working with Python bindings for atomr-view. Covers PyUiSystem usage, declarative scene construction in Python, and async integration. Triggers on Python imports of atomr_view or implementing Python-side UI logic.
---

# Python Bindings for atomr-view

This skill helps you leverage `atomr-view` from Python.

## Mental Model

The Python bindings expose the Rust-native UI system and its declarative
scene engine. You can create windows, update scenes, and interact with the
UI bridge directly from Python.

## Usage

```python
import atomr_view as view

# 1. Initialize the system
ui = view.PyUiSystem()

# 2. Create a window
ui.create_window("main", "Python App")

# 3. Build and set a scene
root = view.SceneNode(
    key=view.SceneKey(),
    kind=view.NodeKind.Container(),
    properties=view.PropertyMap({"layout": view.PropertyValue.String("vertical")}),
    children=[
        view.SceneNode(
            key=view.SceneKey(),
            kind=view.NodeKind.Text(),
            properties=view.PropertyMap({"text": view.PropertyValue.String("Hello")}),
            children=[]
        )
    ]
)
ui.set_scene("main", view.SceneDescription(root))
```

## Async Integration

The bridge methods are thread-safe and can be integrated with Python's
`asyncio` by wrapping the blocking PyO3 calls in threads or using
async-friendly wrappers.

## Best Practices

- **Scene Recycling**: Cache `SceneKey`s in Python objects to ensure
  efficient reconciliation.
- **Data Types**: Use the provided `PropertyValue` types (String, Number,
  Boolean, etc.) to ensure correct serialization to the Rust backend.
- **Error Handling**: Catch `PyResult` errors when calling bridge methods.
