use pyo3::prelude::*;
use atomr_view_core::bridge::{UiBridge, BackendCommand, BackendEvent, CorrelationId};
use atomr_view_core::scene::{SceneDescription, SceneNode, SceneKey, NodeKind, PropertyMap, PropertyValue};
use atomr_view_backends::winit_wgpu::WinitWgpuBackend;
use atomr_view_backends::UiBackend;
use tokio::sync::mpsc;

#[pyclass]
struct PyUiSystem {
    cmd_tx: mpsc::Sender<BackendCommand>,
    evt_rx: Option<mpsc::Receiver<BackendEvent>>,
}

#[pymethods]
impl PyUiSystem {
    #[new]
    fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(100);
        let (evt_tx, evt_rx) = mpsc::channel(100);
        
        let bridge = UiBridge { cmd_rx, evt_tx };
        
        std::thread::spawn(move || {
            let backend = WinitWgpuBackend;
            backend.run(bridge);
        });

        Self { cmd_tx, evt_rx: Some(evt_rx) }
    }

    fn create_window(&self, id: String, title: String, py: Python<'_>) -> PyResult<()> {
        let tx = self.cmd_tx.clone();
        py.allow_threads(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = tx.send(BackendCommand::CreateWindow { id, title }).await;
            });
        });
        Ok(())
    }

    fn set_scene(&self, window_id: String, scene: SceneDescription, py: Python<'_>) -> PyResult<()> {
        let tx = self.cmd_tx.clone();
        py.allow_threads(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = tx.send(BackendCommand::SetScene { window_id, scene }).await;
            });
        });
        Ok(())
    }

    fn open_file_picker(&self, title: String, py: Python<'_>) -> PyResult<CorrelationId> {
        let tx = self.cmd_tx.clone();
        let correlation_id = CorrelationId::new();
        let cid = correlation_id.clone();
        py.allow_threads(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = tx.send(BackendCommand::OpenFilePicker { correlation_id: cid, title }).await;
            });
        });
        Ok(correlation_id)
    }
}

#[pymodule]
fn atomr_view(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyUiSystem>()?;
    m.add_class::<SceneKey>()?;
    m.add_class::<SceneDescription>()?;
    m.add_class::<SceneNode>()?;
    m.add_class::<NodeKind>()?;
    m.add_class::<PropertyMap>()?;
    m.add_class::<PropertyValue>()?;
    m.add_class::<CorrelationId>()?;
    Ok(())
}
