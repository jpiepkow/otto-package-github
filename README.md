# Otto GitHub Package

First-party Otto package for scoped GitHub code investigation and approved authoring.

The package exposes `com.otto.github` as a JSON-RPC tool package. It uses the host `gh` CLI for all GitHub API calls and host-side `git` operations, so GitHub credentials stay on the host and never enter worker containers.

## Capabilities

- Read capability: `cap.default.github.read`
- Write capability: `cap.default.github.write`
- Setup check: `setup.default.github.ready`
- Runtime command produced by build: `bin/otto-tool-github`

## Auth Modes

- HOST: uses the machine's existing `gh auth` session.
- TOKEN: uses a fine-grained PAT materialized by Otto from `credential_ref` and passed to host-side commands as `GH_TOKEN`.

Run setup checks before using the package on a real host. The real setup check verifies `gh --version` and `gh auth status`, returning `gh_missing`, `not_authenticated`, or `ok` with `gh_version` and `gh_authenticated` details.

## Tool Surface

Read tools do not require approval:

- `search_code`
- `fetch_file`
- `list_commits`
- `list_pull_requests`
- `list_issues`
- `prepare_checkout`

Write tools require `cap.default.github.write` and default to approval-required:

- `git_stage`
- `git_commit`
- `branch_create`
- `git_push`
- `pull_request_open`
- `pull_request_merge`
- `comment`
- `issue_create`
- `label_edit`
- `branch_delete`

## Checkout Mount Model

`prepare_checkout` creates a host-owned checkout under the active run root and returns the container mount path:

```text
/otto/checkout/<owner>/<repo>
```

The worker receives the checkout as a read-write bind mount. The agent reads and edits files through that mount, while the package performs GitHub and git mutations on the host with host credentials. See `NOTES.md` for the detailed mount, credential, and cleanup model.

## Install In Otto

The canonical package repository is:

```text
https://github.com/jpiepkow/otto-package-github
```

During Phase 25, development lives in `extensions/com.otto.github/`; extraction to the separate package repository is a follow-up packaging step through the Phase 58 GitHub package install path.

## Build

```sh
cargo build --release
mkdir -p bin
cp target/release/otto-tool-github bin/otto-tool-github
```

## Test

Default tests are offline and deterministic:

```sh
cargo test --manifest-path extensions/com.otto.github/Cargo.toml
```

Live smoke tests are opt-in and require authenticated `gh`:

```sh
cargo test --manifest-path extensions/com.otto.github/Cargo.toml --test runtime_contract -- --ignored live
```
