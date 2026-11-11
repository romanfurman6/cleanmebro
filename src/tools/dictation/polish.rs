//! Fix a dictation's misheard words with an AI command-line tool the user
//! already has signed in: Claude Code (`claude -p`) or Codex (`codex exec`).

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Polish {
    #[default]
    Off,
    Claude,
    Codex,
}

impl Polish {
    pub const ALL: [Polish; 3] = [Polish::Off, Polish::Claude, Polish::Codex];

    pub fn label(self) -> &'static str {
        match self {
            Polish::Off => "Off",
            Polish::Claude => "Claude",
            Polish::Codex => "Codex",
        }
    }
}

/// Longest wait for an answer; past it the words go in as heard.
const TIMEOUT: Duration = Duration::from_secs(25);

fn prompt(text: &str) -> String {
    format!(
        "This text was dictated and transcribed by speech recognition. Fix misheard words, \
         punctuation and capitals. Keep its language, words and meaning: don't translate, \
         rephrase, answer or add anything. Reply with the fixed text only.\n\n{text}"
    )
}

/// The fixed text, or why not. Runs the tool through a login shell, since
/// an app opened from the Finder doesn't get the user's PATH.
pub fn run(tool: Polish, text: &str) -> Result<String, String> {
    let out_file = std::env::temp_dir().join(format!("cmdmeow-polish-{}.txt", std::process::id()));
    let script = match tool {
        Polish::Off => return Ok(text.to_string()),
        // the fastest model, and nothing to start but the model: no MCP
        // servers, tools, project settings or saved session (~4 s, not 9)
        Polish::Claude => "exec claude -p --model haiku --strict-mcp-config --tools= --no-session-persistence --setting-sources user".to_string(),
        Polish::Codex => format!("exec codex exec --skip-git-repo-check --output-last-message '{}' -", out_file.display()),
    };
    let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-lc", &script])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{} didn't start: {e}", tool.label()))?;
    if let Some(mut i) = child.stdin.take() {
        let _ = i.write_all(prompt(text).as_bytes());
    }
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if t0.elapsed() > TIMEOUT => {
                let _ = child.kill();
                return Err(format!("{} took too long", tool.label()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let answer = match tool {
        Polish::Codex => {
            let a = std::fs::read_to_string(&out_file).unwrap_or_default();
            let _ = std::fs::remove_file(&out_file);
            a
        }
        _ => String::from_utf8_lossy(&out.stdout).into_owned(),
    };
    eprintln!("[dictation] {} polished in {} ms", tool.label(), t0.elapsed().as_millis());
    sane(text, answer.trim()).ok_or_else(|| format!("{} gave no usable text", tool.label()))
}

/// The answer, unless it looks like anything but the same text fixed: empty,
/// or far longer or shorter.
fn sane(text: &str, answer: &str) -> Option<String> {
    let (a, b) = (text.chars().count() as f32, answer.chars().count() as f32);
    (!answer.is_empty() && b <= a * 1.6 + 20.0 && b >= a * 0.5).then(|| answer.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_that_are_not_the_text_are_dropped() {
        assert_eq!(sane("привіт як справи", "Привіт, як справи?").as_deref(), Some("Привіт, як справи?"));
        assert_eq!(sane("привіт як справи", ""), None);
        assert_eq!(sane("привіт", &"long answer ".repeat(10)), None);
        assert_eq!(run(Polish::Off, "same").as_deref(), Ok("same"));
    }
}
