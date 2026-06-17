//! Detect the hosting forge (GitLab, GitHub, Bitbucket, …) from a repo's
//! git remote, fully offline. The remote URL also yields the host and
//! `owner/project` path used by the optional GitLab API integration.

use crate::color::Color;
use crate::icons;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Forge {
    GitLab,
    GitHub,
    Bitbucket,
    Other,
}

impl Forge {
    fn from_host(host: &str) -> Self {
        let h = host.to_lowercase();
        // Match on host substrings so self-hosted instances
        // (e.g. gitlab.example.com) are classified correctly.
        if h.contains("gitlab") {
            Self::GitLab
        } else if h.contains("github") {
            Self::GitHub
        } else if h.contains("bitbucket") {
            Self::Bitbucket
        } else {
            Self::Other
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::GitLab => icons::GITLAB,
            Self::GitHub => icons::GITHUB,
            Self::Bitbucket => icons::BITBUCKET,
            Self::Other => icons::GIT,
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::GitLab => Color::Orange,
            Self::GitHub => Color::White,
            Self::Bitbucket => Color::Blue,
            Self::Other => Color::Yellow,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GitLab => "gitlab",
            Self::GitHub => "github",
            Self::Bitbucket => "bitbucket",
            Self::Other => "git",
        }
    }
}

/// Parsed remote: the classified forge, the bare host, and the
/// `owner/project` path (no trailing `.git`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub forge: Forge,
    pub host: String,
    pub project: String,
}

/// Run git in `dir` to read the origin remote URL (falling back to the
/// first configured remote) and parse it into a [`Remote`].
pub fn detect(dir: &str) -> Option<Remote> {
    let url = remote_url(dir)?;
    parse_remote(&url)
}

fn remote_url(dir: &str) -> Option<String> {
    // Prefer `origin`; if absent, fall back to whatever the first remote is.
    if let Some(u) = git_remote_url(dir, Some("origin")) {
        return Some(u);
    }
    let first = run_git(dir, &["remote"])?;
    let name = first.lines().next()?.trim();
    if name.is_empty() {
        return None;
    }
    git_remote_url(dir, Some(name))
}

fn git_remote_url(dir: &str, name: Option<&str>) -> Option<String> {
    let mut args = vec!["remote", "get-url"];
    if let Some(n) = name {
        args.push(n);
    }
    let out = run_git(dir, &args)?;
    let url = out.trim().to_string();
    if url.is_empty() { None } else { Some(url) }
}

fn run_git(dir: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Parse a git remote URL into host + `owner/project`. Handles the common
/// forms:
///   - `https://host/owner/project.git`
///   - `https://user@host/owner/project`
///   - `git@host:owner/project.git`            (scp-like)
///   - `ssh://git@host:2222/owner/project.git`
///   - subgroups: `host/group/sub/project`
pub fn parse_remote(url: &str) -> Option<Remote> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let (host, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("ssh://"))
        .or_else(|| url.strip_prefix("git://"))
    {
        // scheme://[user@]host[:port]/path
        let rest = rest.splitn(2, '/').collect::<Vec<_>>();
        let authority = rest.first()?;
        let path = rest.get(1).copied().unwrap_or("");
        let host = strip_userinfo_and_port(authority);
        (host.to_string(), path.to_string())
    } else if let Some(rest) = url.strip_prefix("git@").or_else(|| {
        // generic scp-like `user@host:path` — only if there's no scheme
        if url.contains("://") { None } else { Some(url) }
    }) {
        // [user@]host:path  (scp-like, ':' separates host from path)
        let (authority, path) = rest.split_once(':')?;
        let host = strip_userinfo_and_port(authority);
        (host.to_string(), path.to_string())
    } else {
        return None;
    };

    let project = normalize_project(&path);
    if host.is_empty() || project.is_empty() {
        return None;
    }

    Some(Remote {
        forge: Forge::from_host(&host),
        host,
        project,
    })
}

/// Drop any `user@` prefix and `:port` suffix from an authority component.
fn strip_userinfo_and_port(authority: &str) -> &str {
    let after_user = authority.rsplit('@').next().unwrap_or(authority);
    // For `host:port`, strip the port. Hosts never contain ':' otherwise here.
    after_user.split(':').next().unwrap_or(after_user)
}

/// Trim a leading slash and trailing `.git` / slash from a repo path.
fn normalize_project(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(url: &str) -> Remote {
        parse_remote(url).expect("should parse")
    }

    #[test]
    fn https_gitlab() {
        let g = r("https://gitlab.com/group/project.git");
        assert_eq!(g.forge, Forge::GitLab);
        assert_eq!(g.host, "gitlab.com");
        assert_eq!(g.project, "group/project");
    }

    #[test]
    fn scp_like_github() {
        let g = r("git@github.com:owner/repo.git");
        assert_eq!(g.forge, Forge::GitHub);
        assert_eq!(g.host, "github.com");
        assert_eq!(g.project, "owner/repo");
    }

    #[test]
    fn ssh_with_port() {
        let g = r("ssh://git@gitlab.example.com:2222/team/sub/app.git");
        assert_eq!(g.forge, Forge::GitLab); // self-hosted by host substring
        assert_eq!(g.host, "gitlab.example.com");
        assert_eq!(g.project, "team/sub/app");
    }

    #[test]
    fn https_with_userinfo_no_dotgit() {
        let g = r("https://oauth2:token@gitlab.com/group/project");
        assert_eq!(g.forge, Forge::GitLab);
        assert_eq!(g.host, "gitlab.com");
        assert_eq!(g.project, "group/project");
    }

    #[test]
    fn subgroups_preserved() {
        let g = r("git@gitlab.com:a/b/c/d.git");
        assert_eq!(g.project, "a/b/c/d");
    }

    #[test]
    fn bitbucket_and_other() {
        assert_eq!(r("https://bitbucket.org/o/p.git").forge, Forge::Bitbucket);
        assert_eq!(r("https://git.sr.ht/~user/repo").forge, Forge::Other);
    }

    #[test]
    fn forge_visuals_are_distinct() {
        // Each forge maps to its own label/icon — guards against copy-paste slips.
        assert_eq!(Forge::GitLab.label(), "gitlab");
        assert_eq!(Forge::GitHub.label(), "github");
        assert_ne!(Forge::GitLab.icon(), Forge::GitHub.icon());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_remote("").is_none());
        assert!(parse_remote("not-a-url").is_none());
        assert!(parse_remote("https://gitlab.com/").is_none()); // no project
    }
}
