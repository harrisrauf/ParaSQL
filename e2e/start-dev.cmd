@echo off
cd /d "D:\.projects\paraquet Viewer"
set "WEBVIEW2_USER_DATA_FOLDER=D:\.projects\paraquet Viewer\src-tauri\target\debug\e2e-profile"
npm run tauri dev -- --config "D:\.projects\paraquet Viewer\src-tauri\tauri.e2e.conf.json" > "D:\.projects\paraquet Viewer\.e2e-dev.log" 2>&1
