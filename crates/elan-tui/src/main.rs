//! elan-tui: terminal UI for the elan federated query system.
//!
//! Connects to elan-query (HTTP REST) for query execution and catalog browsing,
//! and to elan-central (gRPC) for the live audit event stream.
//!
//! The event loop multiplexes three async sources over a single `mpsc` channel:
//! - Keyboard events from crossterm
//! - Audit events from the elan-central gRPC stream
//! - Query results from elan-query (spawned as background tasks)

mod app;
mod client;

use app::{App, Pane};
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;
use tracing::error;

/// Internal event type that unifies all async event sources onto one channel.
#[derive(Debug)]
enum AppEvent {
    Key(KeyEvent),
    AuditMessage(String),
    QueryResult(Result<elan_common::types::api::QueryResponse, String>),
    CatalogLoaded(elan_common::types::api::CatalogResponse),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let query_url = args
        .windows(2)
        .find(|w| w[0] == "--query-endpoint")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "http://localhost:3000".to_string());

    let central_url = args
        .windows(2)
        .find(|w| w[0] == "--central-endpoint")
        .map(|w| w[1].clone());

    let username = args
        .windows(2)
        .find(|w| w[0] == "--user")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "alice".to_string());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(query_url, central_url.clone(), username);
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(128);

    // Spawn keyboard event reader
    let key_tx = event_tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            if let Event::Key(k) = event {
                if key_tx.send(AppEvent::Key(k)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Spawn audit stream (if central endpoint provided)
    if let Some(central_endpoint) = central_url {
        let audit_tx = event_tx.clone();
        tokio::spawn(async move {
            match client::central::CentralClient::connect(&central_endpoint).await {
                Ok(central) => {
                    match central.stream_audit_events().await {
                        Ok(mut stream) => {
                            while let Some(Ok(event)) = stream.next().await {
                                let msg = format!(
                                    "[{}] {} {}",
                                    event.occurred_at
                                        .as_ref()
                                        .map(|_| "now")
                                        .unwrap_or("?"),
                                    event.event_type,
                                    event.user_id,
                                );
                                if audit_tx.send(AppEvent::AuditMessage(msg)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = audit_tx
                                .send(AppEvent::AuditMessage(format!("audit stream error: {e}")))
                                .await;
                        }
                    }
                }
                Err(e) => {
                    let _ = audit_tx
                        .send(AppEvent::AuditMessage(format!(
                            "could not connect to central: {e}"
                        )))
                        .await;
                }
            }
        });
    }

    // Load catalog on startup
    let catalog_tx = event_tx.clone();
    let catalog_client = app.query_client.clone();
    tokio::spawn(async move {
        match catalog_client.catalog().await {
            Ok(catalog) => {
                let _ = catalog_tx.send(AppEvent::CatalogLoaded(catalog)).await;
            }
            Err(e) => {
                let _ = catalog_tx
                    .send(AppEvent::AuditMessage(format!("catalog load error: {e}")))
                    .await;
            }
        }
    });

    loop {
        terminal.draw(|f| app.draw(f))?;

        if let Some(event) = event_rx.recv().await {
            match event {
                AppEvent::Key(key) => {
                    if handle_key(&mut app, key, event_tx.clone()).await? {
                        break;
                    }
                }
                AppEvent::AuditMessage(msg) => {
                    app.push_audit(msg);
                }
                AppEvent::QueryResult(result) => {
                    app.is_loading = false;
                    match result {
                        Ok(resp) => {
                            app.status_message = format!(
                                "Done: {} rows in {}ms",
                                resp.rows.len(),
                                resp.duration_ms
                            );
                            app.results = Some(resp);
                        }
                        Err(e) => {
                            app.status_message = format!("Error: {e}");
                        }
                    }
                }
                AppEvent::CatalogLoaded(catalog) => {
                    let ns_count = catalog.namespaces.len();
                    app.catalog = Some(catalog);
                    app.status_message = format!("Catalog loaded: {ns_count} namespace(s)");
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

async fn handle_key(
    app: &mut App,
    key: KeyEvent,
    event_tx: mpsc::Sender<AppEvent>,
) -> anyhow::Result<bool> {
    use KeyCode::*;

    // Global keys
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, Char('c')) => return Ok(true),
        (KeyModifiers::NONE, Tab) => {
            app.next_pane();
            return Ok(false);
        }
        (KeyModifiers::CONTROL, Char('r')) => {
            // Refresh catalog
            let client = app.query_client.clone();
            let tx = event_tx.clone();
            tokio::spawn(async move {
                match client.catalog().await {
                    Ok(catalog) => { let _ = tx.send(AppEvent::CatalogLoaded(catalog)).await; }
                    Err(e) => { let _ = tx.send(AppEvent::AuditMessage(format!("catalog error: {e}"))).await; }
                }
            });
            app.status_message = "Refreshing catalog...".into();
            return Ok(false);
        }
        (KeyModifiers::NONE, F(5)) | (KeyModifiers::CONTROL, Enter) => {
            execute_query(app, event_tx).await;
            return Ok(false);
        }
        (KeyModifiers::CONTROL, Char('l')) if app.active_pane == Pane::Editor => {
            app.editor = tui_textarea::TextArea::default();
            return Ok(false);
        }
        _ => {}
    }

    // Pane-specific scrolling
    match app.active_pane {
        Pane::Results => match key.code {
            Up => app.results_scroll = app.results_scroll.saturating_sub(1),
            Down => app.results_scroll += 1,
            _ => {}
        },
        Pane::Audit => match key.code {
            Up => app.audit_scroll = app.audit_scroll.saturating_sub(1),
            Down => app.audit_scroll += 1,
            _ => {}
        },
        Pane::Editor => {
            app.editor.input(key);
        }
        _ => {}
    }

    Ok(false)
}

async fn execute_query(app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    let sql = app.editor.lines().join("\n");
    if sql.trim().is_empty() {
        app.status_message = "No SQL to execute".into();
        return;
    }

    app.is_loading = true;
    app.status_message = "Executing...".into();
    app.results_scroll = 0;
    app.active_pane = Pane::Results;

    let client = app.query_client.clone();
    let session_id = app.session_id.clone();
    let tx = event_tx;

    tokio::spawn(async move {
        let result = client
            .query(&sql, &session_id)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.send(AppEvent::QueryResult(result)).await;
    });
}
