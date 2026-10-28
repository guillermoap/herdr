use super::*;

fn sidebar_state(tab_count: usize) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut snapshot = snapshot();
    let tab = snapshot.tabs[0].clone();
    snapshot.tabs = (0..tab_count)
        .map(|index| ClientShellTab {
            tab_id: format!("tab_{}", index + 1),
            label: format!("task {}", index + 1),
            number: index + 1,
            focused: index == 0,
            ..tab.clone()
        })
        .collect();
    state.set_snapshot(Box::new(snapshot));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("frame");
    state
}

#[test]
fn sidebar_tabs_focus_stable_tab_without_dragging_or_switching_workspace() {
    let mut state = sidebar_state(3);
    let workspace = state.hits.workspaces[0].rect;
    let (tab, endpoint, id) = state.hits.sidebar_tabs[1].clone();
    assert!(tab.y >= workspace.bottom());
    assert_eq!(endpoint, ClientEndpointId::Local);
    assert_eq!(id, "tab_2");
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: tab.x + 5,
        row: tab.y,
        modifiers: KeyModifiers::NONE,
    })]);
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::Endpoint {request, ..}]
        if matches!(&request.method, crate::api::schema::Method::TabFocus(target) if target.tab_id == "tab_2"))
    );
    assert!(state.workspace_press.is_none());
    assert!(state.tab_press.is_none());
}

#[test]
fn sidebar_tabs_scroll_with_spaces_without_switching_tabs() {
    let mut state = sidebar_state(30);
    let tab = state.hits.sidebar_tabs[0].0;
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: tab.x + 5,
        row: tab.y,
        modifiers: KeyModifiers::NONE,
    })]);
    assert!(outcome.actions.is_empty());
    assert!(state.workspace_scroll > 0);
    state.workspace_scroll = state.hits.workspace_max_scroll;
    state.compose(106, 30).expect("scrolled frame");
    assert!(state
        .hits
        .sidebar_tabs
        .iter()
        .any(|(_, _, id)| id == "tab_30"));
}

#[test]
fn sidebar_tabs_respect_disabled_mouse_capture() {
    let mut state = sidebar_state(3);
    let rect = state.hits.sidebar_tabs[1].0;
    state.config.mouse_capture = false;
    state.compose(106, 30).expect("frame without mouse capture");
    assert!(state.hits.sidebar_tabs.is_empty());
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x + 5,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    })]);
    assert!(outcome.actions.is_empty());
}

#[test]
fn sidebar_tabs_reveal_keyboard_focus_and_preserve_collapsed_groups() {
    let mut state = sidebar_state(30);
    let mut update = state.snapshot.as_deref().unwrap().clone();
    update.revision = 2;
    update.focused_tab_id = Some("tab_30".into());
    update.workspaces[0].active_tab_id = "tab_30".into();
    for tab in &mut update.tabs {
        tab.focused = tab.tab_id == "tab_30";
    }
    state.set_snapshot(Box::new(update));
    let mut updated_surface = surface();
    updated_surface.projection_revision = 2;
    state.set_pane_surface(updated_surface);
    state.compose(106, 30).expect("focused tab revealed");
    assert!(state
        .hits
        .sidebar_tabs
        .iter()
        .any(|(_, _, id)| id == "tab_30"));
    state
        .collapsed_groups
        .insert(super::super::sidebar_tabs::tab_group_key("ws_1"));
    state.reveal_focused_tab = true;
    state.compose(106, 30).expect("collapsed tabs stay hidden");
    assert!(state.hits.sidebar_tabs.is_empty());
    assert_eq!(state.hits.workspaces.len(), 1);
}

#[test]
fn sidebar_tabs_keep_remote_targets() {
    use crate::client::endpoint::{ProfileId, SavedSshEndpoint};
    let mut state = sidebar_state(2);
    let id = ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap();
    let endpoint = ClientEndpointId::Ssh(id.clone());
    state.set_endpoint_catalog(&[SavedSshEndpoint {
        id,
        label: "Build".into(),
        target: "dev@example.test".into(),
        session: "agents".into(),
        enabled: true,
    }]);
    state.set_endpoint_status(&endpoint, ClientEndpointStatus::Online);
    let mut remote = snapshot();
    remote.boot_id = "remote".into();
    state.set_endpoint_snapshot(&endpoint, Box::new(remote));
    state.compose(106, 40).expect("machines frame");
    let rect = state
        .hits
        .sidebar_tabs
        .iter()
        .find(|(_, target, _)| target == &endpoint)
        .unwrap()
        .0;
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x + 5,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    })]);
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::ActivateEndpoint { endpoint_id, target: Some(ClientEndpointFocusTarget::Tab(tab)) }]
        if endpoint_id == &endpoint && tab == "tab_1")
    );
}

#[test]
fn sidebar_tabs_collapse_without_removing_workspace_or_tab_bar() {
    let mut state = sidebar_state(3);
    let (toggle, _) = state.hits.workspaces[0]
        .group_toggle
        .clone()
        .expect("toggle");
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: toggle.x,
        row: toggle.y,
        modifiers: KeyModifiers::NONE,
    })]);
    state.compose(106, 30).expect("collapsed frame");
    assert!(state.hits.sidebar_tabs.is_empty());
    assert_eq!(state.hits.workspaces.len(), 1);
    assert_eq!(state.hits.tabs.len(), 3);
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: toggle.x,
        row: toggle.y,
        modifiers: KeyModifiers::NONE,
    })]);
    state.compose(106, 30).expect("expanded frame");
    assert_eq!(state.hits.sidebar_tabs.len(), 3);
}

#[test]
fn sidebar_tabs_keep_worktree_tabs_under_their_own_workspace() {
    let mut snapshot = snapshot();
    snapshot.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut child = snapshot.workspaces[0].clone();
    child.workspace_id = "child".into();
    child.focused = false;
    child.worktree.as_mut().unwrap().is_linked_worktree = true;
    snapshot.workspaces.push(child);
    let mut tab = snapshot.tabs[0].clone();
    tab.workspace_id = "child".into();
    tab.tab_id = "child_tab".into();
    tab.focused = false;
    snapshot.tabs.push(tab);
    let rows = super::super::sidebar_tabs::entries(&snapshot, &HashSet::new());
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[1].tab, Some(0));
    assert!(!rows[1].workspace.indented);
    assert_eq!(rows[3].tab, Some(1));
    assert!(rows[3].workspace.indented);
    let collapsed = super::super::sidebar_tabs::entries(&snapshot, &HashSet::from(["repo".into()]));
    assert_eq!(collapsed.len(), 1);
    assert!(collapsed[0].tab.is_none());
}
