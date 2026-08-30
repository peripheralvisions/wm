# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands
Because the terminal sessions might not have `cargo` in the `$PATH` immediately after installation, use the absolute path for Cargo on Windows:
- **Build Tauri App:** `npm run tauri build` or `~/.cargo/bin/cargo.exe build --manifest-path src-tauri/Cargo.toml`
- **Run Tauri App:** `npm run tauri dev`
- **Run tests:** `~/.cargo/bin/cargo.exe test --manifest-path src-tauri/Cargo.toml`
- **Add Shadcn UI components:** `npx shadcn@latest add <component-name>`

## Code Architecture
This project is a Windows 11 scrollable tiling window manager. It is built with **Rust** (for low-level Windows API interactions) and **Tauri + React + Tailwind CSS + shadcn/ui** (for the configuration GUI and system tray).

### Backend (Rust - `src-tauri/`)
- Intercepts window lifecycle events (`EVENT_OBJECT_CREATE`, `EVENT_OBJECT_DESTROY`) using `SetWinEventHook`.
- Listens for `EVENT_SYSTEM_MOVESIZEEND` to handle manual resizing by users using the mouse.
- Captures `Alt+Scroll` via low-level mouse hooks (`WH_MOUSE_LL`) to scroll the infinite horizontal strip left and right.
- Uses `DwmSetWindowAttribute` with `DWMWA_CLOAK` to hide artifacts when automatically resizing/shifting windows.
- Manages custom vertical workspaces by toggling visibility (`ShowWindow`) of windows rather than relying on the fragile native Windows 11 Virtual Desktops API.
- Implements `DwmGetWindowAttribute` with `DWMWA_EXTENDED_FRAME_BOUNDS` to prevent gaps caused by invisible resizing borders on Windows 11.

### Frontend (React - `src/`)
- An invisible Tauri app that sits in the System Tray.
- When the user clicks the tray icon, it opens a configuration dashboard built with `shadcn/ui`.
- Allows users to configure default window sizes, gaps, scroll speeds, and bindings.
