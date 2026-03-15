use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::world::InjectedEvent;

// ---------------------------------------------------------------------------
// Events file format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct EventsFile {
    #[serde(default)]
    pub events: Vec<InjectedEvent>,
}

// ---------------------------------------------------------------------------
// File watcher
// ---------------------------------------------------------------------------

/// Start watching an events file for changes. New events are sent through `tx`.
pub fn start_watcher(
    events_path: PathBuf,
    debounce_ms: u64,
    tx: mpsc::Sender<InjectedEvent>,
) -> anyhow::Result<()> {
    // Validate the events path is a regular file (not a symlink to sensitive locations)
    if events_path.exists() {
        let metadata = std::fs::symlink_metadata(&events_path)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                "God's Eye events file is a symlink, which is not allowed: {}",
                events_path.display()
            );
        }
    }

    // Process any existing events on startup
    if events_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&events_path) {
            if let Ok(parsed) = toml::from_str::<EventsFile>(&content) {
                for event in parsed.events {
                    let _ = tx.blocking_send(event);
                }
            }
        }
    }

    let (notify_tx, notify_rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_millis(debounce_ms), notify_tx)?;

    // Watch the parent directory if file doesn't exist yet
    let watch_path = if events_path.exists() {
        events_path.clone()
    } else if let Some(parent) = events_path.parent() {
        parent.to_path_buf()
    } else {
        events_path.clone()
    };

    debouncer
        .watcher()
        .watch(&watch_path, RecursiveMode::NonRecursive)?;

    // Spawn blocking thread for file watching
    std::thread::spawn(move || {
        let mut processed_ids: HashSet<String> = HashSet::new();

        // Mark initial events as processed
        if events_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&events_path) {
                if let Ok(parsed) = toml::from_str::<EventsFile>(&content) {
                    for event in &parsed.events {
                        processed_ids.insert(event.id.clone());
                    }
                }
            }
        }

        // Keep debouncer alive
        let _debouncer = debouncer;

        for result in notify_rx {
            match result {
                Ok(_events) => {
                    let content = match std::fs::read_to_string(&events_path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };

                    let parsed = match toml::from_str::<EventsFile>(&content) {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!("God's Eye: invalid TOML in events file: {e}");
                            continue;
                        }
                    };

                    for event in parsed.events {
                        if !processed_ids.contains(&event.id) {
                            processed_ids.insert(event.id.clone());
                            tracing::info!("God's Eye: new event detected: {}", event.id);
                            if tx.blocking_send(event).is_err() {
                                tracing::warn!("God's Eye: channel closed, stopping watcher");
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("God's Eye: file watch error: {e:?}");
                }
            }
        }
    });

    Ok(())
}
