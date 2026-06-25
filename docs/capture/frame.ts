// CSS injected into the page at capture time (NOT into the app bundle). It turns
// the full-bleed panel into a framed product shot: the panel floats on a subtle
// backdrop with a soft drop shadow, sized to the capture viewport box.
//
// The panel box is fixed (420×640); the backdrop padding around it is tunable so
// stills can breathe (roomy) while the GIF stays tight + legible (the panel fills
// most of the frame so its text renders near 1:1 when embedded).
export const PANEL_W = 420;
export const PANEL_H = 640;

// Roomy padding for stills; tight padding for the GIF.
export const STILL_PAD = { x: 56, y: 60 };
export const GIF_PAD = { x: 22, y: 24 };

export function viewport(pad: { x: number; y: number }): { width: number; height: number } {
  return { width: PANEL_W + pad.x * 2, height: PANEL_H + pad.y * 2 };
}

// Back-compat constants for the default (still) frame.
export const VIEW_W = viewport(STILL_PAD).width; // 532
export const VIEW_H = viewport(STILL_PAD).height; // 760

export function frameCss(pad: { x: number; y: number }): string {
  return `
  html, body { height: 100%; }
  body {
    /* Light, cool-gray gradient with a faint brand-purple tint. Reads cleanly on
       GitHub's light theme (blends with the white page) while still looking
       intentional on dark. The dark panel pops against it either way. */
    background:
      radial-gradient(125% 125% at 50% 0%, #f3f2f8 0%, #e6e5ee 52%, #d5d4e0 100%);
    display: flex; align-items: center; justify-content: center;
    overflow: hidden;
  }
  #app {
    width: ${PANEL_W}px; height: ${PANEL_H}px; flex: none;
    border-radius: 18px; overflow: hidden;
    box-shadow:
      0 24px 55px -14px rgba(40, 34, 80, 0.40),
      0 8px 22px -10px rgba(40, 34, 80, 0.30),
      0 0 0 1px rgba(20, 18, 40, 0.06);
  }
`;
}

// The default (roomy) frame used by the stills.
export const FRAME_CSS = frameCss(STILL_PAD);

// Stills only: the expanded preview shows an enlarged thumbnail while it "prepares
// a preview"; hide the shimmer/label so the still reads as a finished frame.
export const STILL_TWEAKS_CSS = `
  .preview-loading { display: none !important; }
`;
