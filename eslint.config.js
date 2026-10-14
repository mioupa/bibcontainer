import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

export default defineConfig([
  // 自動生成物（ts-rs の bindings）と Rust 側は対象外にする
  globalIgnores(["dist", "node_modules", "src/bindings", "src-tauri", "crates", "target"]),

  {
    files: ["**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommendedTypeChecked,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2023,
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // 型だけの import は `import type` に統一する
      "@typescript-eslint/consistent-type-imports": "error",
    },
  },

  {
    // shadcn/ui の部品は、部品とそのバリアント定義（buttonVariants など）を同じファイルから export する
    files: ["src/components/ui/**/*.tsx"],
    rules: {
      "react-refresh/only-export-components": "off",
    },
  },

  {
    // 設定ファイルは Node で動く
    files: ["vite.config.ts"],
    languageOptions: { globals: globals.node },
  },

  {
    // テストでは vi.mocked() などの扱いを少し緩める
    files: ["**/*.test.{ts,tsx}"],
    rules: {
      "@typescript-eslint/unbound-method": "off",
    },
  },
]);
