# Future Academy — Release Notes

## Version 2.1.7

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Features & Fixes

- Fix Windows serial port I/O error 995 (`ERROR_OPERATION_ABORTED` / `The I/O operation has been aborted because of either a thread exit or an application request`): properly join serial reader thread on port disconnect before spawning upload toolchain and handle aborted close reads gracefully.
- Do not automatically open console viewer window at startup; application starts cleanly in the system tray and the console can be opened anytime via the tray menu ("Show Console").

## Version 2.1.6

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Features & Fixes

- Decouple console as an independent process from the background server.
- The console window close button [X] is now completely standard, native, and clickable.
- Closing the console window terminates only the console viewer process; the link server continues running in the background (system tray).
- Real-time logging streaming via local session log (`link.log`).
- "Show Console" / "Hide Console" in system tray context menu to open/close the console viewer anytime.

## Version 2.1.5

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Features & Fixes

- Fix console closing behavior: closing or dismissing the console window does not terminate the application.
- Disable console `SC_CLOSE` button to prevent accidental process termination.
- Minimize console to system tray: clicking minimize (`_`) on the console window smoothly hides it to the background tray.
- Add "Show Console" / "Hide Console" toggle in system tray context menu.
- Support `Ctrl+C` and typing `exit`/`hide` inside console to hide window without quitting background link server.

## Version 2.1.4

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Features & Fixes

- Automatically install core via `arduino-cli` and retry build/flash when encountering `Platform '<core>' not found: platform not installed`.
- Stream core installation progress directly to the editor console via `sendstd`.
- Ensure deterministic `arduino-cli.yaml` configuration paths on startup to prevent drive mismatch errors.
- Include standard board manager URLs (`BOARD_MANAGER_URLS`) for ESP32, ESP8266, SparkFun, and RP2040 in toolchain configuration.
## Version 2.1.3-b1

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Improvements

- Refactor logging: route all diagnostics to the launching terminal's stderr instead of writing to a per-OS log file (`link.log`). Logs are visible when the tray app is launched from `cmd` / PowerShell / a terminal, and are stripped of ANSI codes when redirected to a file. The **Show Console Log** tray menu item is removed. The `time` crate dependency is dropped as it was only used for log timestamps.
- Console attach: on Windows, `AllocConsole()` is called at startup to rebind stdio to the parent terminal when one exists, making `tracing` output visible in the launching console without requiring a separate console window.

## Version 2.1.3

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Features & Fixes

- Fix toolchain validation failure for modularized Windify libraries (removed obsolete legacy `Windify` directory requirement from `validate_toolchain`).
- Installer auto-installs Visual C++ Redistributable 2015-2022 when missing on clean Windows installs.
- Optimize firmware upload speed and prune tools payload to shrink installer / portable bundle size.

## Version 2.1.3-beta.1
Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Improvements

- Beta iteration on `2.1.3-beta.0`: include installer auto-install of Visual C++ Redistributable 2015-2022 and the firmware-upload speed / pruned tools payload improvements from this cycle.

## Version 2.1.3-beta.0

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Features

- Installer auto-installs Visual C++ Redistributable 2015-2022 when missing, so first-launch flashing no longer fails on clean Windows installs.

### Improvements

- Optimize firmware upload speed and prune the tools payload to shrink installer / portable bundle size.

## Version 2.1.0
Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Bug Fixes

- Persist web-synced Arduino libraries correctly to avoid data loss during toolchain updates.
- Resume interrupted tool downloads without restarting from scratch.

### Improvements

- Refactor library sync module for improved robustness and atomicity.
- Enhanced path validation for library files to prevent unsafe paths.
- Improved file locking mechanism for concurrent library operations.
- Better conflict detection for I2C sensor libraries (TCS34725/VL53L0X).

## Version 2.0.14
Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Bug Fixes

- Prevent `GetFinalPathNameByHandleW` panic on Windows when executable path involves symlinks, junctions, or special filesystem paths.
- Fix registry read panic in `winreg` crate when encountering malformed values (e.g., `REG_LINK` with UTF-16 null terminators).
- Fix library sync path prefix handling in serial port session — correctly strip library name prefix from file paths.

### Improvements

- Move tools directory to fixed `C:\futureacademy\tools` location on Windows for consistent cross-session access.
- Add panic guards around Windows API calls (`sevenz_rust2`, `winreg`) for robustness.
- Update installer scripts and download tools scripts to use new tools path.

### Paths

- Tools: `C:\futureacademy\tools\` (fixed location on Windows)
- User data: `%LOCALAPPDATA%\Future Academy Link\`

## Version 2.0.2

Windows local hardware link server for [Windify Block](https://stem.windify.edu.vn/).

### Build (maintainers)

```bash
npm install
npm run release
```

`release` runs `ensure:tools` first (downloads `tools/` + `firmwares/` via `fetch:small` when missing), then builds the setup EXE and app zip.

Force re-download tools:

```bash
npm run clean
npm run release
```

If GitHub `winblockcc/winblock-tools` returns **404**, `ensure:tools` falls back to `fetch:local`:

- Copies from `C:\Program Files\Future Academy\tools` if installed, or legacy ProgramData path, or
- Set `WINDY_TOOLS_SOURCE` / `TOOLS_7Z_PATH` before `npm run release`

Produces in `dist/`:

- `Future Academy Link-2.0.2-x64-setup.exe` — Inno Setup installer (GUI + tools.7z)
- `FutureAcademy-2.0.2-x64-app.zip` — portable bundle: GUI + `tools/` + `firmwares/` (unzip and run `Future Academy Link.exe`)

Zip only (requires `tools/` + GUI build): `npm run ensure:tools && npm run build:gui:win && npm run release:app-zip`

Publish to update server (from `scratch-link-server` repo): `npm run seed:releases`

### Download (update server)

Set `PUBLIC_BASE_URL` on the server (e.g. `http://14.225.209.18:8080`).

| Artifact | URL pattern |
|----------|-------------|
| Installer (.exe) | `{PUBLIC_BASE_URL}/downloads/FutureAcademy-2.0.2-x64-setup.exe` |
| Portable (.zip) | `{PUBLIC_BASE_URL}/downloads/FutureAcademy-2.0.2-x64-app.zip` |
| Short links | `{PUBLIC_BASE_URL}/download` (exe), `{PUBLIC_BASE_URL}/download/zip` |

### Download (offline / direct file)

- Installer: `FutureAcademy-2.0.2-x64-setup.exe`
- Portable zip: `FutureAcademy-2.0.2-x64-app.zip`
- Platform: Windows 10/11 (64-bit)
- Size: ~400-500 MB (installer)

### Install

1. Run the installer as administrator.
2. Wait for **Extracting build tools...** (usually 1-3 minutes).
3. Launch **Future Academy** from Start Menu.

The app starts the link server (port `11337`) and opens the editor URL.

### Paths

- App: `C:\Program Files\Future Academy\`
- Tools: `C:\Program Files\Future Academy\tools\` (beside `Future Academy Link.exe`)
- User data: `%LOCALAPPDATA%\Future Academy Link\`

### Troubleshooting

- **Windows 11 blocks installer / tools not extracted:** unsigned build + SmartScreen / Defender. See [docs/installer-windows.md](docs/installer-windows.md) — **Unblock** the exe, **More info → Run anyway**, allow in antivirus, rerun as admin.
- **Downloaded from update server over HTTP:** right-click installer → Properties → **Unblock**, then install.
- **Extracting tools failed:** installer now shows a 7-Zip error dialog; whitelist Inno `%TEMP%` and the install folder, then reinstall.
- **Upload/flash failed:** verify `arduino-cli.exe` exists under `{app}\tools`, then reinstall.
- **Missing VL53L0X / Windify:** from repo run `npm run verify:libs`; copy `tools/Arduino/libraries` into the app `tools` folder or rebuild `tools.7z`.
- **Browser not opened:** open [https://stem.windify.edu.vn/](https://stem.windify.edu.vn/) manually.
- **Port in use:** close other Future Academy instances (tray/Task Manager).

### Uninstall

Windows Settings -> Apps -> Future Academy -> Uninstall.
Optional cleanup: delete `%LOCALAPPDATA%\Future Academy Link`.
