/**
 * build-installer-win.js
 *
 * Orchestrates the full Windows installer pipeline end-to-end:
 *   1. Build FutureAcademyTray.exe (Rust shell) for x86_64-pc-windows-gnu
 *   2. Prepare the installer payload (tools.7z + firmwares + exe)
 *   3. Run Inno Setup to produce the final .exe installer
 *
 * Usage:
 *   node script/build-installer-win.js          — CLI version (no GUI)
 *   node script/build-installer-win.js --gui   — GUI version (Electron)
 *
 * Required:
 *   - Rust toolchain (cargo)
 *   - Node.js
 *   - Inno Setup 6 (ISCC.exe on PATH or installed to Program Files)
 *   - tools/, firmwares/ directories present (run npm run fetch first)
 */

const {spawnSync} = require('child_process');
const fs = require('fs');
const path = require('path');

const repoRoot = path.resolve(__dirname, '..');
const pkg = require('../package.json');
const isGui = process.argv.includes('--gui');

/** Run a command, inheriting stdio, exiting on non-zero. */
const run = (cmd, args, opts = {}) => {
    const inherited = {stdio: 'inherit', shell: false, ...opts};
    const result = spawnSync(cmd, args, {cwd: repoRoot, ...inherited});
    if (result && result.status !== 0) {
        console.error(`[build-installer-win] "${cmd} ${args.join(' ')}" exited ${result.status}`);
        process.exit(result.status || 1);
    }
};

/** Run a npm script. */
const runNpm = script => {
    console.info(`\n=== [${script}] ===`);
    run('npm', ['run', script], {stdio: 'inherit', shell: true});
};

/** Step 1: Build the Rust tray binary. */
const buildShell = () => {
    console.info('\n=== [build:shell:win] ===');
    run('node', ['script/cargo.js', 'build', '--release', '--manifest-path', 'shell/Cargo.toml', '--target', 'x86_64-pc-windows-gnu'], {
        stdio: 'inherit',
        shell: true
    });
};

/** Step 2: Prepare the installer payload. */
const preparePayload = () => {
    console.info('\n=== [prepare:installer-payload] ===');
    const args = ['script/prepare-installer-payload.js'];
    if (isGui) {
        args.push('--gui');
    }
    run('node', args, {stdio: 'inherit', shell: true});
};

/** Step 3: Run Inno Setup. */
const buildSetup = () => {
    console.info('\n=== [build:setup] ===');
    run('node', ['script/build-setup.js'], {stdio: 'inherit', shell: false});
};

const main = () => {
    if (process.platform !== 'win32') {
        console.error('[build-installer-win] Windows only.');
        process.exit(1);
    }

    const ver = pkg.version;
    console.info(`Future Academy Link — Windows Installer Builder`);
    console.info(`Version : ${ver}`);
    console.info(`Mode    : ${isGui ? 'GUI (Electron)' : 'CLI (Rust-only)'}`);
    console.info(`Output  : dist/FutureAcademy-${ver}-x64-setup.exe`);
    console.info('');

    // 1. Build shell
    buildShell();

    // 2. Prepare payload
    preparePayload();

    // 3. Inno Setup
    buildSetup();

    console.info('\nInstaller ready.');
};

main();
