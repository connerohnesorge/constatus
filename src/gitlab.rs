//! Optional GitLab API integration: latest pipeline status and open
//! merge-request count for the current branch.
//!
//! Requests are made in-process with `ureq` (blocking, rustls) — no subprocess
//! to spawn and the token never leaves this process. The pipeline and MR calls
//! run concurrently, each bounded by an overall timeout so a render never
//! hangs, and results are cached on disk with a short TTL so a frequently
//! rendered status line makes at most one round of calls per cache window.

use crate::color::Color;
use crate::forge::Remote;
use crate::icons;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Tunables for the optional API integration.
pub struct Config {
    /// Hard cap (seconds) on each API request — bounds render latency.
    pub timeout_secs: u64,
    /// How long a cached result is reused before re-fetching (seconds).
    pub cache_secs: u64,
}

/// What we display: the latest pipeline status (if any) and the number of
/// open merge requests whose source branch is the current branch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub pipeline: Option<String>,
    pub mr_count: Option<u64>,
}

/// Resolve a GitLab token from the environment, most-specific first.
pub fn token_from_env() -> Option<String> {
    for var in ["GITLAB_TOKEN", "CONSTATUS_GITLAB_TOKEN", "CI_JOB_TOKEN"] {
        if let Ok(v) = std::env::var(var)
            && !v.trim().is_empty()
        {
            return Some(v);
        }
    }
    None
}

/// Fetch (or serve from cache) the pipeline + MR status for `branch`.
/// Caller guarantees `remote.forge == Forge::GitLab` and a non-empty token.
pub fn fetch_status(remote: &Remote, branch: &str, token: &str, cfg: &Config) -> Status {
    // Never send the token to a host we don't trust, and validate the host and
    // token before they enter a URL or header (defense in depth).
    if !host_is_gitlab_api_allowed(&remote.host)
        || !host_is_safe(&remote.host)
        || !token_is_safe(token)
    {
        return Status::default();
    }

    let key = cache_key(&remote.host, &remote.project, branch);

    if let Some(cached) = read_cache(&key, cfg.cache_secs) {
        return cached;
    }

    // Issue both requests concurrently (pipeline on a worker thread, MRs on
    // this one) so a cache miss blocks for ~1x the timeout instead of 2x.
    let timeout = std::time::Duration::from_secs(cfg.timeout_secs.max(1));
    // Disable redirects so a 3xx from the (trusted) API host can never re-send
    // the PRIVATE-TOKEN header to another host — matching curl's no-`-L`
    // behavior, since these GET endpoints only ever return 200 in normal use.
    // https_only forbids an http downgrade; timeout_connect bounds the TCP
    // connect (the request `.timeout()` only covers TLS + read) so a
    // black-holed host can't stall the render.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .redirects(0)
        .https_only(true)
        .build();
    let pipeline_url = api_url(&remote.host, &pipeline_path(&remote.project, branch));
    let mr_url = api_url(&remote.host, &mr_path(&remote.project, branch));

    let worker = {
        let agent = agent.clone();
        let token = token.to_string();
        std::thread::spawn(move || http_get(&agent, &pipeline_url, &token, timeout))
    };
    let mr_count = http_get(&agent, &mr_url, token, timeout)
        .as_deref()
        .map(parse_mr_count);
    let pipeline = worker
        .join()
        .ok()
        .flatten()
        .as_deref()
        .and_then(parse_pipeline_status);

    let status = Status { pipeline, mr_count };
    // Always cache (even an empty/failed result) to back off and avoid
    // hammering the API on every render while it is unavailable.
    write_cache(&key, &status);
    status
}

// ---- URL building (pure) ---------------------------------------------------

fn pipeline_path(project: &str, branch: &str) -> String {
    format!(
        "/projects/{}/pipelines?ref={}&per_page=1&order_by=id&sort=desc",
        percent_encode(project),
        percent_encode(branch),
    )
}

fn mr_path(project: &str, branch: &str) -> String {
    format!(
        "/projects/{}/merge_requests?state=opened&source_branch={}&per_page=100",
        percent_encode(project),
        percent_encode(branch),
    )
}

// ---- host trust (don't send the token to look-alike hosts) -----------------

/// Whether the authenticated API may be called for `host`. `Forge::from_host`
/// classifies the *display* icon by a loose `contains("gitlab")` substring, but
/// the token must only ever be sent to a host we actually trust — otherwise a
/// hostile remote like `evil-gitlab.com` would receive the credential.
fn host_is_gitlab_api_allowed(host: &str) -> bool {
    is_canonical_gitlab_host(host) || host_in_env_allowlist(host)
}

fn is_canonical_gitlab_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "gitlab.com" || h.ends_with(".gitlab.com")
}

/// Self-hosted instances opt in via `CONSTATUS_GITLAB_HOST` (comma/space
/// separated list of exact hostnames).
fn host_in_env_allowlist(host: &str) -> bool {
    std::env::var("CONSTATUS_GITLAB_HOST")
        .map(|list| host_matches_allowlist(host, &list))
        .unwrap_or(false)
}

fn host_matches_allowlist(host: &str, list: &str) -> bool {
    list.split([',', ' ', '\n', '\t'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|allowed| allowed.eq_ignore_ascii_case(host))
}

/// A hostname is safe to embed in a request URL when it contains only the
/// characters a real DNS host uses — never quotes, spaces, or newlines.
fn host_is_safe(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

/// A token is safe to send as a header value when every byte is printable
/// ASCII (no spaces, control chars, or `"`), matching real GitLab tokens.
fn token_is_safe(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic() && b != b'"')
}

/// Percent-encode per RFC 3986, leaving only the unreserved set unescaped.
/// This turns `group/sub/project` into `group%2Fsub%2Fproject` (the form
/// GitLab expects for a URL-encoded project id) and makes branch names safe.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---- Response parsing (pure) -----------------------------------------------

/// First (latest) pipeline's `status` field, if the array is non-empty.
fn parse_pipeline_status(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let first = v.as_array()?.first()?;
    first.get("status")?.as_str().map(str::to_string)
}

/// Number of open merge requests returned (array length).
fn parse_mr_count(json: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len() as u64))
        .unwrap_or(0)
}

// ---- Presentation (pure) ---------------------------------------------------

/// Short human label for a raw GitLab pipeline status.
pub fn pipeline_label(status: &str) -> &'static str {
    match status {
        "success" => "passed",
        "failed" => "failed",
        "running" => "running",
        "pending" | "created" | "preparing" | "waiting_for_resource" | "scheduled" => "pending",
        "canceled" | "cancelled" => "canceled",
        "skipped" => "skipped",
        "manual" => "manual",
        _ => "pipeline",
    }
}

pub fn pipeline_icon(status: &str) -> &'static str {
    match status {
        "success" => icons::PIPE_OK,
        "failed" => icons::PIPE_FAIL,
        "running" | "pending" | "created" | "preparing" | "waiting_for_resource" | "scheduled" => {
            icons::PIPE_RUN
        }
        _ => icons::PIPE_WARN,
    }
}

pub fn pipeline_color(status: &str) -> Color {
    match status {
        "success" => Color::Green,
        "failed" => Color::Red,
        "running" | "pending" | "created" | "preparing" | "waiting_for_resource" | "scheduled" => {
            Color::Yellow
        }
        _ => Color::Magenta,
    }
}

// ---- HTTP transport --------------------------------------------------------

fn api_url(host: &str, path: &str) -> String {
    format!("https://{host}/api/v4{path}")
}

/// One authenticated GET via `agent`. The token is sent as a request header
/// from within this process, so — unlike a `curl` subprocess — it never touches
/// argv or any other process. Returns the response body, or None on a `>= 400`
/// status, timeout, or transport error. The agent disables redirects (so the
/// token is never re-sent to another host) and bounds connect + TLS + read by
/// `timeout`; only a pathological DNS lookup is bounded by the OS resolver.
fn http_get(
    agent: &ureq::Agent,
    url: &str,
    token: &str,
    timeout: std::time::Duration,
) -> Option<String> {
    agent
        .get(url)
        .set("PRIVATE-TOKEN", token)
        .timeout(timeout)
        .call()
        .ok()?
        .into_string()
        .ok()
}

// ---- on-disk cache ---------------------------------------------------------

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Stable FNV-1a hash of the identity (host/project/branch) for the cache file.
fn cache_key(host: &str, project: &str, branch: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in format!("{host}\0{project}\0{branch}").bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// A private, per-user cache directory (`$XDG_CACHE_HOME/constatus` or
/// `~/.cache/constatus`, falling back to the temp dir only if neither is set).
/// Created `0700` so it is never the shared, world-writable temp root — which
/// would let another local user pre-plant a symlink at our predictable
/// filename and have us follow it (CWE-59/377).
fn cache_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("constatus");
    ensure_private_dir(&dir)?;
    Some(dir)
}

#[cfg(unix)]
fn ensure_private_dir(dir: &std::path::Path) -> Option<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .ok()
}

#[cfg(not(unix))]
fn ensure_private_dir(dir: &std::path::Path) -> Option<()> {
    std::fs::create_dir_all(dir).ok()
}

fn cache_path(key: &str) -> Option<std::path::PathBuf> {
    Some(cache_dir()?.join(format!("gitlab-{key}.json")))
}

fn read_cache(key: &str, ttl: u64) -> Option<Status> {
    let raw = std::fs::read_to_string(cache_path(key)?).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let ts = v.get("ts")?.as_u64()?;
    if now_secs().saturating_sub(ts) > ttl {
        return None; // stale
    }
    let pipeline = v
        .get("pipeline")
        .and_then(|p| p.as_str())
        .map(str::to_string);
    let mr_count = v.get("mr").and_then(serde_json::Value::as_u64);
    Some(Status { pipeline, mr_count })
}

fn write_cache(key: &str, status: &Status) {
    let Some(path) = cache_path(key) else {
        return;
    };
    let body = serde_json::json!({
        "ts": now_secs(),
        "pipeline": status.pipeline,
        "mr": status.mr_count,
    })
    .to_string();

    // Write atomically and without following symlinks: `create_new` (O_EXCL)
    // refuses to open through a pre-planted symlink, and the rename swaps the
    // file into place in one step. Best-effort throughout — a failed cache
    // write just means we re-fetch next time.
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
    {
        Ok(mut f) => {
            if f.write_all(body.as_bytes()).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(_) => {
            // A stale temp from a crashed run (recycled pid) — clear and skip.
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_project_path_and_branch() {
        assert_eq!(percent_encode("group/sub/project"), "group%2Fsub%2Fproject");
        assert_eq!(percent_encode("feature/foo bar"), "feature%2Ffoo%20bar");
        assert_eq!(percent_encode("plain-1.2_x~"), "plain-1.2_x~");
    }

    #[test]
    fn pipeline_path_is_well_formed() {
        let p = pipeline_path("g/p", "main");
        assert!(p.starts_with("/projects/g%2Fp/pipelines?"));
        assert!(p.contains("ref=main"));
        assert!(p.contains("per_page=1"));
    }

    #[test]
    fn mr_path_filters_branch() {
        let p = mr_path("g/p", "feat/x");
        assert!(p.contains("state=opened"));
        assert!(p.contains("source_branch=feat%2Fx"));
    }

    #[test]
    fn parses_latest_pipeline_status() {
        let json = r#"[{"id":2,"status":"success"},{"id":1,"status":"failed"}]"#;
        assert_eq!(parse_pipeline_status(json).as_deref(), Some("success"));
    }

    #[test]
    fn empty_pipeline_array_is_none() {
        assert_eq!(parse_pipeline_status("[]"), None);
        assert_eq!(parse_pipeline_status("garbage"), None);
    }

    #[test]
    fn counts_open_mrs() {
        assert_eq!(parse_mr_count(r#"[{"iid":1},{"iid":2}]"#), 2);
        assert_eq!(parse_mr_count("[]"), 0);
        assert_eq!(parse_mr_count("not json"), 0);
    }

    #[test]
    fn status_visuals_cover_states() {
        assert_eq!(pipeline_label("success"), "passed");
        assert_eq!(pipeline_label("failed"), "failed");
        assert_eq!(pipeline_label("pending"), "pending");
        assert_eq!(pipeline_label("weird_new_state"), "pipeline");
        assert_eq!(pipeline_icon("success"), icons::PIPE_OK);
        assert_eq!(pipeline_icon("failed"), icons::PIPE_FAIL);
    }

    #[test]
    fn only_trusted_hosts_receive_the_token() {
        // gitlab.com and its subdomains are trusted out of the box.
        assert!(is_canonical_gitlab_host("gitlab.com"));
        assert!(is_canonical_gitlab_host("GitLab.com"));
        assert!(is_canonical_gitlab_host("api.gitlab.com"));
        // Look-alikes must NOT be trusted (would leak the token).
        assert!(!is_canonical_gitlab_host("evil-gitlab.com"));
        assert!(!is_canonical_gitlab_host("gitlab.attacker.com"));
        assert!(!is_canonical_gitlab_host("gitlab.com.evil.com"));
        // Self-hosted instances must opt in explicitly via the allowlist.
        assert!(!is_canonical_gitlab_host("gitlab.example.com"));
        assert!(host_matches_allowlist(
            "gitlab.example.com",
            "gitlab.example.com, git.internal"
        ));
        assert!(host_matches_allowlist(
            "git.internal",
            "gitlab.example.com git.internal"
        ));
        assert!(!host_matches_allowlist("evil.com", "gitlab.example.com"));
        assert!(!host_matches_allowlist("anything", ""));
    }

    #[test]
    fn rejects_injection_in_host_and_token() {
        assert!(host_is_safe("gitlab.com"));
        assert!(host_is_safe("gitlab.example.com"));
        assert!(!host_is_safe("gitlab.com\"\nurl = \"http://evil")); // config breakout
        assert!(!host_is_safe("host with space"));
        assert!(!host_is_safe(""));

        assert!(token_is_safe("glpat-xxxxxxxxxxxxxxxxxxxx"));
        assert!(!token_is_safe("tok\"en")); // quote breaks header line
        assert!(!token_is_safe("tok\nen")); // newline injects config
        assert!(!token_is_safe(""));
    }

    #[test]
    fn cache_key_is_stable_and_distinct() {
        let a = cache_key("gitlab.com", "g/p", "main");
        let b = cache_key("gitlab.com", "g/p", "main");
        let c = cache_key("gitlab.com", "g/p", "dev");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }
}
