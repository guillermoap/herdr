use std::path::Path;

use crate::api::schema::{
    ErrorResponse, EventData, EventEnvelope, EventKind, LayoutApplyParams, LayoutNode, LayoutPane,
    ResponseResult, SplitDirection, SuccessResponse,
};
use crate::app::App;

pub(super) fn canary_server_layout(checkout: &Path) -> Option<LayoutNode> {
    let canary = checkout.join("backend/canary");
    let gateway = checkout.join("backend/pms-gateway");
    if !canary.is_dir() || !gateway.is_dir() {
        return None;
    }
    let pane = |cwd: &Path, label: &str| {
        Box::new(LayoutNode::Pane {
            pane: LayoutPane {
                cwd: Some(cwd.display().to_string()),
                label: Some(label.into()),
                ..Default::default()
            },
        })
    };
    Some(LayoutNode::Split {
        direction: SplitDirection::Down,
        ratio: 0.5,
        first: pane(&canary, "Canary"),
        second: pane(&gateway, "PMS Gateway"),
    })
}

impl App {
    pub(super) fn apply_worktree_server_layout(
        &mut self,
        ws_idx: usize,
        root: LayoutNode,
    ) -> Result<(), String> {
        let tab_id = self
            .public_tab_id(ws_idx, 0)
            .ok_or_else(|| "new worktree has no initial tab".to_string())?;
        let response = self.handle_layout_apply(
            "worktree-server-layout".into(),
            LayoutApplyParams {
                workspace_id: None,
                tab_id: Some(tab_id),
                tab_label: Some("Servers".into()),
                focus: false,
                root,
            },
        );
        match serde_json::from_str::<SuccessResponse>(&response) {
            Ok(SuccessResponse {
                result: ResponseResult::LayoutApply { .. },
                ..
            }) => Ok(()),
            _ => Err(serde_json::from_str::<ErrorResponse>(&response)
                .map(|response| response.error.message)
                .unwrap_or_else(|_| "could not create worktree server layout".into())),
        }
    }

    pub(super) fn rollback_worktree_server_workspaces(
        &mut self,
        mut indices: Vec<usize>,
        previous_workspace_id: Option<&str>,
    ) {
        indices.sort_unstable();
        let workspaces = indices
            .iter()
            .map(|&idx| self.workspace_info(idx))
            .collect::<Vec<_>>();
        self.state.close_workspaces(indices);
        self.shutdown_detached_terminal_runtimes();
        if let Some(idx) = previous_workspace_id.and_then(|id| {
            self.state
                .workspaces
                .iter()
                .position(|workspace| workspace.id == id)
        }) {
            self.state.switch_workspace(idx);
        }
        for workspace in workspaces {
            self.emit_event(EventEnvelope {
                event: EventKind::WorkspaceClosed,
                data: EventData::WorkspaceClosed {
                    workspace_id: workspace.workspace_id.clone(),
                    workspace: Some(workspace),
                },
            });
        }
    }
}
