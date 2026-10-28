use super::*;

pub(super) fn parent_workspace_entries(snapshot: &ClientShellSnapshot) -> Vec<WorkspaceEntry> {
    render::workspace_entries(snapshot, &HashSet::new())
        .into_iter()
        .filter(|entry| !entry.indented)
        .collect()
}

pub(super) fn workspace_navigation_parent<'a>(
    snapshot: &'a ClientShellSnapshot,
    workspace_id: &str,
) -> Option<&'a str> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)?;
    if let Some(worktree) = workspace
        .worktree
        .as_ref()
        .filter(|worktree| worktree.is_linked_worktree)
    {
        if let Some(parent) = snapshot.workspaces.iter().find(|candidate| {
            candidate.worktree.as_ref().is_some_and(|candidate| {
                candidate.key == worktree.key && !candidate.is_linked_worktree
            })
        }) {
            return Some(&parent.workspace_id);
        }
    }
    Some(&workspace.workspace_id)
}

impl ClientShellState {
    pub(super) fn active_endpoint_workspace_at(&self, point: (u16, u16)) -> Option<String> {
        self.hits
            .workspaces
            .iter()
            .find(|hit| {
                hit.endpoint_id == self.active_endpoint_id && super::contains(hit.rect, point)
            })
            .map(|hit| hit.workspace_id.clone())
    }

    pub(super) fn endpoint_workspace_is_draggable(&self, press: &ClientWorkspacePress) -> bool {
        press.endpoint_id == self.active_endpoint_id
            && self
                .snapshot
                .as_deref()
                .and_then(|snapshot| {
                    snapshot
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.workspace_id == press.workspace_id)
                })
                .is_some_and(|workspace| {
                    !workspace
                        .worktree
                        .as_ref()
                        .is_some_and(|worktree| worktree.is_linked_worktree)
                })
    }

    pub(super) fn finish_endpoint_workspace_press(
        &mut self,
        press: ClientWorkspacePress,
        outcome: &mut ClientShellInput,
    ) {
        self.focus_or_activate(
            press.endpoint_id,
            ClientEndpointFocusTarget::Workspace(press.workspace_id),
            outcome,
        );
    }

    pub(super) fn handle_endpoint_machine_click(
        &mut self,
        point: (u16, u16),
        outcome: &mut ClientShellInput,
    ) -> bool {
        let Some(hit) = self
            .hits
            .machines
            .iter()
            .find(|hit| super::contains(hit.rect, point))
        else {
            return false;
        };
        let endpoint_id = hit.endpoint_id.clone();
        let collapse_toggle = super::contains(hit.collapse_toggle, point);
        if collapse_toggle || endpoint_id == self.active_endpoint_id {
            if !self.collapsed_endpoints.remove(&endpoint_id) {
                self.collapsed_endpoints.insert(endpoint_id.clone());
            }
            outcome.repaint = true;
            if !collapse_toggle && endpoint_id.is_local() {
                self.activate_endpoint(endpoint_id, outcome);
            }
        } else if endpoint_id.is_local() || self.endpoint_is_online(&endpoint_id) {
            outcome.actions.push(ClientShellAction::ActivateEndpoint {
                endpoint_id,
                target: None,
            });
        } else {
            let label = self.endpoint_label(&endpoint_id).to_owned();
            self.receive_endpoint_unavailable(format!("{label} is not ready"));
            outcome.repaint = true;
        }
        true
    }

    pub(super) fn handle_endpoint_agent_click(
        &mut self,
        point: (u16, u16),
        outcome: &mut ClientShellInput,
    ) -> bool {
        let Some((endpoint_id, pane_id)) = self
            .hits
            .endpoint_agents
            .iter()
            .find(|(rect, _, _)| super::contains(*rect, point))
            .map(|(_, endpoint_id, pane_id)| (endpoint_id.clone(), pane_id.clone()))
        else {
            return false;
        };
        self.focus_or_activate(
            endpoint_id,
            ClientEndpointFocusTarget::Pane(pane_id),
            outcome,
        );
        true
    }

    pub(super) fn handle_endpoint_navigation(
        &mut self,
        action: crate::input::KeybindAction,
        outcome: &mut ClientShellInput,
    ) -> bool {
        use crate::input::KeybindAction;
        if !self.multi_endpoint_active() {
            return false;
        }
        if matches!(
            action,
            KeybindAction::PreviousWorkspace | KeybindAction::NextWorkspace
        ) {
            let workspaces = self
                .endpoints
                .iter()
                .filter(|endpoint| endpoint.status == ClientEndpointStatus::Online)
                .flat_map(|endpoint| {
                    endpoint
                        .snapshot
                        .as_deref()
                        .map_or_else(Vec::new, |snapshot| {
                            parent_workspace_entries(snapshot)
                                .into_iter()
                                .filter_map(|entry| {
                                    snapshot.workspaces.get(entry.index).map(|workspace| {
                                        (
                                            endpoint.endpoint_id.clone(),
                                            workspace.workspace_id.clone(),
                                        )
                                    })
                                })
                                .collect()
                        })
                })
                .collect::<Vec<_>>();
            if workspaces.is_empty() {
                return true;
            }
            let focused = self.snapshot.as_deref().and_then(|snapshot| {
                workspace_navigation_parent(snapshot, snapshot.focused_workspace_id.as_deref()?)
            });
            let current = workspaces.iter().position(|(endpoint_id, workspace_id)| {
                endpoint_id == &self.active_endpoint_id && Some(workspace_id.as_str()) == focused
            });
            let next = match (current, action) {
                (Some(index), KeybindAction::PreviousWorkspace) => {
                    (index + workspaces.len() - 1) % workspaces.len()
                }
                (Some(index), KeybindAction::NextWorkspace) => (index + 1) % workspaces.len(),
                (None, KeybindAction::PreviousWorkspace) => workspaces.len() - 1,
                (None, KeybindAction::NextWorkspace) => 0,
                _ => unreachable!("endpoint workspace navigation"),
            };
            let (endpoint_id, workspace_id) = workspaces[next].clone();
            self.focus_or_activate(
                endpoint_id,
                ClientEndpointFocusTarget::Workspace(workspace_id),
                outcome,
            );
            return true;
        }
        if matches!(
            action,
            KeybindAction::PreviousAgent | KeybindAction::NextAgent | KeybindAction::FocusAgent(_)
        ) {
            let agents = super::aggregate_navigation::online_agent_targets(
                &self.endpoints,
                &self.active_endpoint_id,
                self.config.agent_panel_sort,
            );
            if agents.is_empty() {
                return true;
            }
            let next = match action {
                KeybindAction::FocusAgent(index) => {
                    if index >= agents.len() {
                        return true;
                    }
                    index
                }
                KeybindAction::PreviousAgent | KeybindAction::NextAgent => {
                    let focused = self
                        .snapshot
                        .as_deref()
                        .and_then(|snapshot| snapshot.focused_pane_id.as_deref());
                    let current = agents.iter().position(|target| {
                        target.endpoint_id == self.active_endpoint_id
                            && Some(target.pane_id.as_str()) == focused
                    });
                    match (current, action) {
                        (Some(index), KeybindAction::PreviousAgent) => {
                            (index + agents.len() - 1) % agents.len()
                        }
                        (Some(index), KeybindAction::NextAgent) => (index + 1) % agents.len(),
                        (None, KeybindAction::PreviousAgent) => agents.len() - 1,
                        _ => 0,
                    }
                }
                _ => unreachable!("endpoint agent navigation"),
            };
            let target = &agents[next];
            if self.focus_or_activate(
                target.endpoint_id.clone(),
                ClientEndpointFocusTarget::Pane(target.pane_id.clone()),
                outcome,
            ) {
                if target.endpoint_id == self.active_endpoint_id {
                    self.reveal_endpoint_agent(
                        &target.endpoint_id,
                        &target.pane_id,
                        self.hits.agent_body.height,
                    );
                } else {
                    self.pending_agent_reveal =
                        Some((target.endpoint_id.clone(), target.pane_id.clone()));
                }
                outcome.repaint = true;
            }
            return true;
        }
        false
    }

    pub(super) fn activate_endpoint(
        &mut self,
        endpoint_id: ClientEndpointId,
        outcome: &mut ClientShellInput,
    ) -> bool {
        self.pending_workspace_highlight = None;
        self.pending_agent_reveal = None;
        let online = self.endpoint_is_online(&endpoint_id);
        if !online && !endpoint_id.is_local() {
            let label = self.endpoint_label(&endpoint_id).to_owned();
            self.receive_endpoint_unavailable(format!("{label} is not ready"));
            outcome.repaint = true;
            return false;
        }
        if (endpoint_id.is_local() && (self.multi_endpoint_active() || !online))
            || endpoint_id != self.active_endpoint_id
        {
            outcome.actions.push(ClientShellAction::ActivateEndpoint {
                endpoint_id,
                target: None,
            });
        }
        true
    }

    pub(super) fn focus_or_activate(
        &mut self,
        endpoint_id: ClientEndpointId,
        target: ClientEndpointFocusTarget,
        outcome: &mut ClientShellInput,
    ) -> bool {
        #[cfg(windows)]
        if !self.notification_target_is_current(&endpoint_id, &target) {
            return false;
        }
        self.pending_workspace_highlight = None;
        self.pending_agent_reveal = None;
        let online = self.endpoint_is_online(&endpoint_id);
        if !online && !endpoint_id.is_local() {
            let label = self.endpoint_label(&endpoint_id).to_owned();
            self.receive_endpoint_unavailable(format!("{label} is not ready"));
            outcome.repaint = true;
            return false;
        }
        // Local can still be displayed while a remote activation is pending.
        // Route explicit selections through the runtime so they can cancel that handoff.
        if endpoint_id == self.active_endpoint_id
            && !(endpoint_id.is_local() && (self.multi_endpoint_active() || !online))
        {
            let method = match target {
                ClientEndpointFocusTarget::Workspace(workspace_id) => {
                    crate::api::schema::Method::WorkspaceFocus(
                        crate::api::schema::WorkspaceTarget { workspace_id },
                    )
                }
                ClientEndpointFocusTarget::Tab(tab_id) => {
                    crate::api::schema::Method::TabFocus(crate::api::schema::TabTarget { tab_id })
                }
                ClientEndpointFocusTarget::Pane(pane_id) => {
                    crate::api::schema::Method::PaneFocus(crate::api::schema::PaneTarget {
                        pane_id,
                    })
                }
                #[cfg(windows)]
                ClientEndpointFocusTarget::Notification { pane_id, .. } => {
                    crate::api::schema::Method::PaneFocus(crate::api::schema::PaneTarget {
                        pane_id,
                    })
                }
            };
            self.push_endpoint_method(method, outcome);
        } else {
            outcome.actions.push(ClientShellAction::ActivateEndpoint {
                endpoint_id,
                target: Some(target),
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::KeybindAction;
    use crate::protocol::ClientShellWorktree;

    fn grouped_snapshot(focused: &str) -> ClientShellSnapshot {
        let mut snapshot = super::super::tests::snapshot();
        let template = snapshot.workspaces[0].clone();
        snapshot.workspaces = [
            ("first", None),
            ("parent", Some(("repo", false))),
            ("child", Some(("repo", true))),
            ("last", None),
            ("orphan", Some(("other", true))),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (id, worktree))| {
            let mut workspace = template.clone();
            workspace.workspace_id = id.into();
            workspace.number = index + 1;
            workspace.focused = id == focused;
            workspace.worktree = worktree.map(|(key, is_linked_worktree)| ClientShellWorktree {
                key: key.into(),
                label: key.into(),
                is_linked_worktree,
            });
            workspace
        })
        .collect();
        snapshot.focused_workspace_id = Some(focused.into());
        snapshot
    }

    #[test]
    fn parent_workspace_shortcuts_skip_children_and_preserve_orphans() {
        for collapsed in [false, true] {
            for (focused, action, expected) in [
                ("first", KeybindAction::NextWorkspace, "parent"),
                ("parent", KeybindAction::NextWorkspace, "last"),
                ("child", KeybindAction::NextWorkspace, "last"),
                ("child", KeybindAction::PreviousWorkspace, "first"),
                ("last", KeybindAction::PreviousWorkspace, "parent"),
                ("last", KeybindAction::NextWorkspace, "orphan"),
                ("orphan", KeybindAction::PreviousWorkspace, "last"),
                ("orphan", KeybindAction::NextWorkspace, "first"),
                ("first", KeybindAction::PreviousWorkspace, "orphan"),
            ] {
                let mut state =
                    ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
                if collapsed {
                    state.collapsed_groups.insert("repo".into());
                }
                state.set_snapshot(Box::new(grouped_snapshot(focused)));
                assert!(
                    matches!(
                        state.endpoint_method_for_action(action),
                        Some(crate::api::schema::Method::WorkspaceFocus(target))
                            if target.workspace_id == expected
                    ),
                    "{focused} {action:?}, collapsed={collapsed}"
                );
            }
        }
    }

    #[test]
    fn parent_workspace_shortcuts_with_one_group_return_its_parent() {
        for focused in ["parent", "child"] {
            let mut snapshot = grouped_snapshot(focused);
            snapshot
                .workspaces
                .retain(|workspace| matches!(workspace.workspace_id.as_str(), "parent" | "child"));
            let mut state =
                ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
            state.set_snapshot(Box::new(snapshot));
            for action in [
                KeybindAction::PreviousWorkspace,
                KeybindAction::NextWorkspace,
            ] {
                assert!(matches!(
                    state.endpoint_method_for_action(action),
                    Some(crate::api::schema::Method::WorkspaceFocus(target))
                        if target.workspace_id == "parent"
                ));
            }
        }
    }

    #[test]
    fn parent_workspace_shortcuts_keep_duplicate_parent_identity() {
        let mut snapshot = grouped_snapshot("duplicate");
        let mut duplicate = snapshot.workspaces[1].clone();
        duplicate.workspace_id = "duplicate".into();
        duplicate.focused = true;
        snapshot.workspaces.push(duplicate);
        let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
        state.set_snapshot(Box::new(snapshot));
        assert!(matches!(
            state.endpoint_method_for_action(KeybindAction::PreviousWorkspace),
            Some(crate::api::schema::Method::WorkspaceFocus(target))
                if target.workspace_id == "parent"
        ));
    }

    #[test]
    fn parent_workspace_shortcuts_cross_endpoints_without_confusing_workspace_ids() {
        use crate::client::endpoint::{ProfileId, SavedSshEndpoint};

        let profile = SavedSshEndpoint {
            id: ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
            label: "Build".into(),
            target: "dev@build.example".into(),
            session: "agents".into(),
            enabled: true,
        };
        let remote = ClientEndpointId::Ssh(profile.id.clone());
        for (active, focused, action, expected_endpoint, expected_workspace) in [
            (
                ClientEndpointId::Local,
                "child",
                KeybindAction::NextWorkspace,
                ClientEndpointId::Local,
                "last",
            ),
            (
                ClientEndpointId::Local,
                "child",
                KeybindAction::PreviousWorkspace,
                ClientEndpointId::Local,
                "first",
            ),
            (
                ClientEndpointId::Local,
                "orphan",
                KeybindAction::NextWorkspace,
                remote.clone(),
                "first",
            ),
            (
                remote.clone(),
                "child",
                KeybindAction::NextWorkspace,
                remote.clone(),
                "last",
            ),
            (
                remote.clone(),
                "first",
                KeybindAction::PreviousWorkspace,
                ClientEndpointId::Local,
                "orphan",
            ),
            (
                remote.clone(),
                "orphan",
                KeybindAction::NextWorkspace,
                ClientEndpointId::Local,
                "first",
            ),
        ] {
            let mut state =
                ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
            state.set_endpoint_catalog(std::slice::from_ref(&profile));
            state.set_snapshot(Box::new(grouped_snapshot(focused)));
            state.set_endpoint_status(&remote, ClientEndpointStatus::Online);
            let mut remote_snapshot = grouped_snapshot(focused);
            remote_snapshot.boot_id = "remote-boot".into();
            state.set_endpoint_snapshot(&remote, Box::new(remote_snapshot));
            if active == remote {
                assert!(state.activate_endpoint_projection(&remote));
            }
            let mut outcome = ClientShellInput::default();
            assert!(state.handle_endpoint_navigation(action, &mut outcome));
            let [result] = outcome.actions.as_slice() else {
                panic!("expected exactly one navigation action");
            };
            match result {
                ClientShellAction::ActivateEndpoint {
                    endpoint_id,
                    target: Some(ClientEndpointFocusTarget::Workspace(workspace_id)),
                } => {
                    assert_eq!(endpoint_id, &expected_endpoint);
                    assert_eq!(workspace_id, expected_workspace);
                }
                ClientShellAction::Endpoint {
                    endpoint_id,
                    request,
                    ..
                } => {
                    assert_eq!(endpoint_id, &expected_endpoint);
                    assert!(matches!(
                        &request.method,
                        crate::api::schema::Method::WorkspaceFocus(target)
                            if target.workspace_id == expected_workspace
                    ));
                }
                _ => panic!("expected a workspace focus action"),
            }
        }
    }
}
