import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// jsdom にない DOM の機能のうち、Radix UI の部品（Select など）が呼ぶものを、何もしない関数で補う
function polyfill(name: string, value: unknown) {
  if (typeof Element !== "undefined" && !(name in Element.prototype)) {
    Object.defineProperty(Element.prototype, name, { value, configurable: true, writable: true });
  }
}
polyfill("scrollIntoView", () => {});
polyfill("hasPointerCapture", () => false);
polyfill("setPointerCapture", () => {});
polyfill("releasePointerCapture", () => {});

// ResizeObserver も jsdom にない。Radix UI のチェックボックスやポップアップが、大きさの監視に使う
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}

afterEach(() => {
  cleanup();
});
