//! elan-tui application state and ratatui rendering.
//!
//! [`App`] is the single source of truth for the TUI.  The main event loop
//! in `main.rs` mutates `App` in response to [`AppEvent`]s and calls
//! [`App::draw`] on every iteration.

use crate::client::{central::CentralClient, query::QueryClient};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use elan_common::types::api::{CatalogResponse, NamespaceInfo, QueryResponse};
use futures::StreamExt;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs},
    Frame,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tui_textarea::TextArea;
use uuid::Uuid;

/// Which TUI pane currently has keyboard focus.
#[derive(PartialEq, Clone, Copy)]
pub enum Pane {
    Editor,
    Results,
    Audit,
    Catalog,
}

/// All mutable UI state for elan-tui.
pub struct App {
    pub active_pane: Pane,
    pub editor: TextArea<'static>,
    pub results: Option<QueryResponse>,
    pub results_scroll: u16,
    pub audit_log: Vec<String>,
    pub audit_scroll: u16,
    pub catalog: Option<CatalogResponse>,
    pub status_message: String,
    pub session_id: String,
    pub username: String,
    pub query_client: Arc<QueryClient>,
    pub central_client: Option<Arc<CentralClient>>,
    pub is_loading: bool,
}

impl App {
    /// Create a new `App` instance.  The central client is connected asynchronously
    /// after construction, so `central_client` starts as `None`.
    pub fn new(query_url: String, central_url: Option<String>, username: String) -> Self {
        let mut editor = TextArea::default();
        editor.set_placeholder_text("Enter SQL query... (Ctrl+Enter or F5 to execute)");

        Self {
            active_pane: Pane::Editor,
            editor,
            results: None,
            results_scroll: 0,
            audit_log: Vec::new(),
            audit_scroll: 0,
            catalog: None,
            status_message: "Ready".to_string(),
            session_id: Uuid::new_v4().to_string(),
            username: username.clone(),
            query_client: Arc::new(QueryClient::new(query_url, username)),
            central_client: None, // connected async after init
            is_loading: false,
        }
    }

    /// Render the full TUI layout into `frame`.
    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        // Main layout: top half editor, bottom half split 3 ways
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(vertical[1]);

        self.draw_editor(frame, vertical[0]);
        self.draw_results(frame, bottom[0]);
        self.draw_audit(frame, bottom[1]);
        self.draw_catalog(frame, bottom[2]);
        self.draw_status_bar(frame, area);
    }

    fn pane_style(&self, pane: Pane) -> Style {
        if self.active_pane == pane {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        }
    }

    fn draw_editor(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_style(Pane::Editor))
            .title(Span::styled(
                format!(" SQL Editor [{}] ", self.username),
                self.pane_style(Pane::Editor),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(&self.editor, inner);
    }

    fn draw_results(&self, frame: &mut Frame, area: Rect) {
        let title = if self.is_loading {
            " Results [loading...] ".to_string()
        } else if let Some(ref r) = self.results {
            format!(" Results [{} rows] ", r.rows.len())
        } else {
            " Results ".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_style(Pane::Results))
            .title(Span::styled(title, self.pane_style(Pane::Results)));

        if let Some(ref resp) = self.results {
            let header_cells = resp
                .columns
                .iter()
                .map(|c| Cell::from(c.as_str()).style(Style::default().add_modifier(Modifier::BOLD)));
            let header = Row::new(header_cells).height(1);

            let rows: Vec<Row> = resp
                .rows
                .iter()
                .skip(self.results_scroll as usize)
                .map(|row| {
                    let cells = row.iter().map(|v| {
                        Cell::from(match v {
                            serde_json::Value::Null => "NULL".to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                    });
                    Row::new(cells).height(1)
                })
                .collect();

            let widths: Vec<Constraint> = resp
                .columns
                .iter()
                .map(|_| Constraint::Min(12))
                .collect();

            let table = Table::new(rows, widths)
                .header(header)
                .block(block)
                .highlight_style(Style::default().bg(Color::DarkGray));

            frame.render_widget(table, area);
        } else {
            let text = Paragraph::new("No results yet.").block(block);
            frame.render_widget(text, area);
        }
    }

    fn draw_audit(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_style(Pane::Audit))
            .title(Span::styled(
                format!(" Audit [{} events] ", self.audit_log.len()),
                self.pane_style(Pane::Audit),
            ));

        let items: Vec<ListItem> = self
            .audit_log
            .iter()
            .rev()
            .skip(self.audit_scroll as usize)
            .take(area.height as usize)
            .map(|s| ListItem::new(s.as_str()))
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    fn draw_catalog(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_style(Pane::Catalog))
            .title(Span::styled(" Catalog ", self.pane_style(Pane::Catalog)));

        let mut items = vec![];
        if let Some(ref catalog) = self.catalog {
            for ns in &catalog.namespaces {
                items.push(ListItem::new(format!("▾ {}", ns.name))
                    .style(Style::default().add_modifier(Modifier::BOLD)));
                for ds in &ns.datasets {
                    items.push(ListItem::new(format!("  └ {} ({})", ds.name, ds.source_type)));
                }
            }
        } else {
            items.push(ListItem::new("(not loaded — press Ctrl+R)"));
        }

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    fn draw_status_bar(&self, frame: &mut Frame, area: Rect) {
        let help = " Tab:pane  F5/Ctrl+Enter:run  Ctrl+R:refresh  Ctrl+C:quit ";
        let status = Paragraph::new(format!(" {} | {}", self.status_message, help))
            .style(Style::default().fg(Color::Black).bg(Color::Cyan));
        let bar_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(status, bar_area);
    }

    /// Cycle focus to the next pane in tab order: Editor → Results → Audit → Catalog → Editor.
    pub fn next_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Editor => Pane::Results,
            Pane::Results => Pane::Audit,
            Pane::Audit => Pane::Catalog,
            Pane::Catalog => Pane::Editor,
        };
    }

    /// Append an audit message and evict the oldest entry if the log exceeds 500 lines.
    pub fn push_audit(&mut self, msg: String) {
        self.audit_log.push(msg);
        // Cap in-memory history to avoid unbounded growth during long sessions.
        if self.audit_log.len() > 500 {
            self.audit_log.remove(0);
        }
    }
}
