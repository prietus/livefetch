# livefetch

A fastfetch-style system info tool, with two twists the others don't have:

1. **Animated logos** — GIF / WebP / PNG / JPG, rendered with Kitty graphics, iTerm2 inline images, or a half-block ANSI fallback. Falls back to a built-in ASCII distro logo (auto-detected from `/etc/os-release`) when no image is given.
2. **Optional live dashboard** — pass `--live` and it stays open, refreshing CPU / memory / swap / network every 500 ms with Unicode sparklines and bars. Without it, behaves like fastfetch: print and exit.

![livefetch](docs/demo.gif)

## Install

### Arch Linux (AUR)

```sh
yay -S livefetch-bin    # prebuilt binary from the latest GitHub release
yay -S livefetch-git    # builds from master
```

(or `paru`, or any AUR helper of your choice)

### From source

```sh
cargo install --path .
```

Or build a release binary:

```sh
cargo build --release
# binary lands in target/release/livefetch
```

Requires a recent Rust toolchain (2021 edition).

## Usage

```sh
# Snapshot mode (default): print info + first frame and exit (good for .zshrc)
livefetch --image ~/Pictures/tux.gif

# Live dashboard: refreshes metrics + animates the image until Ctrl-C
livefetch --image ~/Pictures/tux.gif --live

# Custom refresh interval
livefetch --refresh 1000

# Pick the image protocol explicitly
livefetch --protocol kitty   # or iterm2, ansi, none

# Custom module list
livefetch --modules os,kernel,cpu,memory,network

# List every available module
livefetch --list-modules

# Force a specific ASCII distro logo (auto-detected by default; `none` disables it)
livefetch --logo arch
livefetch --list-logos

# Strip a solid background from the logo (handy for transparent-looking gifs)
livefetch --image logo.gif --chroma auto
livefetch --image logo.gif --chroma '#ffffff' --chroma-tolerance 32
```

### Flags

| Flag | Description |
| --- | --- |
| `--image <PATH>` | Image to display (gif / webp / png / jpg). |
| `--protocol <auto\|kitty\|iterm2\|ansi\|none>` | Force a specific image protocol. |
| `--image-cols <N>` | Columns reserved for the image (default 34). |
| `--no-animate` | Render only the first frame. |
| `--live` | Live dashboard — stay open and refresh metrics until Ctrl-C. |
| `--refresh <MS>` | Live metrics refresh interval, milliseconds (default 500, min 50). |
| `--modules <a,b,c>` | Override the module list. |
| `--list-modules` | Print every available module + description. |
| `--logo <NAME>` | ASCII distro logo when no `--image` is given. `auto` (default), `none`, or any name from `--list-logos`. |
| `--list-logos` | Print every built-in ASCII logo name. |
| `--chroma <auto\|#RRGGBB\|none>` | Remove a solid background colour from the image. |
| `--chroma-tolerance <0-255>` | Per-channel tolerance when matching the chroma colour. |
| `--config <PATH>` | Path to a JSON config (default `~/.config/livefetch/config.json`). |

### Config file

`~/.config/livefetch/config.json` (or `%APPDATA%\livefetch\config.json` on Windows):

```json
{
  "image": "~/Pictures/tux.gif",
  "protocol": "auto",
  "image_cols": 34,
  "animate": true,
  "live": false,
  "refresh_ms": 500,
  "modules": ["title", "separator", "os", "kernel", "cpu", "memory", "network", "colors"],
  "chroma": "auto",
  "chroma_tolerance": 24,
  "logo": "auto"
}
```

CLI flags always override the config file.

## Modules

Run `livefetch --list-modules` for the full set. A quick summary:

- **Identity**: `title`, `separator`, `os`, `host`, `kernel`, `init`, `uptime`, `loadavg`, `packages`
- **Session**: `shell`, `terminal`, `session`, `de`, `wm`, `resolution`
- **Live metrics**: `cpu`, `cputemp`, `memory`, `swap`, `network`, `disk`
- **Network**: `localip`, `localip6`
- **Misc**: `audio`, `theme`, `battery`, `locale`, `gpu`, `gpudriver`
- **Layout**: `break`, `colors`

Static modules (OS, packages, GPU…) are computed once and cached; only the
live ones (`cpu`, `memory`, `swap`, `network`, …) recompute on each tick.

## Platform support

| Platform | Status |
| --- | --- |
| Linux | First-class. All modules. |
| macOS | First-class. All modules except Linux-specific (`init`, `cputemp`, GTK `theme`). |
| Windows | Core modules work; some platform-specific bits return `n/a`. |

Terminal protocols:

- **Kitty** — Kitty, Ghostty (auto-detected).
- **iTerm2 inline images** — iTerm2, WezTerm.
- **ANSI half-blocks** — fallback for anything else (xterm, Alacritty, tmux, terminals without graphics).

## Why not fastfetch / neofetch?

- They don't animate logos.
- They have no live mode.

`livefetch` works as a drop-in replacement (print and exit, drop into your `.zshrc`), but pass `--live` and it turns into a small dashboard — CPU / RAM / network sparklines next to your animated logo — until you Ctrl-C.

## Releases

Tag-driven via GitHub Actions. Pushing a tag matching `v*` builds release
binaries for Linux x86_64 and macOS aarch64 (Apple Silicon), and attaches
them — plus SHA-256 checksums — to a GitHub Release.

```sh
git tag v0.1.0
git push origin v0.1.0
```

## License

[MIT](LICENSE). Bundled ASCII distro logos are adapted from
[neofetch](https://github.com/dylanaraps/neofetch) (also MIT) — full notice in
[THIRD-PARTY-LICENSES](THIRD-PARTY-LICENSES).
