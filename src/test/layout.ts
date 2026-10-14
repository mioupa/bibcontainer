// jsdom には画面のレイアウトがないので、仮想スクロールが行を描くよう、要素の大きさを固定で返すようにする。
import { afterAll, beforeAll } from "vitest";

export function stubElementSize(size: { width: number; height: number }) {
  const props = ["offsetWidth", "clientWidth", "offsetHeight", "clientHeight"] as const;
  const originals = new Map<string, PropertyDescriptor | undefined>();

  beforeAll(() => {
    for (const prop of props) {
      originals.set(prop, Object.getOwnPropertyDescriptor(HTMLElement.prototype, prop));
      Object.defineProperty(HTMLElement.prototype, prop, {
        configurable: true,
        get: () => (prop.endsWith("Width") ? size.width : size.height),
      });
    }
  });

  afterAll(() => {
    for (const prop of props) {
      const original = originals.get(prop);
      if (original) Object.defineProperty(HTMLElement.prototype, prop, original);
      else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[prop];
    }
  });
}
