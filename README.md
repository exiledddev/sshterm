# SSCL — Secure Shell Command Line

A terminal application for Linux that opens and remembers SSH connections.
Written in Rust, drawn with [egui](https://github.com/emilk/egui) on a
transparent, borderless window.

The centre of the window is a real terminal running on a pseudo-terminal, so it
behaves like Konsole or GNOME Terminal — with fish-style autosuggestions on top.
The left sidebar browses your saved connections; the right sidebar fingerprints
whichever SSH server the current session is talking to.

```
┌────────────────────────────────────────────────────────────────────┐
│  ⬛ SECURE SHELL COMMAND LINE      ● Test box · root@203.0.113.9    │
│  [+ New Connection] [⏻ Terminate Session] [🗀 Open Connections…] ▣▣ │
├──────────────┬──────────────────────────────────┬──────────────────┤
│ Session      │                                  │ Session Info     │
│ Browser      │   the terminal (dark grey)       │                  │
│              │                                  │ IP, banner,      │
│ ▸ Test box   │   ╭─ ● root@vm ─ ~               │ host keys,       │
│   [▶ Start]  │   ╰─❯ git st│atus                │ algorithms,      │
│   [✎]        │        ╰ ghost suggestion        │ OS clues…        │
└──────────────┴──────────────────────────────────┴──────────────────┘
```

## Installing

Build dependencies (Debian, Ubuntu, Pop!\_OS):

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
     libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
     openssh-client
```

Then, with a [Rust toolchain](https://rustup.rs) installed:

```sh
./install.sh          # builds in release mode and installs under ~/.local
./install.sh --uninstall
```

Or just `cargo run --release`.

`openssh-client` is a runtime requirement: SSCL runs the real `ssh` binary for
sessions and uses `ssh-keyscan` to retrieve host keys.

## The ribbon

| Button | What it does |
| --- | --- |
| **New Connection** | Opens the form below. `Ctrl+Shift+N` |
| **Terminate Session** | Kills the session on the centre page. SSH sessions close; the local shell restarts. `Ctrl+Shift+W` |
| **Open Connections Folder** | Opens the app folder in your file browser. `Ctrl+Shift+O` |
| **▣ / ▣** | Toggle the left and right sidebars. `Ctrl+Shift+B` / `Ctrl+Shift+I` |

`Ctrl+Shift+T` opens another local shell. `Ctrl+±` and `Ctrl+0` change the
terminal font size.

### Hiding your details

The eye button in the Session Browser header, or `Ctrl+Shift+P`, masks every
address, user name, key file name and host-key fingerprint in the sidebars, the
title bar and the status bar — for screen-sharing, or for a screenshot in a bug
report. Only SSCL's own chrome is masked: the terminal shows whatever the
remote host sends, and SSCL does not rewrite that.

## Saving a connection

**New Connection** asks for four things:

* **Session name** — the label in the Session Browser, and the folder name
* **Public IP address**
* **Username**
* **Private key** — dragged and dropped onto the dialog (or click to browse)

Port and a free-text note are optional; the port defaults to 22.

Pressing **Create** makes a folder named after the session inside the app
folder, copies the private key into it, runs `chmod 400` on the copy so ssh
accepts it, and writes the rest as JSON next to it:

```
~/.local/share/sscl/connections/Test box/
├── connection.json
└── id_ed25519          (mode 400)
```

```json
{
  "name": "Test box",
  "host": "203.0.113.9",
  "username": "root",
  "port": 22,
  "key_file": "id_ed25519",
  "note": "",
  "created": "2026-01-01 12:00:00 UTC"
}
```

That is everything needed to reconnect later, so **Start** runs:

```sh
ssh -i <folder>/id_ed25519 -o IdentitiesOnly=yes root@203.0.113.9
```

Host-key checking is left at ssh's own defaults — the first-connect prompt
appears in the terminal and is answered there, exactly as it would be in any
other terminal.

**Edit** reopens the same form with the stored values, and can change any of
them. Renaming the session renames its folder; dropping in a new key replaces
the stored one. The same dialog can delete a connection, folder and key
included.

## The terminal

A real PTY running your login shell, or `ssh` for a saved connection.

* xterm-compatible: 256 colours and true colour, alternate screen, bracketed
  paste, mouse reporting, 10 000 lines of scrollback
* Select with the mouse to copy; middle-click pastes the selection
* `Ctrl+Shift+C` / `Ctrl+Shift+V` for the clipboard — plain `Ctrl+C`, `Ctrl+X`
  and `Ctrl+V` stay as the control codes a terminal needs
* `Shift+PageUp` / `Shift+PageDown` and the wheel scroll the scrollback

### Autosuggestions

As you type, the rest of the most recent matching command appears in grey after
the cursor, and a small list offers the alternatives:

* **Right arrow**, `Ctrl+F` or **End** accepts the suggestion
* **Alt+↑ / Alt+↓** cycles through the list, **Esc** dismisses it
* **Tab** is left alone — it is the shell's own completion

Suggestions come from commands you have run in SSCL (kept in
`~/.local/share/sscl/history`, seeded once from `~/.bash_history` or
`~/.zsh_history`) and from the executables on your `$PATH`. Everything stays on
your machine.

Because the shell inside the PTY does its own line editing, SSCL mirrors the
keystrokes it forwards to know what the current line says. Anything it cannot
model faithfully — tab completion, `Ctrl+R`, arrow-key history — makes it stop
guessing until the next `Enter`, so a suggestion is only ever shown when it is
known to be right.

### The prompt

Local shells get a generated two-line prompt (a green or red status dot, user,
host, path, git branch, and the exit code when a command fails):

```
╭─ ● root@vm ─ ~/projects/sscl ─ main*
╰─❯
```

It is written to `~/.local/share/sscl/sscl.bashrc` (or `sscl.zshrc`,
`sscl.fish`), which sources your own configuration first and then sets `PS1`.
Your own dotfiles are never modified. Remote sessions show whatever prompt the
server sends.

## Session Info

For an SSH session, the right sidebar shows what the server itself announced.
SSCL opens a second, short-lived TCP connection, exchanges identification
strings and reads the server's `SSH_MSG_KEXINIT` packet (RFC 4253 §7.1). No
authentication is attempted and nothing is sent beyond an identification
string.

* IP address the host resolved to, port and user
* Identification string, e.g. `SSH-2.0-OpenSSH_9.6p1 Ubuntu-3ubuntu13`
* Protocol version, software and distribution comment
* Key-exchange, host-key, cipher, MAC and compression algorithm lists
* Host keys with their SHA-256 fingerprints and sizes, from `ssh-keyscan`
* Operating-system clues and implementation quirks inferred from the above

The fingerprints match what `ssh-keygen -lf` prints and what ssh shows you on
first connect. Clues and quirks are labelled as inferences, because that is what
they are — "offers sntrup761x25519, so OpenSSH 8.5 or newer" is a good guess,
not a fact the server stated.

`sscl-probe`, installed alongside the app, prints the same information from a
shell:

```sh
sscl-probe example.com 22
```

## Design notes

The window is one flat, low-opacity sheet of glass — no gradients, no colour
tint, just a neutral wash with a hairline edge, the way JetBrains' newer UI
treats its window background. The title bar, the ribbon, both sidebars, the
status bar and the space between them all share it, with no dividers.

The terminal is the exception: an opaque island, inset from the chrome on every
side, with rounded corners and a thin outline, the way an editor pane sits in
those same IDEs. It is the only surface with a background of its own, so it is
the only thing the eye has to find.

Along the bottom is a status bar spanning the full width: which session is on
screen, the terminal's grid size, and whatever the terminal wants to say —
scrollback position, a copy confirmation, how a session ended. None of it
floats over the terminal any more.

### Backdrop blur

On **KDE Plasma (X11)** SSCL asks KWin to blur what is behind it, by setting
`_KDE_NET_WM_BLUR_BEHIND_REGION` on its own window. The region follows the
window's rounded corners and is updated when the window is resized, so the blur
does not spill out as a hard square behind them. Nothing to configure — as long
as KWin's Blur desktop effect is enabled (*System Settings → Desktop Effects*),
it just works.

The glass then adapts to what it is sitting on:

| | |
| --- | --- |
| **Blur active** | thin glass, so the blurred desktop reads through it |
| **No blur** | dense glass, so a busy wallpaper can never interfere with the text |

SSCL picks between them by asking the X server whether a compositor has
announced the blur effect. Override it with `SSCL_BLUR`:

```sh
SSCL_BLUR=on   sscl   # force the thin glass
SSCL_BLUR=off  sscl   # force the dense glass
SSCL_BLUR=auto sscl   # detect (the default)
```

To set the opacity yourself and ignore all of that, give `SSCL_OPACITY` a
percentage — lower is more see-through:

```sh
SSCL_OPACITY=55 sscl
```

On a **Plasma Wayland** session the equivalent is a Wayland protocol that winit
does not expose, so SSCL cannot ask for blur itself and falls back to the dense
glass. Two things get the blur back:

```sh
WINIT_UNIX_BACKEND=x11 sscl        # run under XWayland, where the property works
```

or install [Force Blur](https://github.com/taj-ny/kwin-effects-forceblur) and
add `sscl` to its window list — that blurs the window from KWin's side, on both
session types. Pair either with `SSCL_BLUR=on`.

On other desktops the property is simply ignored. GNOME can blur with the
Blur my Shell extension (its *Applications* component); everywhere else the
dense glass keeps the app translucent and readable.

### Chrome and assets

Because the window is borderless, SSCL draws its own title bar: drag it to move
the window, double-click to maximise, and the window edges are resize handles.

Icons are drawn with vector primitives at runtime and the fonts are the ones
egui bundles, so the binary needs no image or font assets. The application icon
is monochrome — a grey rounded tile with a `>_` prompt — so it sits quietly in a
task bar next to everything else. A unit test asserts every visible pixel of it
stays grey, so it cannot drift back to a colour.

If a launcher still shows an older, coloured icon after an update, it is
caching it. Reinstall and clear the cache:

```sh
./install.sh
rm -rf ~/.cache/icon-cache.kcache        # KDE
kbuildsycoca6 2>/dev/null || kbuildsycoca5
```

## Development

```sh
cargo test                 # unit tests
cargo clippy --all-targets
```

The live SSH probe test is skipped unless you point it at a server:

```sh
SSCL_TEST_SSH_HOST=127.0.0.1 SSCL_TEST_SSH_PORT=22 cargo test
```

Layout:

| Path | Contents |
| --- | --- |
| `src/app.rs` | application state and panel layout |
| `src/store.rs` | connection folders, key installation, `chmod 400` |
| `src/sshinfo.rs` | the SSH transport probe and host-key fingerprints |
| `src/term/` | PTY sessions, the terminal widget, autosuggestions |
| `src/ui/` | splash, window chrome, sidebars, dialogs, widgets |
| `src/theme.rs`, `src/icons.rs` | palette, glass surfaces, vector icons |
| `src/platform/kwin_blur.rs` | the KWin backdrop-blur region |
| `src/privacy.rs` | masking of addresses, user names and key files |

## Licence

Apache-2.0. See [LICENSE](LICENSE).
