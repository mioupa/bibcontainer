#!/bin/bash
# Google Books API の ISBN 検索（q=isbn:）が動いているかを確かめる。
# 2026-10 初めから、isbn:・intitle:・inauthor: を付けた検索が HTTP 200 で 0 件を返す障害が出ている
# （Google Developer forums のスレッド、Issue Tracker 569300353）。直ったかどうかをこれで確かめる。
# API キーはアプリが保存した macOS のキーチェーンから読み、表示もファイルへの保存もしない。
# 使い方: bash scripts/check-google-books.sh
set -u
KEY=$(security find-generic-password -s io.github.mioupa.bibcontainer -a google -w 2>/dev/null)
if [ -z "$KEY" ]; then
  echo "キーチェーンから API キーを読めませんでした（アプリの設定で Google の API キーを保存してください）"
  exit 1
fi
for isbn in 9780132350884 9780141439518 9784003101117; do
  printf '%s  ' "$isbn"
  curl -s "https://www.googleapis.com/books/v1/volumes?q=isbn:$isbn&key=$KEY" | python3 -I -c '
import json, sys
d = json.load(sys.stdin)
if "error" in d:
    print("エラー:", d["error"].get("code"), d["error"].get("status")); sys.exit()
items = d.get("items") or []
vi = (items[0].get("volumeInfo") or {}) if items else {}
print("件数", d.get("totalItems"), " ", (vi.get("title") or "")[:40], " 書影", "あり" if vi.get("imageLinks") else "なし")
'
  sleep 1
done
