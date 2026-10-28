use super::*;

#[derive(Clone, Copy)]
pub(super) struct SpaceEntry {
    pub(super) workspace: WorkspaceEntry,
    pub(super) tab: Option<usize>,
    last_tab: bool,
}

pub(super) fn tab_group_key(workspace_id: &str) -> String {
    format!("tabs:{workspace_id}")
}

pub(super) fn entries(
    snapshot: &ClientShellSnapshot,
    collapsed: &HashSet<String>,
) -> Vec<SpaceEntry> {
    let mut tabs = HashMap::<&str, Vec<usize>>::new();
    for (index, tab) in snapshot.tabs.iter().enumerate() {
        tabs.entry(&tab.workspace_id).or_default().push(index);
    }
    let mut entries = Vec::new();
    for workspace in render::workspace_entries(snapshot, collapsed) {
        entries.push(SpaceEntry {
            workspace,
            tab: None,
            last_tab: false,
        });
        let source = &snapshot.workspaces[workspace.index];
        if collapsed.contains(&tab_group_key(&source.workspace_id))
            || source.worktree.as_ref().is_some_and(|worktree| {
                !worktree.is_linked_worktree && collapsed.contains(&worktree.key)
            })
        {
            continue;
        }
        if let Some(indices) = tabs.get(source.workspace_id.as_str()) {
            for (position, index) in indices.iter().enumerate() {
                entries.push(SpaceEntry {
                    workspace,
                    tab: Some(*index),
                    last_tab: position + 1 == indices.len(),
                });
            }
        }
    }
    entries
}

pub(super) fn render_tab(
    buffer: &mut Buffer,
    rect: Rect,
    snapshot: &ClientShellSnapshot,
    entry: &SpaceEntry,
    endpoint: &ClientEndpointId,
    active: bool,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let Some(tab) = entry.tab.and_then(|index| snapshot.tabs.get(index)) else {
        return;
    };
    let palette = &config.palette;
    let focused = active && tab.focused;
    let branch = if entry.last_tab { "└─" } else { "├─" };
    let prefix = if entry.workspace.indented {
        if entry.workspace.last_child {
            "      "
        } else {
            "   │  "
        }
    } else {
        ""
    };
    let prefix = format!("{prefix}   {branch} ");
    let style = if focused {
        Style::default()
            .fg(palette.text)
            .bg(palette.active_row_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.subtext0)
    };
    if focused {
        buffer.set_style(rect, style);
    }
    render::put_text(
        buffer,
        rect.x,
        rect.y,
        rect.width,
        &prefix,
        style.fg(palette.overlay0),
    );
    let x = rect.x.saturating_add(render::display_width(&prefix));
    let icon = status_icon(tab.agent_status, config.status_indicators);
    render::put_text(
        buffer,
        x,
        rect.y,
        rect.right().saturating_sub(x),
        icon,
        style.fg(status_color(tab.agent_status, palette)),
    );
    let x = x.saturating_add(render::display_width(icon) + 1);
    render::put_text(
        buffer,
        x,
        rect.y,
        rect.right().saturating_sub(x),
        &tab.label,
        style,
    );
    hits.sidebar_tabs
        .push((rect, endpoint.clone(), tab.tab_id.clone()));
}
