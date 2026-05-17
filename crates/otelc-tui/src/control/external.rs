//! External control plane: an adapter onto a third-party OpAMP server.
//!
//! OpAMP standardizes only the Agent↔Server protocol — there is no standard
//! Server↔console API. "External mode" is therefore an *adapter*: a concrete
//! adapter must be written per backend. This build ships the abstraction and
//! mode selection; it does not bundle a backend-specific adapter. See
//! `docs/design/dld.md` for the adapter design.

use super::{ControlEvent, ControlPlane};
use tokio::sync::mpsc;

/// Placeholder external control plane. Selecting `--mode external` proves the
/// `ControlPlane` abstraction supports a second backend; a real deployment
/// supplies a backend-specific adapter here.
pub struct ExternalControlPlane {
    url: String,
}

impl ExternalControlPlane {
    /// Build the external control plane for a given server URL.
    pub fn start(url: String) -> (Box<dyn ControlPlane>, mpsc::Receiver<ControlEvent>) {
        let (tx, rx) = mpsc::channel(8);
        let notice = format!(
            "External mode targets {url}. OpAMP defines no standard server-to-console \
             API, so a backend-specific adapter must be supplied (see docs/design/dld.md). \
             This build ships no adapter — use embedded mode for a live fleet."
        );
        tokio::spawn(async move {
            let _ = tx.send(ControlEvent::Notice(notice)).await;
        });
        (Box::new(Self { url }), rx)
    }
}

impl ControlPlane for ExternalControlPlane {
    fn mode(&self) -> &'static str {
        "external"
    }

    fn endpoint(&self) -> String {
        self.url.clone()
    }

    fn push_config(&self, _uid: &str, _yaml: &str) -> Result<(), String> {
        Err("external mode needs a backend-specific adapter".to_string())
    }

    fn restart(&self, _uid: &str) -> Result<(), String> {
        Err("external mode needs a backend-specific adapter".to_string())
    }
}
