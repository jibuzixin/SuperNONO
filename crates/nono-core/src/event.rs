//! The JSON event bus.
//!
//! Every subsystem (window, renderer, behavior, extensions, agent gateway)
//! talks through this bus instead of calling each other directly.  The wire
//! format intentionally matches the design document: a JSON object with a
//! `"type"` discriminator.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::unix_millis;

/// Events exchanged between subsystems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    // ── user input ────────────────────────────────────────────────
    UserInput { text: String },
    UserClick { button: String },
    DragStarted,
    DragEnded,

    // ── animation / character state ───────────────────────────────
    PlayAnimation {
        anim_id: String,
        form: u8,
        #[serde(default)]
        loop_anim: bool,
    },
    AnimationFinished { anim_id: String },
    ChangeState { state: String },
    ChangeForm { form: u8 },
    SetScale { scale: f32 },
    /// Enabled displays changed (platform display ids).
    SetDisplays { display_ids: Vec<u32> },
    /// The pet was summoned to the primary display's centre.
    Summoned,
    SetTrail { enabled: bool, interval_ms: u32, lifetime_ms: u32, opacity: f32 },
    SetSpeed {
        global: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        follow: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        idle: Option<f32>,
    },

    // ── assets ────────────────────────────────────────────────────
    AssetLoaded { asset_id: String },
    AssetError { asset_id: String, message: String },

    // ── extensions (M3) ───────────────────────────────────────────
    ExtensionEvent {
        plugin: String,
        event: serde_json::Value,
    },

    // ── agent gateway (M2) ────────────────────────────────────────
    AgentRequest {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_id: Option<String>,
    },
    AgentReply {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emotion: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggest_anim: Option<String>,
    },
    AgentError { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub ts_ms: u64,
    pub event: Event,
}

type Waker = Arc<dyn Fn() + Send + Sync>;

/// A broadcast event bus.  Publishing is non-blocking and never fails because a
/// consumer is slow (slow consumers simply miss old messages).
#[derive(Clone)]
pub struct Bus {
    tx: broadcast::Sender<Envelope>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus").field("receivers", &self.tx.receiver_count()).finish()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx, waker: Arc::new(Mutex::new(None)) }
    }

    /// The waker is called after every event so the GUI thread can wake up.
    pub fn set_waker<F: Fn() + Send + Sync + 'static>(&self, waker: F) {
        *self.waker.lock().unwrap() = Some(Arc::new(waker));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: Event) {
        let envelope = Envelope { ts_ms: unix_millis(), event };
        if tracing::enabled!(tracing::Level::DEBUG) {
            match serde_json::to_string(&envelope) {
                Ok(json) => tracing::debug!(target: "nono.bus", "{json}"),
                Err(err) => tracing::debug!(target: "nono.bus", "unserialisable event: {err}"),
            }
        }
        // No receivers is fine: modules are optional.
        let _ = self.tx.send(envelope);
        if let Some(waker) = self.waker.lock().unwrap().as_ref() {
            waker();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_json_matches_documented_shape() {
        let json = serde_json::to_string(&Event::PlayAnimation {
            anim_id: "form1/walk/down".into(),
            form: 1,
            loop_anim: true,
        })
        .unwrap();
        assert_eq!(json, r#"{"type":"PlayAnimation","anim_id":"form1/walk/down","form":1,"loop_anim":true}"#);

        let reply: Event = serde_json::from_str(
            r#"{"type":"AgentReply","text":"哈喽！","emotion":"happy","suggest_anim":"talk"}"#,
        )
        .unwrap();
        assert_eq!(
            reply,
            Event::AgentReply {
                text: "哈喽！".into(),
                emotion: Some("happy".into()),
                suggest_anim: Some("talk".into()),
            }
        );

        let trail: Event = serde_json::from_str(
            r#"{"type":"SetTrail","enabled":true,"interval_ms":120,"lifetime_ms":640,"opacity":0.22}"#,
        )
        .unwrap();
        assert_eq!(
            trail,
            Event::SetTrail { enabled: true, interval_ms: 120, lifetime_ms: 640, opacity: 0.22 }
        );

        let speed: Event =
            serde_json::from_str(r#"{"type":"SetSpeed","global":90.0,"idle":120.0}"#).unwrap();
        assert_eq!(
            speed,
            Event::SetSpeed { global: 90.0, follow: None, idle: Some(120.0) }
        );
    }

    #[test]
    fn publish_reaches_subscribers_and_wakes() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let woken = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = woken.clone();
        bus.set_waker(move || flag.store(true, std::sync::atomic::Ordering::SeqCst));

        bus.publish(Event::UserInput { text: "你好".into() });
        let envelope = rx.try_recv().unwrap();
        assert_eq!(envelope.event, Event::UserInput { text: "你好".into() });
        assert!(woken.load(std::sync::atomic::Ordering::SeqCst));
    }
}
