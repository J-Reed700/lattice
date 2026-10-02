// Keep the embedded Canvas entirely local. Excalidraw otherwise falls back to
// its public CDN when its runtime font loader cannot resolve a package asset.
window.EXCALIDRAW_ASSET_PATH = '/excalidraw-assets/';
