#!/usr/bin/env node
/**
 * build-gui.js
 *
 * Builds the Future Academy Link GUI application using Rust/cargo
 * with the gui feature enabled, outputting to the /build folder.
 *
 * Usage: node script/build-gui.js [--release]
 */

const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const SHELL_DIR = path.resolve(__dirname, "..", "shell");
const BUILD_DIR = path.resolve(__dirname, "..", "build");
const TARGET_DIR = path.resolve(SHELL_DIR, "target");

// Parse arguments
const args = process.argv.slice(2);
const isRelease = args.includes("--release") || args.includes("-r");
const profile = isRelease ? "release" : "debug";

// Determine source and destination exe names
const EXE_NAME = "FutureAcademyTray.exe";
const SRC_EXE = path.resolve(TARGET_DIR, profile, EXE_NAME);
const DEST_EXE = path.resolve(BUILD_DIR, EXE_NAME);

console.log(`Building GUI (${profile})...`);
console.log(`  Source: ${SRC_EXE}`);
console.log(`  Dest:   ${DEST_EXE}`);

// Build with cargo
console.log("\nRunning cargo build...");
const cargoArgs = [
    "build",
    isRelease ? "--release" : "",
    "--manifest-path", path.join(SHELL_DIR, "Cargo.toml"),
    "--features", "gui",
].filter(Boolean);

try {
    execSync(`cargo ${cargoArgs.join(" ")}`, {
        cwd: SHELL_DIR,
        stdio: "inherit",
        shell: true,
    });
} catch (err) {
    console.error("\nCargo build failed:", err.message);
    process.exit(1);
}

// Ensure build directory exists
if (!fs.existsSync(BUILD_DIR)) {
    console.log(`\nCreating build directory: ${BUILD_DIR}`);
    fs.mkdirSync(BUILD_DIR, { recursive: true });
}

// Copy exe to build folder
if (fs.existsSync(SRC_EXE)) {
    console.log(`\nCopying ${EXE_NAME} to ${BUILD_DIR}...`);
    fs.copyFileSync(SRC_EXE, DEST_EXE);
    const stats = fs.statSync(DEST_EXE);
    console.log(`Done! Output size: ${(stats.size / 1024 / 1024).toFixed(2)} MB`);
} else {
    console.error(`\nError: Build output not found at ${SRC_EXE}`);
    console.error("Build may have succeeded but produced a different filename.");
    process.exit(1);
}

console.log("\nBuild complete!");
console.log(`  Executable: ${DEST_EXE}`);
