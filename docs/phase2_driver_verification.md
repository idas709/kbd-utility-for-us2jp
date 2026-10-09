# Phase 2：ドライバー技術検証報告書

**対象プロジェクト：** HID Keyboard Layout Remapper  
**検証日：** 2026年10月9日  
**ステータス：** 検証完了 (Verified)

---

## 1. 概要

本ドキュメントは、要件定義書 Version 1.3「Phase 2：ドライバー技術検証」および「第11章 技術的な留意事項」に基づき、Windows 11 上におけるデバイス別入力変換およびKMDFフィルタードライバーの技術的成立性を検証した結果をまとめたものである。

---

## 2. 技術検証項目と結論

### 2.1 USB HIDキーボードに対するデバイス単位のフィルター配置

- **検証結果：** 成立確認 (Verified)
- **アーキテクチャ：**
  - Kbdclass Upper Filter (`kbfiltr`) 方式を採用。
  - PnP スタックにおいて、キーボードデバイスが接続されると `KbFiltr_EvtDeviceAdd` が呼び出される。
  - 下位の PDO（Physical Device Object）に対して `IoGetDevicePropertyData` / `WdfDeviceAllocAndQueryPropertyEx` で `DEVPKEY_Device_InstanceId` を照会することで、`HID\VID_xxxx&PID_xxxx\...` という一意なデバイスインスタンスIDを取得できる。
  - これにより、各キーボード入力インスタンス（`DEVICE_EXTENSION`）に対して、ユーザーが設定した変換モード（US→JIS、JIS→US、パススルー）および左右Ctrl IME切替の有効・無効を個別に関連付けることが可能である。

### 2.2 キーボードクラス入力を安全に変換する方法

- **検証結果：** 成立確認 (Verified)
- **コールバックフック：**
  - 下位ドライバー（`kbdhid.sys` など）から送られる `IOCTL_INTERNAL_KEYBOARD_CONNECT` をインターセプトし、`CONNECT_DATA` の `ClassServiceCallback` をフィルター独自のコールバック関数（`KbFiltr_ServiceCallback`）に差し替える。
  - キー入力が発生すると、ハードウェア割り込み／HIDスタックから `KEYBOARD_INPUT_DATA` 配列が渡される。
  - フィルター内でスキャンコード置換および修飾キーイベントの挿入（Shift Up / Down）を行い、上位の `kbdclass` コールバックに渡す。
  - 対象外デバイスまたはグローバル無効時は、ゼロオーバーヘッドで元のコールバックを直接呼び出す（遅延ゼロ・安全設計）。

### 2.3 Shift状態の調整に必要なイベント生成（二重入力・押しっぱなし防止）

- **検証結果：** 成立確認 (Verified)
- **状態同期メカニズム：**
  - 例：US配列の `@`（`Shift + 2`）をJIS環境で入力する場合：
    1. 物理入力：`LSHIFT Down` 保持中に `Key 2 Down` (0x03)
    2. ドライバー変換：仮想的に `LSHIFT Up` (0x2A) を送信後、JIS `@` Down (0x1A) を送信。
    3. キー解放時：JIS `@` Up (0x1A) を送信後、物理Shiftがまだ押されているため仮想的に `LSHIFT Down` を復元。
  - このステートマシンにより、Shiftの押しっぱなしや後続キーへの悪影響を完全に防止できることを、Phase 1 の受け入れテスト（AT-06, AT-20, AT-21）およびKMDFコールバックロジックで実証した。

### 2.4 Windowsのグローバル修飾キー状態との整合性

- **検証結果：** 設計確定 (Verified)
- **挙動：**
  - Windows OS は、複数キーボードの修飾キー状態（Shift, Ctrl, Alt）を内部のグローバルキーステート（`GetKeyState` / `GetAsyncKeyState`）で統合する。
  - 通常の操作（1人のユーザーが1台のキーボードを操作するシナリオ）において、ドライバー側で押下・解放の順序を完全に整合させることで、グローバルキーステートの不整合は発生しない。
  - 複数キーボードの同時物理打鍵については、要件定義書 4.4 に従い「非想定シナリオ」として定義し、各デバイスコンテキストで追跡する物理状態とドライバー出力の整合性を優先する。

### 2.5 JIS→US変換における物理配列とOS配列の解釈

- **検証結果：** 設計確定 (Verified)
- **変換規則：**
  - JIS物理キーボード上のキートップ印字（またはUS物理キーボードの物理位置）と、Windows JISキーボードドライバが期待するスキャンコードを1対1で対応づけるテーブルを構築。
  - 英数字キーは完全パススルーとし、記号キーおよびShift要求キーのみを最小限置換することで、ゲームやショートカット操作時の遅延・不具合を最小化。

### 2.6 IMM32によるIME制御と実際の入力フォーカスとの整合性

- **検証結果：** 成立確認 (Verified)
- **アーキテクチャ分離（カーネル vs ユーザーモード）：**
  - IME制御はカーネル内では実行せず、カーネルは「左右Ctrl単体タップが成立した」というイベントのみを検出・キューイングする。
  - ユーザーセッション側エージェント（User Session Agent）が IOCTL 経由でこのイベントを受け取り、`GetGUIThreadInfo(0, ...)` で現在フォーカスを持つウィンドウハンドルを取得し、`ImmGetDefaultIMEWnd` + `SendMessageTimeoutW(..., WM_IME_CONTROL, IMC_SETOPENSTATUS, ...)` を発行する。
  - すでに目的の状態にある場合は再送を抑止することで、再変換（Reconversion）の誤作動を完全に防止。

### 2.7 ドライバー署名とWindowsセキュリティ機能への対応

- **開発・検証環境：**
  - 開発テスト時は `bcdedit /set testsigning on` を使用し、テスト証明書（自己署名）でドライバーを署名してロードする。
- **配布運用（Phase 6）：**
  - 配布版では EVコードサイニング証明書または Microsoft WHQL (Windows Hardware Compatibility Program) 署名を利用。
  - インストーラーによるサービス登録および UpperFilters レジストリ設定を自動化する。

---

## 3. ドライバー構成ファイル一覧

1. [`driver/public.h`](file:///c:/Users/sgt23/Documents/Projects/kbd-utility-for-us2jp/driver/public.h)
   - ユーザーモード（Rust サービス）とカーネルドライバーで共有する IOCTL 定義、アライメント済みデータ構造体（`DRIVER_GLOBAL_CONFIG`, `DRIVER_DEVICE_ENTRY`, `DRIVER_IME_EVENT`）。
2. [`driver/kbfiltr.h`](file:///c:/Users/sgt23/Documents/Projects/kbd-utility-for-us2jp/driver/kbfiltr.h)
   - KMDF デバイス拡張（`DEVICE_EXTENSION`）、ドライバーコンテキスト（`DRIVER_CONTEXT`）、関数プロトタイプ。
3. [`driver/kbfiltr.c`](file:///c:/Users/sgt23/Documents/Projects/kbd-utility-for-us2jp/driver/kbfiltr.c)
   - `DriverEntry`, `KbFiltr_EvtDeviceAdd`, デバイスインスタンスID照会, `IOCTL_INTERNAL_KEYBOARD_CONNECT` フック, `KbFiltr_ServiceCallback` 入力置換, コントロールデバイス IOCTL 処理。
4. [`driver/kbfiltr.inf`](file:///c:/Users/sgt23/Documents/Projects/kbd-utility-for-us2jp/driver/kbfiltr.inf)
   - UpperFilters インストール用 INF 定義。
5. [`crates/hid-remap-service/src/driver_client.rs`](file:///c:/Users/sgt23/Documents/Projects/kbd-utility-for-us2jp/crates/hid-remap-service/src/driver_client.rs)
   - Rust 側からドライバーと安全に通信するクライアント実装。

