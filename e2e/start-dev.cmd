@echo off
cd /d "D:\.projects\paraquet Viewer"
npm run tauri dev -- --config "D:\.projects\paraquet Viewer\src-tauri\tauri.e2e.conf.json" > "D:\.projects\paraquet Viewer\.e2e-dev.log" 2>&1
