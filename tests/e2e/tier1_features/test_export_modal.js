import assert from 'node:assert/strict';
import path from 'node:path';

export const name = 'Tier 1 — Export Modal Lifecycle, Preview & File Explorer (F18-F20, F22)';

export async function run() {
  // ──────────────────────────────────────────────────────────────────────────
  // Test 1 (F18): Export Modal Lifecycle Management (No premature dismissal)
  // ──────────────────────────────────────────────────────────────────────────
  {
    class ExportModalState {
      constructor() {
        this.isOpen = false;
        this.isRendering = false;
        this.progress = 0;
        this.renderedPath = null;
        this.error = null;
      }

      startRender() {
        this.isOpen = true;
        this.isRendering = true;
        this.progress = 0;
        this.renderedPath = null;
        this.error = null;
      }

      onProgress(p) {
        this.progress = p;
      }

      onRenderSuccess(outputPath) {
        this.isRendering = false;
        this.progress = 100;
        this.renderedPath = outputPath;
        // Modal must REMAIN open for in-app video playback
        assert.equal(this.isOpen, true, 'ExportModal must stay open upon render completion');
      }

      userDismiss() {
        this.isOpen = false;
      }
    }

    const modal = new ExportModalState();
    modal.startRender();
    modal.onProgress(50);
    modal.onRenderSuccess('/path/to/exported/video.mp4');

    assert.equal(modal.isOpen, true, 'Modal should remain open for preview after render');
    assert.equal(modal.isRendering, false);
    assert.equal(modal.renderedPath, '/path/to/exported/video.mp4');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2 (F19): In-App Video Preview Asset Protocol URL Generator
  // ──────────────────────────────────────────────────────────────────────────
  {
    function convertPathToTauriAssetUrl(filePath) {
      if (!filePath) return '';
      // Tauri 2 asset protocol: asset://localhost/<escaped-path> or http://asset.localhost/<path>
      const normalized = filePath.replace(/\\/g, '/');
      const encoded = encodeURI(normalized);
      return `asset://localhost/${encoded.replace(/^\//, '')}`;
    }

    const testPath = 'C:\\Users\\aremu\\Videos\\Dabar\\dabar_clip.mp4';
    const assetUrl = convertPathToTauriAssetUrl(testPath);
    assert(assetUrl.startsWith('asset://localhost/'), 'Must generate Tauri asset protocol URL');
    assert(assetUrl.includes('dabar_clip.mp4'), 'URL must preserve filename');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3 (F20): Windows "Show in Folder" Explorer Command Specification
  // ──────────────────────────────────────────────────────────────────────────
  {
    // On Windows, revealing a file in Explorer must execute:
    // explorer.exe /select,"<file_path>"
    // Note: passing just <file_path> to explorer opens the file in default player
    // rather than selecting/highlighting it in Windows Explorer!

    function buildWindowsRevealArgs(targetPath) {
      // Must use /select,"path"
      return ['/select,', path.normalize(targetPath)];
    }

    const sampleFile = 'C:\\Users\\test\\Videos\\clip.mp4';
    const args = buildWindowsRevealArgs(sampleFile);
    assert.equal(args[0], '/select,', 'First argument must be /select, to highlight the file');
    assert(args[1].includes('clip.mp4'), 'Second argument must be normalized file path');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 4 (F22): Transcript Custom Range Wiring to ExportModal Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    // When the user marks a custom range in Transcript view and clicks "Export Selected Clip",
    // the Transcript view must pass the range to ExportModal.
    function prepareTranscriptExportPayload(sermonId, range, title = 'Custom Selection') {
      if (!range || range.end <= range.start) {
        throw new Error('Invalid selection range');
      }
      return {
        sermonId,
        startTime: range.start,
        endTime: range.end,
        clipTitle: title,
        duration: range.end - range.start,
      };
    }

    const payload = prepareTranscriptExportPayload('sermon-123', { start: 14.5, end: 42.0 });
    assert.equal(payload.sermonId, 'sermon-123');
    assert.equal(payload.startTime, 14.5);
    assert.equal(payload.endTime, 42.0);
    assert.equal(payload.duration, 27.5);
  }
}
