<p align="center">
  <img src="assets/bolt.png" width="76" alt="Volt">
</p>

<h1 align="center">Volt</h1>

<p align="center">
  A lightweight Roblox performance app for Windows 10 and 11, written in Rust.<br>
  Real, reversible tweaks — measured numbers only.
</p>

<p align="center">
  <a href="https://github.com/itriedcoding/Vu-Game/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/itriedcoding/Vu-Game/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/itriedcoding/Vu-Game/releases"><img alt="Release" src="https://img.shields.io/github/v/release/itriedcoding/Vu-Game?display_name=tag"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-2F7DFF">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-2F7DFF">
</p>

---

## What it is

Volt is a single native `.exe` that reads what Windows and Roblox actually expose,
applies a short list of reversible tweaks, and shows you the result. There is no
embedded browser, no installer, no updater, no telemetry, and no background
service — closing the window stops everything.

* **Dashboard** — Roblox detection, CPU, RAM, Roblox process memory, frame rate
  when Roblox itself reports it, live optimisation status and the **Optimize
  Roblox** / **Restore defaults** buttons.
* **Optimize** — five reversible tweaks, a ranked list of background programs
  competing for CPU and memory, and memory guidance built from the current measurements.
* **Cleanup** — Roblox session logs, MicroProfiler captures, crash dumps and
  leftover Roblox temp files, with a measured "reclaimable now" figure.

## Download

1. Open the [Releases](https://github.com/itriedcoding/Vu-Game/releases) page and download `volt.exe` from the latest release
   (or `Volt-<version>-windows-x64.zip` if you also want the README and license).
2. Run it. No installation step, no accounts, no first-run wizard.

Windows SmartScreen will warn about an unsigned binary. Choose **More info → Run
anyway** if you trust the source, or build it yourself with the instructions below.

**Requirements:** Windows 10 (64-bit) or Windows 11, plus a normal GPU driver
(the interface renders with OpenGL through `eframe`/`glow`).

## What Volt actually changes

| Optimisation | What happens | Where | Undo |
| --- | --- | --- | --- |
| Roblox process priority | Roblox Player is set to **Above Normal** so Windows schedules its threads ahead of background work | The running `RobloxPlayerBeta.exe` process (`SetPriorityClass`) | Restore puts it back to **Normal** |
| Background recording | Game Bar's background clip recording is turned off | `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\AppCaptureEnabled` and `HKCU\System\GameConfigStore\GameDVR_Enabled` | Previous value restored, or the value removed if Volt created it |
| Windows Game Mode | Game Mode is enabled so the foreground game is prioritised | `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled` | Same |
| Game Bar startup panel | Stops the Game Bar panel opening automatically with a game | `HKCU\Software\Microsoft\GameBar\ShowStartupPanel` | Same |
| High performance power plan | Switches the active Windows power plan | `PowerSetActiveScheme` | The plan that was active before is restored |

Every change is written to `%APPDATA%\Volt\state.json` **before** it is applied, and
**Restore defaults** replays that log in reverse. If a restore fails (for example
because Windows blocked the power plan change), the record stays so you can retry —
Volt never reports a change as undone when it is not.

### What Volt never does

* It never edits Roblox game files, installed client content, or client settings files.
* It never injects code, hooks the renderer, or interacts with Roblox anti-cheat.
* It never disables Defender, antivirus, Windows Update or any Windows service.
* It never writes machine-wide policy keys or other "registry cleaner" tweaks.
* It never closes a program on its own — the background list only offers an
  explicit, per-program **End task** action.

## About the frame-rate card

Roblox does not expose live frame rate to other applications, and Volt does not
inject into the client to fake one. So the card has two honest states:

* **A number** — only when the Roblox client writes a fresh frame-rate stat into its
  own session log (`%LOCALAPPDATA%\Roblox\logs`). The card is then tagged
  `MEASURED` and says how many seconds old the value is.
* **`—`** — otherwise, labelled `Not exposed by Roblox · never estimated`.

Settings values that merely describe a *limit* (`FramerateCap`,
`DFIntTaskSchedulerTargetFps`, "fps cap") are deliberately rejected by the parser so
a configured cap can never be presented as a measurement.

CPU, memory, per-process CPU share and Roblox memory are always measured directly
from Windows, sampled once per second.

## Background programs and memory guidance

Volt ranks running programs by **measured** CPU share and working set. It only
lists programs that are genuinely eating resources: known heavy consumers (browsers,
launchers, overlays, sync clients and similar) above a small threshold, plus any
unknown process using more than 1.5 GB of RAM or 8% of the CPU. Windows components
and security software are never listed and can never be closed from Volt.

Memory guidance is derived from the numbers on screen (for example "93% of your RAM
is in use", "Roblox is using 3.8 GB") and suggests what usually helps. It does not
promise a frame-rate gain, because Volt cannot measure one.

## Cleanup scope

Only these are eligible:

* `%LOCALAPPDATA%\Roblox\logs` — `.log`, `.dmp` and MicroProfiler `.html` files older
  than two minutes.
* `%TEMP%` entries whose name starts with `Roblox`.

Files modified in the last two minutes, files that Windows refuses to delete, and
anything outside those folders are skipped and reported. Installed client versions,
assets, game content and account data are never touched.

## Privileges

Volt runs without administrator rights for all process and per-user settings. The
power plan change is the one item Windows may refuse for a standard user; when that
happens the step is reported as failed with the actual error, and the Restore button
still knows nothing was changed. Volt never asks for elevation on its own.

## Build from source

```powershell
git clone https://github.com/itriedcoding/Vu-Game.git
cd Vu-Game
cargo build --release
```

The executable is `target\release\volt.exe`. Requires a recent stable Rust toolchain
(Rust 1.85 or newer; the interface crate needs that MSRV).

Run it directly:

```powershell
cargo run --release
```

### Checks

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The engine crate (`volt-core`) is platform independent apart from the Win32 layer,
so its parsers, backup/restore logic, cleanup planner and recommendation ranking are
unit tested on any host. The interface crate (`volt`) is Windows-only by design.

## Project layout

```
crates/
  core/            volt-core — the engine, no UI dependencies
    src/platform/  Win32 layer (processes, registry, power plans) + non-Windows stub
    src/optimize.rs  catalogue, apply/restore engine, live verification
    src/cleanup.rs   scan and remove Roblox diagnostics safely
    src/fps.rs       honest log-based frame-rate probe
    src/monitor.rs   once-per-second sampler thread
    src/state.rs     %APPDATA%\Volt\state.json backups
  app/             volt — eframe/egui interface
    src/theme.rs     design tokens and egui styling
    src/widgets.rs   painted cards, buttons, pills, charts
    src/views.rs     Dashboard, Optimize and Cleanup
assets/            icon, generated .ico, Barlow font files
.github/workflows/ CI and tag-triggered release builds
```

The interface is drawn with `egui`: one immediate-mode pass per frame, no layout
engine, no web runtime. When idle it only requests a repaint about once a second,
because that is the sampling interval of the dashboard.

## Releases and CI

* `.github/workflows/ci.yml` runs on every push to `main` and on pull requests:
  formatting, clippy with `-D warnings`, tests, a release build, and it uploads
  `volt.exe` as a build artifact.
* `.github/workflows/release.yml` runs when a `v*` tag is pushed. It validates,
  builds with the size-optimised release profile, packages
  `Volt-<version>-windows-x64.zip`, writes `SHA256SUMS.txt`, and publishes a GitHub
  release containing the standalone `volt.exe`, the zip and the checksums.

```powershell
git tag v1.0.0
git push origin v1.0.0
```

The workflow also accepts two other entry points:

* Push a commit to the `release` branch — the version is read from `Cargo.toml`
  (`v1.0.0` for version `1.0.0`) and published.
* Open **Actions → Release → Run workflow** and enter the tag you want.

### Release profile

`opt-level = "z"`, fat LTO, one codegen unit, `panic = "abort"` and symbols stripped —
the binary stays small and starts fast, and the GUI cannot link a second copy of the
Rust runtime.

## FAQ

**Why is the FPS card often `—`?**
Because Roblox rarely writes a live frame rate to its log, and Volt will not guess.
Press `Esc` → Settings → Performance in-game for Roblox's own counter.

**Does this improve my FPS?**
It removes avoidable competition for CPU time and background capture work, and it
does not claim a specific number. Volt has no way to measure a gain it did not cause,
so it does not display one.

**Do I need to keep Volt open?**
Only while you apply or restore settings. Priority changes apply to the running
client; the registry and power-plan changes persist in Windows until you restore them.

**Is Roblox Studio supported?**
Volt targets Roblox Player. Studio is not modified in any way.

## License and disclaimer

MIT — see [LICENSE](LICENSE). Bundled Barlow fonts are licensed under the SIL Open
Font License 1.1 (see `assets/fonts/OFL.txt`).

Volt is not affiliated with, endorsed by, or associated with Roblox Corporation.
"Roblox" is a trademark of Roblox Corporation. Use at your own risk.
