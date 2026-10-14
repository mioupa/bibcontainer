import { describe, expect, it } from "vitest";
import cCodeRust from "../../crates/core/src/barcode/c_code.rs?raw";
import requirements from "../../docs/requirements.md?raw";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";

/** Rust のソースから、`const NAME: [&str; N] = [ "…", … ];` の文字列を順に取り出す。 */
function rustStrings(name: string): string[] {
  const body = new RegExp(`const ${name}: \\[&str; \\d+\\] = \\[([^\\]]*)\\];`).exec(
    cCodeRust,
  )?.[1];
  expect(body, `${name} が c_code.rs にない`).toBeDefined();
  return [...(body ?? "").matchAll(/"([^"]*)"/g)].map((m) => m[1] ?? "");
}

/** Rust のソースから、`const CONTENT: [(&str, &str); N] = [ ("00", "総記"), … ];` の組を取り出す。 */
function rustContent(): Record<string, string> {
  const body = /const CONTENT: \[\(&str, &str\); \d+\] = \[([^\]]*)\];/.exec(cCodeRust)?.[1];
  expect(body, "CONTENT が c_code.rs にない").toBeDefined();
  return Object.fromEntries(
    [...(body ?? "").matchAll(/\("(\d\d)", "([^"]*)"\)/g)].map((m) => [m[1] ?? "", m[2] ?? ""]),
  );
}

const byDigit = (names: string[]) => Object.fromEntries(names.map((name, i) => [String(i), name]));

describe("開発用の C コード表（付録 A）", () => {
  // 名称は原本どおり（付録 A・Rust の c_code.rs と同じ）。ここで食い違うと、ブラウザのモックだけ別の名称になる。
  it("販売対象・発行形態・大分類・中分類の名称が、Rust の表（c_code.rs）と同じ", () => {
    expect(PROVISIONAL_C_CODE_TABLE.audience).toEqual(byDigit(rustStrings("AUDIENCE")));
    expect(PROVISIONAL_C_CODE_TABLE.format).toEqual(byDigit(rustStrings("FORMAT")));
    expect(PROVISIONAL_C_CODE_TABLE.contentMajor).toEqual(byDigit(rustStrings("CONTENT_MAJOR")));
    expect(PROVISIONAL_C_CODE_TABLE.content).toEqual(rustContent());
  });

  it("原本の名称になっている（以前の名称ではない）", () => {
    const t = PROVISIONAL_C_CODE_TABLE;
    expect(t.audience["5"]).toBe("女性");
    expect(t.audience["6"]).toBe("学参I（小中）");
    expect(t.format["5"]).toBe("辞典・事典");
    expect(t.format["8"]).toBe("電磁的記録媒体など");
    expect(t.format["9"]).toBe("コミックス");
    expect(t.content["23"]).toBe("伝記・系譜");
    expect(t.content["56"]).toBe("海事・兵器");
    expect(t.content["58"]).toBe("各種工業");
    expect(t.content["78"]).toBe("生活");
  });

  // 要件書の付録 A の表（Markdown）の名称とも照合する。表の行は `| 5 | 女性 |` と `| 7 芸術・生活 | 70 … ／ 71 … |`。
  it("要件書の付録 A の名称と同じ", () => {
    const appendix = requirements.slice(requirements.indexOf("## 付録 A"));
    const section = (title: string) => {
      const start = appendix.indexOf(title);
      const next = appendix.indexOf("###", start + title.length);
      return appendix.slice(start, next === -1 ? undefined : next);
    };
    const digitRows = (text: string) =>
      Object.fromEntries(
        [...text.matchAll(/^\| (\d) \| (.+) \|$/gm)].map((m) => [m[1] ?? "", m[2] ?? ""]),
      );
    expect(PROVISIONAL_C_CODE_TABLE.audience).toEqual(digitRows(section("### 第 1 桁")));
    expect(PROVISIONAL_C_CODE_TABLE.format).toEqual(digitRows(section("### 第 2 桁")));

    const content = section("### 第 3・4 桁");
    const major: Record<string, string> = {};
    const middle: Record<string, string> = {};
    for (const m of content.matchAll(/^\| (\d) (\S+) \| (.+) \|$/gm)) {
      major[m[1] ?? ""] = m[2] ?? "";
      for (const item of (m[3] ?? "").split("／")) {
        const [code, ...name] = item.trim().split(" ");
        middle[code ?? ""] = name.join(" ");
      }
    }
    expect(PROVISIONAL_C_CODE_TABLE.contentMajor).toEqual(major);
    expect(PROVISIONAL_C_CODE_TABLE.content).toEqual(middle);
  });
});
