import { configure } from '@testing-library/react';

// Async route chunks and authoritative reads can settle while other repository checks run.
configure({ asyncUtilTimeout: 5_000 });

// jsdom omits pointer capture. Gesture geometry and capture lifecycle are verified in Playwright.
if (typeof HTMLElement !== 'undefined' && !HTMLElement.prototype.setPointerCapture) {
  HTMLElement.prototype.setPointerCapture = () => {};
}
