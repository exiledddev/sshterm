# ACLI — Amplified Command Line Interface

A terminal for Linux that splits. Every pane is a real pseudo-terminal running
your shell, arranged on one sheet of translucent glass. Written in Rust, drawn
with [egui](https://github.com/emilk/egui) on a transparent, borderless window.

There is no splash screen, no session manager and nothing to load — it opens
straight into a live shell.

> ACLI is the SSH-free sibling of **SSCL** (Secure Shell Command Line), which
> lives on this repository's `main` branch. Same terminal, same design; this
> one splits instead of remembering servers.

```
┌────────────────────────────────────────────────────────────┐
│ ⬛ AMPLIFIED COMMAND LINE INTERFACE   bash 61×33 · 2 panes  │
│ [▤ Split Right] [▤ Split Down] [▣ Close Pane]  │  [≡]      │
│ ┌──────────────────────┐ ┌───────────────────────────────┐ │
│ │ ╭─ ● you@host ─ ~    │ │ ╭─ ● you@host ─ ~/src ─ main* │ │
│ │ ╰─❯ cargo test       │ │ ╰─❯ git log                   │ │
│ └──────────────────────┘ └───────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

## Installing

Build dependencies (Debian, Ubuntu, Pop!\_OS):

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
     libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev
```

Then, with a [Rust toolchain](https://rustup.rs) installed:

```sh
git clone -b acli https://github.com/exiledddev/sshterm.git acli
cd acli
./install.sh          # builds in release mode and installs under ~/.local
./install.sh --uninstall
```

Or just `cargo run --release`.

## Splitting

| Button | Shortcut | |
| --- | --- | --- |
| **Split Right** | `Ctrl+Shift+R` | another terminal beside this one |
| **Split Down** | `Ctrl+Shift+D` | another terminal below this one |
| **Close Pane** | `Ctrl+Shift+W` | closes the focused pane |
| **≡** | `Ctrl+Shift+,` | prompt colours |

Click a pane to focus it, or move with `Ctrl+Shift+←↑↓→`. The focused pane
wears a brighter ring. Drag the gap between two panes to change how the space
is divided; neither side can be dragged away entirely.

Panes are a tree, so splits nest: split right, then split the right-hand pane
downwards, and the left one keeps the full height. Closing a pane gives its
room back to its sibling.

Exiting the shell in a pane closes that pane, exactly as in any other terminal.
Exiting the last one closes the window.

`Ctrl+±` and `Ctrl+0` change the terminal font size.

## The terminal

A real PTY per pane, running your login shell.

* xterm-compatible: 256 colours and true colour, alternate screen, bracketed
  paste, mouse reporting, 10 000 lines of scrollback
* Select with the mouse to copy; middle-click pastes the selection
* `Ctrl+Shift+C` / `Ctrl+Shift+V` for the clipboard — plain `Ctrl+C`, `Ctrl+X`
  and `Ctrl+V` stay as the control codes a terminal needs
* `Shift+PageUp` / `Shift+PageDown` and the wheel scroll the scrollback

### Autosuggestions

As you type, the rest of the most recent matching command appears in grey after
the cursor, with a small list of the alternatives:

* **Right arrow**, `Ctrl+F` or **End** accepts the suggestion
* **Alt+↑ / Alt+↓** cycles through the list, **Esc** dismisses it
* **Tab** is left alone — it is the shell's own completion

Suggestions come from commands you have run in ACLI (kept in
`~/.local/share/acli/history`, seeded once from `~/.bash_history` or
`~/.zsh_history`) and from the executables on your `$PATH`. Everything stays on
your machine.

Because the shell does its own line editing, ACLI mirrors the keystrokes it
forwards to know what the current line says. Anything it cannot model
faithfully — tab completion, `Ctrl+R`, arrow-key history — makes it stop
guessing until the next `Enter`, so a suggestion is only ever shown when it is
known to be right.

### The prompt, and its colours

Panes get a generated two-line prompt: a status dot, user, host, path, git
branch, and the exit code when a command fails.

```
╭─ ● you@host ─ ~/projects/acli ─ main*
╰─❯
```

**≡ in the ribbon** (or `Ctrl+Shift+,`) opens Settings, where each part of it
has its own colour picker and a live preview. Changes are saved as you make
them, to `~/.config/acli/config.json`:

```json
{
  "prompt": {
    "frame":  [95, 95, 135],
    "user":   [175, 135, 255],
    "host":   [95, 255, 215],
    "path":   [135, 175, 215],
    "git":    [255, 175, 95],
    "ok":     [95, 255, 95],
    "err":    [255, 95, 95],
    "arrow":  [175, 135, 255]
  }
}
```

Colours are written as true-colour escapes, so the picker maps exactly onto
what the shell emits. New panes pick them up immediately; **Apply to open
panes** re-sources the prompt in every pane that is sitting at an empty prompt
and leaves busy ones alone.

ACLI never modifies your dotfiles. It writes `~/.local/share/acli/acli.bashrc`
(or `acli.zshrc`, `acli.fish`), which sources your own configuration first and
then sets the prompt, and launches the shell against that. bash, zsh and fish
are themed; any other shell starts normally with its own prompt.

## Design notes

The window is one flat, low-opacity sheet of glass — no gradients, no colour
tint, just a neutral wash with a hairline edge, in the manner of JetBrains'
newer UI. The ribbon sits directly on it, and the glass continues past the
panes on every side.

Each terminal is an opaque island: rounded, outlined, inset from the chrome and
from its neighbours. They are the only surfaces with a background of their own.

### Backdrop blur

On **KDE Plasma (X11)** ACLI asks KWin to blur what is behind it, by setting
`_KDE_NET_WM_BLUR_BEHIND_REGION` on its own window. The region follows the
window's rounded corners and is updated on resize, so the blur does not spill
out as a hard square behind them. Nothing to configure, as long as KWin's Blur
desktop effect is on.

The glass then adapts to what it is sitting on:

| | |
| --- | --- |
| **Blur active** | thin glass, so the blurred desktop reads through it |
| **No blur** | dense glass, so a busy wallpaper cannot interfere with the text |

`ACLI_BLUR=on|off|auto` overrides the detection, and `ACLI_OPACITY=1..100` sets
the opacity outright — lower is more see-through.

On a **Plasma Wayland** session the equivalent is a Wayland protocol that winit
does not expose, so ACLI falls back to the dense glass. Running under XWayland
(`WINIT_UNIX_BACKEND=x11`) or installing
[Force Blur](https://github.com/taj-ny/kwin-effects-forceblur) gets the blur
back; pair either with `ACLI_BLUR=on`.

### Chrome and assets

The window is borderless, so ACLI draws its own title bar: drag it to move the
window, double-click to maximise, and the window edges are resize handles.

Icons are drawn with vector primitives at runtime and the fonts are the ones
egui bundles, so the binary needs no image or font assets. The application icon
is monochrome — a grey tile carrying a rising level meter — and a unit test
asserts every visible pixel of it stays grey.

## Development

```sh
cargo test                 # unit tests
cargo clippy --all-targets
```

| Path | Contents |
| --- | --- |
| `src/app.rs` | panes, splits, ribbon actions |
| `src/layout.rs` | the pane tree: splitting, closing, sizing, navigation |
| `src/prompt.rs` | prompt colours, the generated rc files, the config file |
| `src/term/` | PTY sessions, the terminal widget, autosuggestions |
| `src/ui/` | window chrome, settings, shared widgets |
| `src/theme.rs`, `src/icons.rs` | palette, glass surfaces, vector icons |
| `src/platform/kwin_blur.rs` | the KWin backdrop-blur region |

## Licence

Apache-2.0. See [LICENSE](LICENSE).
