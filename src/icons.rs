/// Nerd Font icons for status line segments.

pub const MODEL: &str = "\u{f01fc}"; // 󰇼 nf-md-brain
pub const FOLDER: &str = "\u{f07c}";  //  nf-fa-folder_open
/// Returns a moon phase icon based on context window used percentage.
/// ● (full moon) when fresh, degrades to ○ (empty moon) as context fills up.
pub fn context_icon(used_pct: f64) -> &'static str {
    if used_pct < 25.0 {
        "●" // U+25CF full moon – fresh context
    } else if used_pct < 50.0 {
        "◑" // U+25D1 half moon
    } else if used_pct < 75.0 {
        "◔" // U+25D4 quarter moon
    } else {
        "○" // U+25CB empty moon – context nearly gone
    }
}
pub const CACHE: &str = "\u{f1c0}";   //  nf-fa-database
pub const INPUT: &str = "\u{f063}";   //  nf-fa-arrow_down
pub const OUTPUT: &str = "\u{f062}";  //  nf-fa-arrow_up
pub const COST: &str = "\u{f155}";    //  nf-fa-dollar
pub const CLOCK: &str = "\u{f017}";   //  nf-fa-clock_o
pub const BRANCH: &str = "\u{e725}";  //  nf-dev-git_branch
pub const BAR_FULL: char = '█';
pub const BAR_EMPTY: char = '░';

// Forge (remote host) icons.
pub const GITLAB: &str = "\u{f296}";    //  nf-fa-gitlab
pub const GITHUB: &str = "\u{f09b}";    //  nf-fa-github
pub const BITBUCKET: &str = "\u{f171}"; //  nf-fa-bitbucket
pub const GIT: &str = "\u{e702}";       //  nf-dev-git (generic)

// GitLab merge-request and pipeline icons.
pub const MR: &str = "\u{f407}";        //  nf-oct-git_pull_request
pub const PIPE_OK: &str = "\u{f058}";   //  nf-fa-check_circle (passed)
pub const PIPE_FAIL: &str = "\u{f057}"; //  nf-fa-times_circle (failed)
pub const PIPE_RUN: &str = "\u{f021}";  //  nf-fa-refresh (running/pending)
pub const PIPE_WARN: &str = "\u{f071}"; //  nf-fa-warning (canceled/manual/skipped)
