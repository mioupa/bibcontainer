# 0029: PC カメラでのバーコード読み取り

## 状態

採用（2026-11-15 にユーザーが設計を承認した。実装済み。zxing-wasm の API・wasm の同梱・CSP は、ビルドと node での読み取りまで確かめた。jsdom のテストまで。**実機（WKWebView・WebView2）での操作は未確認**。「影響・今後」の一覧のとおり）

## 日付

2026-11-15

## 背景

0022 §1 の 7 番目（最後。FR-SCAN-20 [COULD]）。PC のカメラ（Mac の連係カメラで iPhone を使う場合を含む）で EAN-13 を読み、数字列を、リーダーからの入力と同じ処理に渡す。macOS では `NSCameraUsageDescription` が要る。

読み取りは Tauri のウィンドウ（macOS は WKWebView、Windows は WebView2）の中の Web API（`getUserMedia`）で行う。ウィンドウの種類をまたいで動かす必要がある。

## 決定

### 1. 置き場所と、オン・オフ

- スキャン登録画面（`ScanScreen.tsx`）の右カラム、`ManualInput` の上に「カメラで読む」のカード（`src/features/scanScreen/camera/CameraPanel.tsx`）を置く。中身は、オン・オフの切り替え、映像のプレビュー（`<video>` の `srcObject`、`muted playsInline`）、カメラの選択（`navigator.mediaDevices.enumerateDevices()` で 2 台以上あるときだけ出す `<select>`）、状態・失敗の文。
- **画面を開いたときは毎回オフ。** オン・オフは保存しない。選んだカメラの deviceId だけ、`sessionStore.ts` の persist（localStorage。`bibcontainer.scanMode`）の `cameraDeviceId` に足して覚える（`partialize` に追加）。`merge` は `mode` だけの古い保存も読め、足した項目がなければ初期値（null。既定のカメラ）のままなので、`version` は 1 のまま。Rust 側の設定（`settings.rs`）には足さない（端末ごと・画面の都合の値で、同期の対象でも、他の機能が読む値でもないため）。
- 切り替えと、カメラの選択を操作したあと、フォーカスを手入力欄に戻す（`focusManualInput`）。切り替えのボタンにフォーカスがあるときのスペースは、画面全体の確定キー（`useCommitKey`。keydown と keyup を止める）が先に拾うので、ボタンの操作にはならず「確定」になる。ほかのボタン（モード切り替えなど）と同じ作法（0012 §3・§9）。

### 2. 読み取り（`camera/decode.ts`・`camera/useCameraScan.ts`）

- ライブラリは npm の `zxing-wasm` **3.1.6**（`package.json` に版を固定。内蔵の zxing-cpp は commit `2c3dcfe`）。`zxing-wasm/reader`（読み取りだけの小さい版。`full` は書き込みも入って大きい）を使う。
- 確かめた API（導入した版の `dist/es/reader/index.d.ts`・`bindings/readerOptions.d.ts`・`share.d.ts` を読み、`node` で実際に読ませた）:
  - `readBarcodes(input: Blob | ArrayBuffer | Uint8Array | ImageData, readerOptions?: ReaderOptions): Promise<ReadResult[]>`。
  - 使うオプション: `formats: ["EAN13"]`（正式な名前は `EAN13`。`EAN-13` も別名で通るが、正式名を使う。`ISBN`・`EANUPC` ではない）、`tryHarder: true`、`maxNumberOfSymbols: 4`（2 以上。余裕を見て）、`eanAddOnSymbol: "Ignore"`（`"Ignore" | "Read" | "Require"`。EAN-2・5 のアドオンは読まない。2段目の 192 は別の EAN-13 なので、これで読める）。`tryRotate` `tryInvert` `tryDownscale` は既定のまま。
  - `ReadResult` の `text`（数字列）と `isValid`、`format`（`"EAN13"`）を使う。チェックディジットの検証は zxing が行い、`isValid` が偽のものは捨てる。そのうえで、判定そのもの（雑誌・書籍以外・ISBN の正しさ）は、リーダーと同じ `classify_code`（Rust）に任せる。フロントでは独自に判定しない。
  - node での確認: 有効な ISBN の EAN-13 1 本、ISBN と 192 の 2 本を縦に並べた画像で、どちらも読めた（EAN-13 を自前で描いた画像。実際のカメラの映像ではない）。
- **wasm はアプリに同梱し、既定の CDN には取りに行かせない。** zxing-wasm の既定の `locateFile` は `https://fastly.jsdelivr.net/npm/zxing-wasm@3.1.6/dist/...` を返す（`share.js`）。`prepareZXingModule({ overrides: { locateFile }, fireImmediately: true })` で上書きする（`overrides` を渡すと既定の `locateFile` は使われない。`share.js` の `prepareZXingModule` は、`overrides` があればそれを、なければ既定を使う）。`locateFile` は、`.wasm` なら `import("zxing-wasm/reader/zxing_reader.wasm?url")` の値（Vite が dist に出すファイルの URL）を返す。`?url` の型は `vite/client` の宣言で足りる。`fireImmediately: true` で、wasm の読み込みの失敗をその場で受ける（失敗したら次のオンでやり直す）。
- ビルドでの確認（`pnpm build`）: `dist/assets/zxing_reader-BCjuyud6.wasm`（946.86 kB、gzip 414 kB）が出る。zxing の JS は別のチャンク `reader-*.js`（37.7 kB）で、`?url` のモジュール（`export default "/assets/zxing_reader-….wasm"`）も別チャンク。どちらも**カメラをオンにしたときに初めて動的 import で読み込む**。メインのバンドル（`index-*.js`）は 1,065.54 kB → 1,074.55 kB（+9.0 kB、gzip +3.0 kB。カメラの画面の部品の分）。jsDelivr の URL の文字列は、`reader-*.js` の中の既定の `locateFile` に残るが、上書きしているので使われない（`index-*.js` には出ない）。
- メインスレッドで動かす（Worker は使わない）。フレームは `requestVideoFrameCallback`（なければ `requestAnimationFrame`）で取り、映像の中央の高さ 60% の帯（上下 2 段が入る高さ）を、幅 960 px までに縮めて canvas に描き、`ImageData` にして読む。読み取りは前のが終わるまで次を始めず、始めから 150 ms 以上あける。
- カメラは `getUserMedia({ video: { width: { ideal: 1280 }, height: { ideal: 720 }, deviceId?: { exact } }, audio: false })`。一覧（`enumerateDevices`）は、許可を得たあとに取る（許可の前は名前が空になる）。

### 3. 同じコードの抑制（`camera/dedupe.ts`。純粋な関数 `stepDedupe`。時刻は引数）

カメラは同じバーコードを毎フレーム読むので、次の規則で絞る。

1. 同じ値が**連続する 2 回の読み取り**で読めたら採用する（1 フレームの読み間違いを渡さない）。間に読めない回があれば、連続とは数えない。
2. 一度渡した値は、映像から**消えて 1.5 秒（`GONE_MS`）**たつまで渡し直さない。映したままなら 1 回だけ。消えて 1.5 秒たってから映せば、もう一度渡す。
3. 1 回の読み取りに複数の値があれば、ISBN（978・979）→ 2段目（192）→ ほかの順に 1 つずつ渡す。
4. 13 桁の数字以外は渡さない。978・979・192 以外の 13 桁（食品など）は渡し、`classify_code` がリーダーと同じ理由で断る。

### 4. 渡し先と、止める条件

- 渡し先は `useScanFlow()` の `submitDigits(digits)`（`useScanCapture` には合流させない。`useScanCapture` は、キー入力を数字列にまとめる層で、カメラには要らない）。音・ペアリング・確定・書誌取得は、それ以降のふだんの流れ。
- `flow.listening` が偽（フォーム・欲しい本の確認を開いている）、または `useAnyModalOpen()` が真のあいだは、渡さない。**読み取りは続け**、その間に映った値は「まだ映っている」として覚える（`stepDedupe` の `accepting` が偽）。止めている間に読み取りをやめてしまうと、閉じたあとに映したままの本が「新しく映った」ことになり、前のコードが入ってしまうため。
- 画面を離れた（アンマウント）、切り替えをオフにした、`document.visibilitychange` でウィンドウが隠れた、のときは、すべての track を `stop()` してカメラを放す。隠れて止めたあと、オンのままなら、見えるようになったときに開き直す（抑制の状態は初めから）。React の StrictMode の二重マウントと、`getUserMedia` の Promise がアンマウントのあとに解決した場合は、解決した時点で、すぐ stop する（`cancelled` の印）。track の `ended`（カメラを外した）は「カメラが見つかりません」にする。
- 音はカメラ側では鳴らさない。値を渡したとき、映像の枠の色を 0.4 秒だけ変える（`data-flash`。音なし）。

### 5. 失敗したとき（`camera/errors.ts`）

カードに理由と直し方を日本語で出す（文言は `src/i18n/ja/scan.ts` の `screen.camera`）。失敗してもリーダーと手入力は使える。

| 原因 | 種類 | 内容 |
|---|---|---|
| `navigator.mediaDevices` がない、`isSecureContext` が偽 | `unsupported` | この画面では使えない |
| `NotAllowedError`（`SecurityError`） | `denied` | macOS は「システム設定 → プライバシーとセキュリティ → カメラ」、Windows は「設定 → プライバシーとセキュリティ → カメラ」で許可する |
| `NotFoundError` | `notFound` | カメラをつなぐ |
| `NotReadableError`（`AbortError`） | `inUse` | ほかのアプリが使っていないか |
| `OverconstrainedError`（覚えていた deviceId のカメラがない。`NotFoundError` も、deviceId を指定していたときは同じに扱う） | — | 既定のカメラでやり直す（文は出さない） |
| wasm の読み込みの失敗、続けて 5 回の読み取りの失敗 | `decoder` | 再起動。カメラは放す |
| ほか | `other` | もう一度オンに |

### 6. Tauri の設定

- **CSP**（`src-tauri/tauri.conf.json`）: `script-src` に `'wasm-unsafe-eval'` を足した（`'self' 'wasm-unsafe-eval'`）。wasm のコンパイル・実行（`WebAssembly.instantiate`）に要る。`'unsafe-eval'` ではなく、wasm だけを許す。**ほかは広げない**: Worker は使わないので `worker-src` は足さない。`<video>.srcObject`（MediaStream）は `media-src` の対象ではないので足さない。wasm ファイルは同じオリジン（`'self'`）から `fetch` するので、`connect-src` も変えない。
- **Info.plist**: `src-tauri/Info.plist` を新しく作り、`NSCameraUsageDescription`（「本のバーコード（ISBN）を読み取って登録するために、カメラを使います。」）だけを入れた。Tauri がビルドのときに、自動で生成する Info.plist と統合する。macOS の開発モード（`pnpm tauri dev`）は、バンドルの Info.plist を持たない実行ファイルを直接起動するので、この文言が使われない可能性がある（「影響・今後」の未確認）。`pnpm tauri build --bundles app` は実行していない。
- **WebView 側の許可**（wry 0.57.0。`Cargo.lock` の版と、`~/.cargo/registry` のソースで確かめた）: macOS の `wkwebview/class/wry_web_view_ui_delegate.rs` の `requestMediaCapturePermissionForOrigin` は、権限ハンドラ（`permission_handler`）が設定されていなければ `WKPermissionDecision::Grant` を返す。つまり WebView 側は許可し、**OS（TCC）のカメラの許可のダイアログに任せる**。ハンドラを設定すると `Default` が `Prompt`（WebKit 自身のダイアログ）になるので、設定しない。Windows の `webview2/mod.rs` は、ハンドラがなければ `PermissionRequested` に何もせず（クリップボードの読み取りの許可だけ別）、WebView2 の既定の動き（許可のダイアログを出す）になる。Tauri 2 の側でハンドラを設定している箇所は、このアプリにはない。

### 7. BarcodeDetector を使わない理由

Web 標準の `BarcodeDetector` は、WebKit（WKWebView）にはなく、Chromium（WebView2）でも、Windows のデスクトップでは使えない実装が多い（プラットフォームの機能に頼る）。このアプリは macOS（WKWebView）と Windows（WebView2）の両方で、同じに動く必要があり、どちらでも確実に使える保証がない。wasm の zxing は、どちらでも同じに動く（手元の Mac では未確認。「影響・今後」）。

### 8. ブラウザ（`pnpm dev`）

スキャン登録画面は Tauri の外では使えない作り（0012）なので、カメラもそのまま。dev 用のモックは足さない。

## 理由

- 抑制の規則を、画面から独立した純粋な関数にしたのは、時刻・順序・止めている間の扱いのすべての組み合わせを、タイマーなしで確かめられるようにするため。
- 止めている間も読み取りを続けるのは、§4 のとおり、閉じた直後に前のコードを入れないため。読み取りの負荷（メインスレッドで 150 ms おき）は、止めている間も同じだが、フォームを開いている時間は短い。
- メインスレッドで動かすのは、Worker に wasm と JS を渡す仕組み（CSP の `worker-src`、Vite の Worker のバンドル）を足さず、最小の構成で始めるため。EAN-13 を 960 px 幅の帯で読む重さで、画面の操作が引っかかるなら、Worker に移す（読み取りの関数だけを差し替えればよい）。
- wasm を `?url` で同梱するのは、蔵書管理アプリが、使うたびに外部の CDN へ通信するのは望ましくない（オフラインでも読める。バージョンが固定される）ため。

## 影響・今後

### 要件書との差異

なし。FR-SCAN-20 のとおり。要件書の「Info.plist に設定する」は、`src-tauri/Info.plist` で満たした。

### 変えたもの

`package.json`（`zxing-wasm` 3.1.6）、`src-tauri/tauri.conf.json`（CSP）、`src-tauri/Info.plist`（新規）、`src/features/scanScreen/camera/`（新規。`dedupe.ts`・`decode.ts`・`errors.ts`・`useCameraScan.ts`・`CameraPanel.tsx` とテスト）、`ScanScreen.tsx`（カードを置く）、`sessionStore.ts`（`cameraDeviceId`）、`src/i18n/ja/scan.ts`（`screen.camera`）。受け入れ確認は `docs/acceptance/phase3.md` の AT-58〜AT-62 と、3 章の V1〜V9・X6。

### 未確認（実機で確かめる）

- `pnpm tauri dev` でカメラを開くとき、macOS の許可のダイアログが出るか。出ないとき、どのアプリ（ターミナルなど）が許可の対象になるか。`Info.plist` の文言が、開発モードで表示されるか（バンドルしたアプリでは表示されるはず）。
- 連係カメラ（iPhone）が一覧に出て、映像が来るか。
- Windows（WebView2）の許可のダイアログの出方と、その記憶（次回から聞かれないか。アプリのデータの場所に保存されるか）。
- `tauri://localhost`（macOS）・`http://tauri.localhost`（Windows）が secure context か（`isSecureContext` が真か）。偽なら、`getUserMedia` は使えず、「この画面では使えません」と出る。その場合は、Tauri の設定（`dangerousUseHttpScheme` など）の見直しになる。
- wasm の読み込みが、`tauri://` と `http://tauri.localhost` の配信で通るか（`WebAssembly.instantiateStreaming` は MIME が `application/wasm` でないと使えない。Emscripten の読み込みは、失敗すると ArrayBuffer での方法に切り替わるはずだが、確かめていない）。
- 実際のカメラの映像での読み取りの速さ・精度（暗さ、反射、距離、上下 2 段の同時）と、メインスレッドで動かしたときの画面の引っかかり。Worker への移行の要否の判断材料にする。
- `pnpm tauri build --bundles app` した .app で、Info.plist が統合され、カメラの許可の文言が出ること。
