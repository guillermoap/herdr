use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Paragraph, Widget},
};

use super::*;

pub(super) struct AgentRow {
    pub(super) pane_id: String,
    pub(super) status: crate::api::schema::AgentStatus,
    pub(super) focused: bool,
    pub(super) tab_id: String,
    pub(super) tab_heading: String,
    pub(super) rows: Vec<Vec<crate::ui::ResolvedToken>>,
}

impl AsRef<AgentRow> for AgentRow {
    fn as_ref(&self) -> &AgentRow {
        self
    }
}

pub(super) struct TabHeading {
    pub(super) endpoint_id: ClientEndpointId,
    pub(super) tab_id: String,
    pub(super) label: String,
    pub(super) focused: bool,
    pub(super) stale: bool,
}

pub(super) enum AgentListItem<R> {
    Tab(TabHeading),
    Agent { row: R, last_in_tab: Option<bool> },
}

impl<R: AsRef<AgentRow>> AgentListItem<R> {
    pub(super) fn lines(&self, row_gap: u16) -> usize {
        match self {
            Self::Tab(_) => 1,
            Self::Agent { row, last_in_tab } => {
                let gap = if *last_in_tab == Some(false) {
                    row_gap
                } else {
                    0
                };
                row.as_ref().rows.len().max(1) + usize::from(gap)
            }
        }
    }

    pub(super) fn gap_before(&self, next: &Self, row_gap: u16) -> u16 {
        match (self, next) {
            (
                Self::Agent {
                    last_in_tab: None, ..
                },
                _,
            )
            | (_, Self::Tab(_)) => row_gap,
            _ => 0,
        }
    }
}

pub(super) fn groups_by_tab(
    sort: crate::config::AgentPanelSortConfig,
    agent_view_label: Option<&str>,
) -> bool {
    sort == crate::config::AgentPanelSortConfig::Spaces && agent_view_label.is_none()
}

pub(super) fn agent_list_items<R: AsRef<AgentRow>>(
    rows: Vec<R>,
    grouped: bool,
    heading: impl Fn(&R) -> TabHeading,
) -> Vec<AgentListItem<R>> {
    let mut items = Vec::with_capacity(rows.len());
    let mut open_heading = None;
    let mut rows = rows.into_iter().peekable();
    while let Some(row) = rows.next() {
        if !grouped {
            items.push(AgentListItem::Agent {
                row,
                last_in_tab: None,
            });
            continue;
        }
        let current = heading(&row);
        let focused = current.focused;
        let last = rows.peek().is_none_or(|next| {
            let next = heading(next);
            next.endpoint_id != current.endpoint_id || next.tab_id != current.tab_id
        });
        let index = match open_heading {
            Some(index) => index,
            None => {
                items.push(AgentListItem::Tab(current));
                items.len() - 1
            }
        };
        if let AgentListItem::Tab(open) = &mut items[index] {
            open.focused |= focused;
        }
        open_heading = (!last).then_some(index);
        items.push(AgentListItem::Agent {
            row,
            last_in_tab: Some(last),
        });
    }
    items
}

pub(super) fn render_agent_items<R: AsRef<AgentRow>>(
    buffer: &mut Buffer,
    area: Rect,
    items: &[AgentListItem<R>],
    empty_message: Option<&str>,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
    mut render_agent: impl FnMut(&mut Buffer, Rect, &R, Option<bool>, &mut ShellHitMap),
) {
    let row_gap = config.agents.row_gap;
    render_agent_list(
        buffer,
        area,
        items,
        empty_message,
        config,
        agent_scroll,
        hits,
        |item| item.lines(row_gap),
        |item, next| item.gap_before(next, row_gap),
        |buffer, rect, item, hits| match item {
            AgentListItem::Tab(heading) => render_tab_heading(buffer, rect, heading, config, hits),
            AgentListItem::Agent { row, last_in_tab } => {
                let agent_rect = Rect {
                    height: rect
                        .height
                        .min(row.as_ref().rows.len().max(1).min(u16::MAX as usize) as u16),
                    ..rect
                };
                render_agent(buffer, agent_rect, row, *last_in_tab, hits);
                if *last_in_tab == Some(false) {
                    for y in agent_rect.bottom()..rect.bottom() {
                        put_text(
                            buffer,
                            rect.x,
                            y,
                            rect.width,
                            " │",
                            Style::default().fg(config.palette.overlay0),
                        );
                    }
                }
            }
        },
    );
}

fn render_tab_heading(
    buffer: &mut Buffer,
    rect: Rect,
    heading: &TabHeading,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let style = if heading.stale {
        Style::default()
            .fg(config.palette.overlay0)
            .add_modifier(Modifier::DIM)
    } else if heading.focused {
        Style::default()
            .fg(config.palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(config.palette.subtext0)
            .add_modifier(Modifier::BOLD)
    };
    put_text(
        buffer,
        rect.x,
        rect.y,
        rect.width,
        &format!(" {}", heading.label),
        style,
    );
    hits.sidebar_tabs
        .push((rect, heading.endpoint_id.clone(), heading.tab_id.clone()));
}

pub(super) fn ordered_agent_pane_ids(
    snapshot: &ClientShellSnapshot,
    sort: crate::config::AgentPanelSortConfig,
) -> Vec<String> {
    if snapshot.agent_view_label.is_some() {
        return snapshot
            .agent_order
            .iter()
            .filter(|pane_id| {
                snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.pane_id == pane_id.as_str())
            })
            .cloned()
            .collect();
    }
    let mut agents = snapshot.agents.iter().collect::<Vec<_>>();
    if sort == crate::config::AgentPanelSortConfig::Priority {
        agents.sort_by_key(|agent| {
            (
                std::cmp::Reverse(status_priority(agent.agent_status)),
                std::cmp::Reverse(agent.state_change_seq),
            )
        });
    }
    agents
        .into_iter()
        .map(|agent| agent.pane_id.clone())
        .collect()
}

pub(super) fn render_agent_panel(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
) {
    if !render_agent_panel_header(
        buffer,
        area,
        snapshot.agent_view_label.as_deref(),
        config,
        hits,
    ) {
        return;
    }

    let grouped = groups_by_tab(
        config.agent_panel_sort,
        snapshot.agent_view_label.as_deref(),
    );
    let items = agent_list_items(
        agent_rows(snapshot, config, None, !grouped),
        grouped,
        |row| TabHeading {
            endpoint_id: endpoint_id.clone(),
            tab_id: row.tab_id.clone(),
            label: row.tab_heading.clone(),
            focused: row.focused,
            stale: false,
        },
    );
    render_agent_items(
        buffer,
        area,
        &items,
        snapshot
            .agent_view_label
            .as_ref()
            .map(|_| " no matching agents"),
        config,
        agent_scroll,
        hits,
        |buffer, rect, row, last_in_tab, hits| {
            hits.agents.push((rect, row.pane_id.clone()));
            render_agent_row(buffer, rect, row, last_in_tab, config);
        },
    );
}

pub(super) fn render_agent_panel_header(
    buffer: &mut Buffer,
    area: Rect,
    agent_view_label: Option<&str>,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) -> bool {
    if area.height == 0 {
        return false;
    }
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        &"─".repeat(area.width as usize),
        Style::default().fg(config.palette.surface_dim),
    );
    if area.height < 2 {
        return false;
    }
    put_text(
        buffer,
        area.x,
        area.y + 1,
        area.width,
        " agents",
        Style::default()
            .fg(config.palette.overlay0)
            .add_modifier(Modifier::BOLD),
    );
    let sort_label = agent_view_label.unwrap_or(match config.agent_panel_sort {
        crate::config::AgentPanelSortConfig::Spaces => "grouped",
        crate::config::AgentPanelSortConfig::Priority => "priority",
    });
    let sort_width = display_width(sort_label).min(area.width as usize) as u16;
    let sort_rect = Rect::new(
        area.right().saturating_sub(sort_width),
        area.y + 1,
        sort_width,
        1,
    );
    hits.agent_sort_toggle = if config.mouse_capture && agent_view_label.is_none() {
        sort_rect
    } else {
        Rect::default()
    };
    put_text(
        buffer,
        sort_rect.x,
        sort_rect.y,
        sort_rect.width,
        sort_label,
        Style::default()
            .fg(if agent_view_label.is_some() {
                config.palette.accent
            } else {
                config.palette.overlay0
            })
            .add_modifier(Modifier::BOLD),
    );
    true
}

pub(super) fn render_agent_list<T>(
    buffer: &mut Buffer,
    area: Rect,
    rows: &[T],
    empty_message: Option<&str>,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
    row_lines: impl Fn(&T) -> usize,
    gap_between: impl Fn(&T, &T) -> u16,
    mut render_row: impl FnMut(&mut Buffer, Rect, &T, &mut ShellHitMap),
) {
    let body = Rect::new(
        area.x,
        area.y.saturating_add(3),
        area.width,
        area.height.saturating_sub(3),
    );
    hits.agent_body = body;
    if body.is_empty() || rows.is_empty() {
        *agent_scroll = 0;
        if let Some(message) = empty_message.filter(|_| !body.is_empty()) {
            put_text(
                buffer,
                body.x,
                body.y,
                body.width,
                message,
                Style::default()
                    .fg(config.palette.overlay0)
                    .add_modifier(Modifier::DIM),
            );
        }
        return;
    }

    let row_heights = rows
        .iter()
        .map(|row| row_lines(row).max(1).min(u16::MAX as usize) as u16)
        .collect::<Vec<_>>();
    let gaps = list_gaps(rows, gap_between);
    let metrics =
        super::scroll::list_scroll_metrics(&row_heights, &gaps, body.height, *agent_scroll);
    hits.agent_max_scroll = metrics.max_offset_from_bottom;
    hits.agent_scroll_metrics = Some(metrics);
    *agent_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let show_scrollbar = metrics.max_offset_from_bottom > 0 && body.width > 1;
    let content_width = body.width.saturating_sub(u16::from(show_scrollbar));
    let mut y = body.y;
    for (index, row) in rows.iter().enumerate().skip(*agent_scroll) {
        let height = row_heights[index].min(body.height);
        if y.saturating_add(height) > body.bottom() {
            break;
        }
        let rect = Rect::new(body.x, y, content_width, height);
        render_row(buffer, rect, row, hits);
        y = y.saturating_add(height).saturating_add(gaps[index]);
    }

    if show_scrollbar {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.agent_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, &config.palette);
    }
}

pub(super) fn agent_rows(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    machine: Option<&str>,
    show_location: bool,
) -> Vec<AgentRow> {
    ordered_agent_pane_ids(snapshot, config.agent_panel_sort)
        .into_iter()
        .filter_map(|pane_id| agent_row(snapshot, &pane_id, config, machine, show_location))
        .collect()
}

pub(super) fn agent_row(
    snapshot: &ClientShellSnapshot,
    pane_id: &str,
    config: &ClientShellConfig,
    machine: Option<&str>,
    show_location: bool,
) -> Option<AgentRow> {
    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)?;
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
    let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == agent.tab_id);
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == agent.pane_id);
    let tab_count = snapshot
        .tabs
        .iter()
        .filter(|candidate| candidate.workspace_id == agent.workspace_id)
        .count();
    let informative_tab = tab
        .filter(|tab| tab_count > 1 || tab.custom_label)
        .map(|tab| tab.label.as_str());
    let tab_label = informative_tab.filter(|_| show_location);
    let tab_heading = informative_tab.map_or_else(
        || workspace.label.clone(),
        |tab| format!("{} › {tab}", workspace.label),
    );
    let agent_label = agent
        .display_agent
        .as_deref()
        .or(agent.name.as_deref())
        .or(agent.agent.as_deref())
        .or(agent.title.as_deref());
    let labels = agent
        .state_labels
        .iter()
        .cloned()
        .collect::<HashMap<_, _>>();
    let tokens = agent.tokens.iter().cloned().collect::<HashMap<_, _>>();
    let state_text = labels
        .get(status_text(agent.agent_status))
        .map(String::as_str)
        .unwrap_or_else(|| sidebar_status_text(agent.agent_status));
    let canonical_agent = agent
        .agent
        .as_deref()
        .and_then(crate::detect::parse_agent_label);
    let rows = crate::ui::sidebar_agent_rows(
        &config.agents,
        crate::ui::AgentTokenContext {
            machine,
            workspace: Some(workspace.label.as_str()).filter(|_| show_location),
            tab: tab_label,
            pane: agent
                .title
                .as_deref()
                .or_else(|| pane.and_then(|pane| pane.label.as_deref())),
            agent_label,
            terminal_title: agent.terminal_title.as_deref(),
            terminal_title_stripped: agent.terminal_title_stripped.as_deref(),
            canonical_agent,
            tokens: &tokens,
        },
        state_text,
    );
    Some(AgentRow {
        pane_id: agent.pane_id.clone(),
        status: agent.agent_status,
        focused: agent.focused,
        tab_id: agent.tab_id.clone(),
        tab_heading,
        rows,
    })
}

pub(super) fn render_agent_row(
    buffer: &mut Buffer,
    rect: Rect,
    row: &AgentRow,
    last_in_tab: Option<bool>,
    config: &ClientShellConfig,
) {
    let palette = &config.palette;
    let row_style = if row.focused {
        Style::default().bg(palette.active_row_bg)
    } else {
        Style::default()
    };
    let name_style = if row.focused {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(palette.subtext0)
            .add_modifier(Modifier::BOLD)
    };
    let status_style = Style::default().fg(status_color(row.status, palette));
    let secondary = Style::default().fg(palette.overlay0);
    let icon = (
        status_icon(row.status, config.status_indicators),
        Style::default().fg(status_color(row.status, palette)),
    );
    let rows = if row.rows.is_empty() {
        vec![vec![crate::ui::ResolvedToken {
            kind: crate::ui::ResolvedTokenKind::StateIcon,
            style: Default::default(),
        }]]
    } else {
        row.rows.clone()
    };
    for (index, tokens) in rows.iter().take(rect.height as usize).enumerate() {
        let prefix = match (last_in_tab, index) {
            (None, 0) => " ",
            (None, _) => "   ",
            (Some(false), 0) => " ├─ ",
            (Some(true), 0) => " └─ ",
            (Some(false), _) => " │    ",
            (Some(true), _) => "      ",
        };
        let indent = display_width(prefix);
        let mut spans = vec![ratatui::text::Span::styled(prefix, secondary)];
        spans.extend(crate::ui::resolved_token_spans(
            tokens,
            icon,
            status_style,
            name_style,
            secondary,
            secondary,
            palette,
            rect.width.saturating_sub(indent as u16) as usize,
        ));
        Paragraph::new(Line::from(spans)).style(row_style).render(
            Rect::new(rect.x, rect.y + index as u16, rect.width, 1),
            buffer,
        );
    }
}

pub(super) fn list_gaps<T>(rows: &[T], gap_between: impl Fn(&T, &T) -> u16) -> Vec<u16> {
    (0..rows.len())
        .map(|index| {
            rows.get(index + 1)
                .map_or(0, |next| gap_between(&rows[index], next))
        })
        .collect()
}

fn put_text(buffer: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    for (offset, character) in text.chars().take(width as usize).enumerate() {
        if let Some(cell) = buffer.cell_mut((x + offset as u16, y)) {
            cell.set_char(character).set_style(style);
        }
    }
}

fn display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

fn sidebar_status_text(status: crate::api::schema::AgentStatus) -> &'static str {
    use crate::api::schema::AgentStatus;
    match status {
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Working => "working",
        AgentStatus::Idle | AgentStatus::Unknown => "idle",
    }
}
