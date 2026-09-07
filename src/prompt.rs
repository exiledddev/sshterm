//! The generated shell prompt, and the colours it uses.
//!
//! ACLI does not modify your dotfiles. It writes an rc file of its own that
//! sources yours first and then sets `PS1`, and launches the shell against
//! that. The colours in it are yours to change from Settings; they are stored
//! in `~/.config/acli/config.json` and written as true-colour escapes, so the
//! picker maps straight onto what the shell emits.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// An sRGB colour, stored the way egui's colour picker wants it.
pub type Rgb = [u8; 3];

/// Colours of each piece of the generated prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptTheme {
    /// The `╭─` and `╰─` box drawing.
    pub frame: Rgb,
    /// User name.
    pub user: Rgb,
    /// Host name.
    pub host: Rgb,
    /// Working directory.
    pub path: Rgb,
    /// Git branch, when there is one.
    pub git: Rgb,
    /// The status dot after a command that succeeded.
    pub ok: Rgb,
    /// The status dot, arrow and exit code after one that failed.
    pub err: Rgb,
    /// The `❯` on the second line.
    pub arrow: Rgb,
}

impl Default for PromptTheme {
    /// The stock palette. These are the exact xterm-256 colours the prompt
    /// used before it became configurable, so the default look is unchanged.
    fn default() -> Self {
        Self {
            frame: [0x5F, 0x5F, 0x87],
            user: [0xAF, 0x87, 0xFF],
            host: [0x5F, 0xFF, 0xD7],
            path: [0x87, 0xAF, 0xD7],
            git: [0xFF, 0xAF, 0x5F],
            ok: [0x5F, 0xFF, 0x5F],
            err: [0xFF, 0x5F, 0x5F],
            arrow: [0xAF, 0x87, 0xFF],
        }
    }
}

/// Reaches one colour inside a theme, so the settings UI can edit it in place.
pub type Field = fn(&mut PromptTheme) -> &mut Rgb;

/// Every colour, in the order the settings window shows them.
pub const FIELDS: &[(&str, Field)] = &[
    ("Frame", |t| &mut t.frame),
    ("User", |t| &mut t.user),
    ("Host", |t| &mut t.host),
    ("Path", |t| &mut t.path),
    ("Git branch", |t| &mut t.git),
    ("Success", |t| &mut t.ok),
    ("Failure", |t| &mut t.err),
    ("Prompt arrow", |t| &mut t.arrow),
];

impl PromptTheme {
    /// Reads the stored theme, falling back to the default when the file is
    /// missing or unreadable. A malformed file is reported so the settings
    /// window can say so rather than silently resetting.
    pub fn load() -> (Self, Option<String>) {
        let path = crate::paths::config_file();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str::<Config>(&text) {
            Ok(config) => (config.prompt, None),
            Err(e) => (
                Self::default(),
                Some(format!("{} is not valid JSON: {e}", path.display())),
            ),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = crate::paths::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        let text = serde_json::to_string_pretty(&Config { prompt: *self })
            .map_err(|e| format!("Cannot serialise settings: {e}"))?;
        let path = crate::paths::config_file();
        std::fs::write(&path, text).map_err(|e| format!("Cannot write {}: {e}", path.display()))
    }

    /// Writes the rc file for `shell` and returns its path.
    pub fn write_rc(&self, shell: Shell) -> std::io::Result<PathBuf> {
        let dir = crate::paths::data_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(shell.rc_name());
        std::fs::write(&path, self.render(shell))?;
        Ok(path)
    }

    /// The rc file's contents for `shell`.
    pub fn render(&self, shell: Shell) -> String {
        let template = match shell {
            Shell::Bash => BASH,
            Shell::Zsh => ZSH,
            Shell::Fish => FISH,
        };
        let mut out = template.to_string();
        for (name, get) in FIELDS {
            let mut copy = *self;
            let rgb = *get(&mut copy);
            out = out.replace(&placeholder(name), &shell.colour(rgb));
        }
        out.replace("@BOLD@", shell.bold())
            .replace("@RESET@", shell.reset())
    }
}

/// The on-disk shape, so more settings can be added later without breaking
/// existing files.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    prompt: PromptTheme,
}

fn placeholder(name: &str) -> String {
    format!("@{}@", name.to_uppercase().replace(' ', "_"))
}

/// The shells ACLI generates a prompt for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// Recognises a shell from the path in `$SHELL`.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.file_name()?.to_str()? {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }

    pub fn rc_name(self) -> &'static str {
        match self {
            Self::Bash => "acli.bashrc",
            Self::Zsh => "acli.zshrc",
            Self::Fish => "acli.fish",
        }
    }

    /// How this shell wants a true-colour foreground written, escaped so it
    /// does not count towards the prompt's visible width.
    fn colour(self, [r, g, b]: Rgb) -> String {
        match self {
            Self::Bash => format!("\\[\\e[38;2;{r};{g};{b}m\\]"),
            Self::Zsh => format!("%{{\x1b[38;2;{r};{g};{b}m%}}"),
            Self::Fish => format!("{r:02x}{g:02x}{b:02x}"),
        }
    }

    fn bold(self) -> &'static str {
        match self {
            Self::Bash => "\\[\\e[1m\\]",
            Self::Zsh => "%{\x1b[1m%}",
            Self::Fish => "",
        }
    }

    fn reset(self) -> &'static str {
        match self {
            Self::Bash => "\\[\\e[0m\\]",
            Self::Zsh => "%{\x1b[0m%}",
            Self::Fish => "normal",
        }
    }
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

const BASH: &str = r##"# Generated by ACLI (Amplified Command Line Interface). Safe to delete.
# Rewritten whenever the prompt colours change in Settings.
[ -f /etc/bash.bashrc ] && . /etc/bash.bashrc
[ -f "$HOME/.bashrc" ] && . "$HOME/.bashrc"

# The whole prompt is rebuilt each time, so every escape sits inside PS1 and
# can be wrapped in \[ \] — bash then counts the visible width correctly.
__acli_ps1() {
  local ec=$?
  local branch dirty="" git_segment=""
  branch=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null)
  if [ -n "$branch" ]; then
    [ -n "$(git status --porcelain 2>/dev/null)" ] && dirty="*"
    git_segment=" @FRAME@─@RESET@ @GIT_BRANCH@${branch}${dirty}@RESET@"
  fi

  local dot arrow code=""
  if [ $ec -eq 0 ]; then
    dot="@SUCCESS@●@RESET@"
    arrow="@BOLD@@PROMPT_ARROW@❯@RESET@"
  else
    dot="@FAILURE@●@RESET@"
    arrow="@BOLD@@FAILURE@❯@RESET@"
    code="  @FAILURE@[$ec]@RESET@"
  fi

  PS1="@FRAME@╭─@RESET@ ${dot} @USER@\u@FRAME@@@HOST@\h@RESET@ @FRAME@─@RESET@ @PATH@\w@RESET@${git_segment}${code}\n@FRAME@╰─@RESET@${arrow} "
  PS2="@FRAME@  ·@RESET@ "
}
PROMPT_COMMAND=__acli_ps1
"##;

const ZSH: &str = r##"# Generated by ACLI (Amplified Command Line Interface). Safe to delete.
# Rewritten whenever the prompt colours change in Settings.
[ -f /etc/zsh/zshrc ] && source /etc/zsh/zshrc
[ -f "$HOME/.zshrc" ] && source "$HOME/.zshrc"

# As in bash: rebuild PROMPT each time so the %{ %} width markers are part of
# PROMPT itself. Escapes inside a $(...) result would not be expanded.
__acli_precmd() {
  local branch dirty="" git_segment=""
  branch=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null)
  if [ -n "$branch" ]; then
    [ -n "$(git status --porcelain 2>/dev/null)" ] && dirty="*"
    git_segment=" @FRAME@─@RESET@ @GIT_BRANCH@${branch}${dirty}@RESET@"
  fi

  PROMPT="@FRAME@╭─@RESET@ %(?.@SUCCESS@●@RESET@.@FAILURE@●@RESET@) @USER@%n@FRAME@@@HOST@%m@RESET@ @FRAME@─@RESET@ @PATH@%~@RESET@${git_segment}%(?..  @FAILURE@[%?]@RESET@)
@FRAME@╰─@RESET@%(?.@BOLD@@PROMPT_ARROW@❯@RESET@.@BOLD@@FAILURE@❯@RESET@) "
  PROMPT2="@FRAME@  ·@RESET@ "
}
autoload -Uz add-zsh-hook 2>/dev/null && add-zsh-hook precmd __acli_precmd || precmd_functions+=(__acli_precmd)
"##;

const FISH: &str = r##"# Generated by ACLI (Amplified Command Line Interface). Safe to delete.
# Rewritten whenever the prompt colours change in Settings.
function fish_prompt
    set -l ec $status
    set_color @FRAME@; echo -n '╭─ '
    if test $ec -eq 0
        set_color @SUCCESS@
    else
        set_color @FAILURE@
    end
    echo -n '● '
    set_color @USER@; echo -n (whoami)
    set_color @FRAME@; echo -n '@'
    set_color @HOST@; echo -n (prompt_hostname)
    set_color @FRAME@; echo -n ' ─ '
    set_color @PATH@; echo -n (prompt_pwd)
    set -l branch (git symbolic-ref --short HEAD 2>/dev/null; or git rev-parse --short HEAD 2>/dev/null)
    if test -n "$branch"
        set_color @FRAME@; echo -n ' ─ '
        set_color @GIT_BRANCH@; echo -n $branch
    end
    if test $ec -ne 0
        set_color @FAILURE@; echo -n "  [$ec]"
    end
    echo
    set_color @FRAME@; echo -n '╰─'
    if test $ec -eq 0
        set_color -o @PROMPT_ARROW@
    else
        set_color -o @FAILURE@
    end
    echo -n '❯ '
    set_color @RESET@
end
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("acli-tests-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            crate::paths::override_dirs(dir);
        });
    }

    #[test]
    fn every_placeholder_is_substituted() {
        let theme = PromptTheme::default();
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let rendered = theme.render(shell);
            // A literal @ survives between user and host; only the
            // @PLACEHOLDER@ forms must be gone.
            for (name, _) in FIELDS {
                assert!(
                    !rendered.contains(&placeholder(name)),
                    "{shell:?} kept {name}"
                );
            }
            assert!(!rendered.contains("@BOLD@") && !rendered.contains("@RESET@"));
        }
    }

    #[test]
    fn colours_are_emitted_as_true_colour() {
        let mut theme = PromptTheme {
            user: [1, 2, 3],
            ..Default::default()
        };

        let bash = theme.render(Shell::Bash);
        assert!(bash.contains("\\[\\e[38;2;1;2;3m\\]"), "{bash}");

        let zsh = theme.render(Shell::Zsh);
        assert!(zsh.contains("\x1b[38;2;1;2;3m"));

        // fish takes a bare hex triple.
        theme.user = [0xAB, 0xCD, 0xEF];
        assert!(theme.render(Shell::Fish).contains("set_color abcdef"));
    }

    #[test]
    fn recognises_the_shells_it_can_theme() {
        assert_eq!(Shell::from_path(Path::new("/bin/bash")), Some(Shell::Bash));
        assert_eq!(Shell::from_path(Path::new("/usr/bin/zsh")), Some(Shell::Zsh));
        assert_eq!(Shell::from_path(Path::new("/usr/bin/fish")), Some(Shell::Fish));
        assert_eq!(Shell::from_path(Path::new("/bin/dash")), None);
        assert_eq!(Shell::from_path(Path::new("/")), None);
    }

    #[test]
    fn settings_survive_a_round_trip() {
        scratch();
        let theme = PromptTheme {
            path: [10, 20, 30],
            arrow: [40, 50, 60],
            ..Default::default()
        };
        theme.save().unwrap();

        let (loaded, error) = PromptTheme::load();
        assert_eq!(loaded, theme);
        assert!(error.is_none());
    }

    #[test]
    fn a_broken_config_reports_itself_instead_of_resetting_silently() {
        scratch();
        std::fs::create_dir_all(crate::paths::config_dir()).unwrap();
        std::fs::write(crate::paths::config_file(), "{ not json").unwrap();

        let (loaded, error) = PromptTheme::load();
        assert_eq!(loaded, PromptTheme::default());
        assert!(error.is_some_and(|e| e.contains("not valid JSON")));

        // Leave the scratch config valid for the round-trip test.
        PromptTheme::default().save().unwrap();
    }

    #[test]
    fn writes_an_rc_file_per_shell() {
        scratch();
        let theme = PromptTheme::default();
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let path = theme.write_rc(shell).unwrap();
            assert!(path.is_file());
            assert_eq!(path.file_name().unwrap(), shell.rc_name());
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(body.contains("Generated by ACLI"));
            assert!(body.contains('❯'));
        }
    }
}
