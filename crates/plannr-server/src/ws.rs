//! WebSocket hub: browsers connect per project, receive change notifications and answer
//! viewport commands (screenshot, camera, selection…) issued by the MCP or the API.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

struct Client {
    project: String,
    tx: mpsc::UnboundedSender<String>,
    browser: bool,
    last_active: Instant,
    state: Value,
}

#[derive(Default)]
pub struct Hub {
    clients: Mutex<HashMap<u64, Client>>,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>,
    next: AtomicU64,
}

impl Hub {
    pub fn broadcast(&self, project: &str, msg: &Value) {
        let text = msg.to_string();
        let clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        for c in clients.values().filter(|c| c.project == project) {
            let _ = c.tx.send(text.clone());
        }
    }

    /// Latest viewport state the most recently active browser reported.
    pub fn last_state(&self, project: &str) -> Option<Value> {
        let clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        clients.values().filter(|c| c.project == project && c.browser).max_by_key(|c| c.last_active).map(|c| c.state.clone())
    }

    /// Send a command to the most recently active browser on the project and await its reply.
    pub async fn command(&self, project: &str, port: u16, name: &str, args: Value) -> Result<Value, String> {
        let req_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
            let Some(c) = clients.values().filter(|c| c.project == project && c.browser).max_by_key(|c| c.last_active) else {
                return Err(format!("no browser connected — open http://localhost:{port}/?project={project}"));
            };
            let mut command = json!({ "name": name });
            if let Value::Object(map) = args { for (k, v) in map { command[k] = v; } }
            let msg = json!({ "type": "command", "reqId": req_id, "command": command });
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(req_id.clone(), tx);
            if c.tx.send(msg.to_string()).is_err() {
                self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&req_id);
                return Err("browser connection closed".into());
            }
        }
        match tokio::time::timeout(COMMAND_TIMEOUT, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("browser connection closed".into()),
            Err(_) => {
                self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&req_id);
                Err(format!("the browser did not answer `{name}` within 15 s"))
            }
        }
    }

    pub async fn run(self: Arc<Self>, socket: WebSocket, project: String) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        self.clients.lock().unwrap_or_else(|e| e.into_inner()).insert(id, Client { project: project.clone(), tx, browser: false, last_active: Instant::now(), state: Value::Null });
        let (mut sink, mut stream) = socket.split();
        let writer = tokio::spawn(async move {
            while let Some(text) = rx.recv().await {
                if sink.send(Message::Text(text.into())).await.is_err() { break; }
            }
        });
        while let Some(Ok(msg)) = stream.next().await {
            let text = match msg {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => break,
                _ => continue,
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("hello") => {
                    let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(c) = clients.get_mut(&id) {
                        c.browser = v.get("client").and_then(|c| c.as_str()).map_or(true, |c| c == "browser");
                        c.last_active = Instant::now();
                    }
                }
                Some("state") => {
                    let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(c) = clients.get_mut(&id) { c.browser = true; c.last_active = Instant::now(); c.state = v.clone(); }
                }
                Some("reply") => {
                    let req_id = v.get("reqId").and_then(|r| r.as_str()).unwrap_or("").to_string();
                    if let Some(tx) = self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&req_id) {
                        let r = match v.get("error") {
                            Some(e) if !e.is_null() => Err(e.as_str().map(|s| s.to_string()).unwrap_or_else(|| e.to_string())),
                            _ => Ok(v.get("result").cloned().unwrap_or(json!({}))),
                        };
                        let _ = tx.send(r);
                    }
                }
                _ => {}
            }
        }
        self.clients.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        writer.abort();
    }
}
