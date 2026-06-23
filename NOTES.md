# GitHub Checkout Mount Reference

`prepare_checkout` fills a host-owned checkout directory for the active run and returns the path the agent should use inside the worker container. The package receives `run_id` through `ToolInvokeParams`; it does not accept run identity from model/tool arguments.

## Run Spec Wiring

A job or session that wants `prepare_checkout` must declare a `WorkerWorkspaceDirectory` before the worker starts:

```json
{
  "source": "$OTTO_ROOT/runs/<run_id>/github/checkout",
  "target": "/otto/checkout",
  "access_mode": "mount",
  "read_only": false
}
```

`access_mode = "mount"` is Otto's BIND mount mode. It must not be `copy`: `prepare_checkout` clones on the host after container spawn, and a live bind mount is what makes the newly populated tree appear in the already-running worker.

The checkout tool then writes repositories under:

```text
$OTTO_ROOT/runs/<run_id>/github/checkout/<owner>/<repo>
```

and returns:

```text
/otto/checkout/<owner>/<repo>
```

The agent reads and edits files at the returned `mount_path`. The package does not read file contents from the checkout for API read tools.

## Core Paths

- `crates/otto-control-plane/src/app.rs:19630` defines the per-run root shape under `$OTTO_ROOT/runs/<run_id>`.
- `crates/otto-control-plane/src/app.rs:19781` materializes workspace directories before spawn.
- `crates/otto-control-plane/src/app.rs:19797` converts `workspace_directories` into `WorkerMount` entries at spawn.
- `crates/otto-core/src/providers/worker.rs:116` defines `WorkerWorkspaceDirectory`.
- `crates/otto-core/src/providers/worker.rs:210` carries `WorkerCleanupParams.remove_workspace`.

## Credential Boundary

All `gh` and `git` operations run in the host package process. HOST mode uses the host `gh` auth setup. TOKEN mode injects the PAT only into host-side commands and rewrites the cloned repository remote to `https://github.com/<owner>/<repo>.git` immediately after clone, so `git remote -v` in the mounted checkout does not expose the PAT.

Credentials never enter the worker container. The container sees only the populated checkout directory at `/otto/checkout/<owner>/<repo>`.

## Cleanup

The checkout source lives under the run root. When the worker lifecycle ends with `remove_workspace = true`, existing run-workspace cleanup removes `$OTTO_ROOT/runs/<run_id>` and therefore the GitHub checkout tree. No package-specific cleanup hook is required.

Live mount visibility and cleanup remain Manual-Only UAT items from Phase 25 validation: D-09 verifies the mounted checkout is visible and credential-free in the worker, and D-11 verifies the per-run checkout is removed on run teardown.

## Repo Delete Omission

The D-05 write-capability surface includes repository deletion as a theoretically approval-gated destructive operation. v1 wiring deliberately omits a `repo_delete` tool and handler while keeping the write capability ready for future expansion.

Rationale: repository deletion is irreversible at the package boundary and does not need to be exercised in fake/live smoke tests for the authoring workflow. The implemented v1 remote write set covers PR open/merge, issue/PR comments, issue creation, label edits, and branch deletion. If repo deletion is added later, it must be registered as a separate `requires_approval = true` tool with an unmistakably destructive description and a dedicated UAT path.

## Package Repo

`com.otto.github` is packaged from:

```text
https://github.com/jpiepkow/otto-package-github
```

Install through Otto's GitHub package install path, consistent with the Phase 59 package-decoupling model used for other first-party packages.

Package invariants:

- `otto.toml` declares the build command and runtime binary without relying on monorepo-only paths.
- `Cargo.toml` pins `otto-extension-sdk` to an Otto repo tag that includes `ToolInvokeParams.run_id`.
- The extracted repository has no path dependencies on the Otto monorepo.
- `README.md` documents install, auth modes, setup check, live smoke tests, and the read/write tool surface.
