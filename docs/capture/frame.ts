// CSS injected into the page at capture time (NOT into the app bundle). It turns
// the full-bleed panel into a framed product shot: the panel floats on a subtle
// backdrop with a soft drop shadow, sized to the capture viewport box.
//
// Sizing is the single source of truth shared with harness.ts:
//   panel box 420×640, backdrop padding 56px horizontal / 60px vertical
//   → viewport 532×760.
export const PANEL_W = 420;
export const PANEL_H = 640;
export const PAD_X = 56;
export const PAD_Y = 60;
export const VIEW_W = PANEL_W + PAD_X * 2; // 532
export const VIEW_H = PANEL_H + PAD_Y * 2; // 760

export const FRAME_CSS = `
  html, body { height: 100%; }
  body {
    background: radial-gradient(120% 120% at 50% 0%, #2a2342 0%, #15131f 55%, #0d0c12 100%);
    display: flex; align-items: center; justify-content: center;
    overflow: hidden;
  }
  #app {
    width: ${PANEL_W}px; height: ${PANEL_H}px; flex: none;
    border-radius: 18px; overflow: hidden;
    box-shadow:
      0 24px 60px -12px rgba(0, 0, 0, 0.65),
      0 8px 24px -8px rgba(0, 0, 0, 0.5),
      0 0 0 1px rgba(255, 255, 255, 0.04);
  }
`;

// Stills only: the expanded preview shows an enlarged thumbnail while it "prepares
// a preview"; hide the shimmer/label so the still reads as a finished frame.
export const STILL_TWEAKS_CSS = `
  .preview-loading { display: none !important; }
`;
