# Custom workspace build

This fork is based on upstream Herdr 0.9.5. It uses `herdr-custom` for release
configuration and state directories (`herdr-custom-dev` for debug builds).
Existing Herdr sessions are independent. Keep the Apache 2.0 license and upstream
notices when distributing the fork.

## Workspace navigation

Expanded desktop sidebars show each workspace's tabs underneath it, including
tabs inside grouped worktree workspaces. Click a tab to focus it; right-click a
tab on the active machine for the usual tab menu. The mouse wheel scrolls the
sidebar. Workspace arrows collapse tabs and grouped worktrees. The existing tab
bar and tab shortcuts continue to work.

Previous/next workspace shortcuts cycle through parent workspaces. From a child
worktree, navigation starts at its parent. A worktree without an open parent
remains reachable as a top-level workspace. Explicit workspace selection and the
workspace picker still reach children.

## Native worktree launcher

Bind `open_worktree` to the former launcher shortcut:

```toml
[keys]
open_worktree = "prefix+shift+f"
```

Remove the custom command using the same shortcut. From the parent workspace,
open the native worktree picker and choose an existing checkout. The CLI/API
`worktree open` path applies the same behavior.

When a newly opened checkout contains both `backend/canary` and
`backend/pms-gateway`, its initial tab becomes **Servers**, with a Canary shell
above a PMS Gateway shell. This preserves the old script's behavior: it opens
shells but does not start server commands. Other repositories use the ordinary
single-shell layout. Reopening an existing workspace preserves its layout.
Layout failures clean up newly created panes/workspaces; no checkout is deleted.

## Build and update

Install Rust 1.96.1, Zig 0.16.0, and `just`. Set `ZIG` if Zig 0.16.0 is not on
`PATH`. Then run `just build`, and install `target/release/herdr` as
`~/.local/bin/herdr-custom`. Run `herdr-custom` from a fresh terminal.

Disable upstream update notifications in the custom config:

```toml
[update]
version_check = false
```

The upstream self-updater is disabled in this fork to prevent replacing the
custom binary. Fetch upstream tags, integrate changes into the custom branch,
run validation, and rebuild to update it. Existing plugins can be installed
separately; the board and annotation tools were not rewritten in this version.
