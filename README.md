# constatus

A configurable status line for Claude Code that displays context, token usage, cost, git branch, and hosting-forge information (with optional GitLab pipeline/MR status).

![constatus screenshot](assets/screenshot.png)

*The screenshot shows the default (non-GitLab) preset; the `{forge}`, `{pipeline}`, and `{mr}` fields are illustrated in the examples below.*

## Installation

### With Cargo

```bash
cargo install --path .
```

### With Nix

Build and install via the flake:

```bash
nix build                # Build the package
nix run                  # Run directly
nix shell              # Enter dev environment
nix flake show         # View all outputs
```

Or add to your flake inputs:

```nix
constatus.url = "github:user/constatus";
```

## Usage

constatus reads JSON from stdin and formats a status line based on your configuration:

```bash
echo '{"model":{"display_name":"Claude Opus 4.6"},"workspace":{"current_dir":"/home/user/project"},"context_window":{"remaining_percentage":70}}' | constatus
```

## Output Examples

**Default preset:**
```
󰇼 Claude Opus 4.6 │  project │  main │  gitlab │ [███░░░░░░░] 30% │ 256K cached │  passed │ 💰 $0.15
```

## Placeholders

The following placeholders are available in format strings:

- `{model}` — AI model name
- `{dir}` — Current directory name
- `{branch}` — Git branch name
- `{forge}` — Hosting forge detected from the git remote (GitLab, GitHub, Bitbucket, …)
- `{context}` — Context window usage percentage
- `{bar}` — Visual progress bar
- `{cache}` — Cache read tokens
- `{input}` — Input tokens
- `{output}` — Output tokens
- `{cost}` — Estimated cost
- `{duration}` — Time since conversation started
- `{pipeline}` — Latest GitLab pipeline status for the current branch *(requires a token, see [GitLab integration](#gitlab-integration))*
- `{mr}` — Count of open GitLab merge requests for the current branch, e.g. `2 MRs` *(requires a token)*

Prefix with `?` to hide empty sections: `?{branch}` only shows if a branch is found.

## Presets

**Minimal:**
```
{model} | {dir} | ?{branch}
```

**Default:**
```
{model} | {dir} | ?{branch} | ?{forge} | {bar} {context}% | ?{cache} cached | ?{pipeline} | ?{cost}
```

**Full:**
```
{model} | {dir} | ?{branch} | ?{forge} | {bar} {context}% | ?{cache} cached | ?{pipeline} | ?{mr} | ?In: {input} | ?Out: {output} | ?{cost} | ?{duration}
```

## Options

```
-f, --format <FORMAT>       Custom format string (overrides preset)
-p, --preset <PRESET>       Preset: minimal, default, full [default: default]
-F, --fallback <TEXT>       Fallback when no data available [default: "Claude Ready"]
-s, --separator <SEP>       Section separator [default: " │ "]
-c, --color <MODE>          Color mode: always, never, auto [default: auto]
-i, --icons                 Enable Nerd Font icons [default: true]
--no-icons                  Disable icons
-w, --bar-width <N>         Progress bar width in chars [default: 10]
--no-gitlab                 Disable the GitLab API integration ({pipeline}, {mr})
--gitlab-timeout <SECS>     Per-request timeout for GitLab API calls [default: 2]
--gitlab-cache <SECS>       Reuse cached GitLab API results for N seconds [default: 30]
```

## Input Format

constatus expects JSON with the following structure (all fields optional):

```json
{
  "model": {
    "display_name": "Claude Opus 4.6"
  },
  "workspace": {
    "current_dir": "/path/to/project"
  },
  "context_window": {
    "remaining_percentage": 70,
    "current_usage": {
      "cache_read_input_tokens": 256000,
      "input_tokens": 1500,
      "output_tokens": 800
    }
  },
  "conversation": {
    "started_at": "2025-03-07T14:30:00Z"
  }
}
```

## Colors

Color output adapts based on context usage:
- **Green:** Low usage (< 50%)
- **Yellow:** Medium usage (50–70%)
- **Red:** High usage (≥ 70%)

The `{branch}` field appears in yellow when a git branch is detected.

The `{forge}` field is colored per host — GitLab orange, GitHub white, Bitbucket blue, and any other host shows a generic git icon in yellow.

## GitLab integration

constatus detects the hosting forge from your git remote entirely offline: the
`{forge}` placeholder shows a GitLab/GitHub/Bitbucket icon based on
`git remote get-url origin`. No token or network is required for `{forge}`.

The optional `{pipeline}` and `{mr}` placeholders show, for the **current
branch**, the latest GitLab CI pipeline status and the number of open merge
requests. These query the GitLab API and activate only when **all** of the
following hold:

- the remote host is trusted for the API (see below),
- a git branch is detected (a detached HEAD counts as no branch), and
- an API token is available in the environment.

The token is read from the first of `GITLAB_TOKEN`, `CONSTATUS_GITLAB_TOKEN`, or
`CI_JOB_TOKEN`. Create a [personal access token](https://docs.gitlab.com/ee/user/profile/personal_access_tokens.html)
with the `read_api` scope:

```bash
export GITLAB_TOKEN=glpat-xxxxxxxxxxxxxxxxxxxx
```

**Trusted hosts.** So your token is never sent to a look-alike domain, the
authenticated API is called only for `gitlab.com` and its subdomains by default.
For a **self-hosted** instance, opt in explicitly with a comma-separated
allowlist of exact hostnames (the offline `{forge}` icon needs no such opt-in):

```bash
export CONSTATUS_GITLAB_HOST=gitlab.example.com
```

**How it works (and why it's safe for a status line):**

- Requests are made by shelling out to `curl` (no extra build dependencies);
  the token is passed via curl's stdin config, never in the process arguments.
- Each request is bounded by `--gitlab-timeout` (default 2s, and the pipeline +
  MR requests run concurrently) so a render never hangs. Results are cached
  under `$XDG_CACHE_HOME/constatus` (or `~/.cache/constatus`) for
  `--gitlab-cache` seconds (default 30s), so a frequently-redrawn status line
  makes at most one round of API calls (pipeline + MRs) per cache window.
- Anything missing or failing (no token, no `curl`, network error, untrusted or
  non-GitLab remote) is silently omitted — pair the placeholders with `?` to
  hide them.
- Disable the API path entirely with `--no-gitlab`; `{forge}` still works.

`{pipeline}` is colored by status (green = passed, red = failed, yellow =
running/pending, magenta = canceled/manual/skipped); `{mr}` shows the open MR
count for the current branch (e.g. ` 2 MRs`).

## Features

- **Git branch detection:** Runs `git rev-parse --abbrev-ref HEAD` in the workspace directory
- **Forge detection:** Identifies GitLab/GitHub/Bitbucket from the git remote, fully offline
- **GitLab status (optional):** Pipeline and merge-request status for the current branch via the GitLab API, gated on a token and cached
- **Token tracking:** Displays input, output, and cache read tokens with abbreviations (k, M)
- **Cost estimation:** Calculates approximate API costs based on token counts
- **Graceful degradation:** Missing data is silently omitted (especially useful with `?` prefix)
- **Nerd Font icons:** Optional icons for visual appeal (disable with `--no-icons`)
- **Color control:** Auto-detects terminal support or override with `--color`

## Development

Enter the dev shell with all build tools:

```bash
nix flake update    # Update dependencies
direnv allow        # Auto-load environment (requires direnv)
```

Available shell scripts:
- `dx` — Edit `flake.nix`
- `rx` — Edit `Cargo.toml`

Run tests and build:

```bash
cargo build
cargo test
nix build
```
