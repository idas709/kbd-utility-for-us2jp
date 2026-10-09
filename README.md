# HID Keyboard Layout Remapper

Windows 11 上で、HIDデバイスインスタンスIDにより物理キーボードを個別に識別し、**キーボード単位での US/JIS 配列変換** および **左右CtrlキーによるIME制御（左Ctrl: OFF / 右Ctrl: ON）** を実現する常駐型ユーティリティです。

Windows全体のレイアウト設定（JIS 106/109など）を変更することなく、接続された物理キーボードごとに異なる配列モードを適用できます。

---

## クイックスタート（単一スタンドアロン版）

**ドライバーのインストールやサービス登録なし**で、ダブルクリック一発ですぐに使える単一ポータブル版を提供しています：

```
dist\hid-remap.exe
```

- **インストール不要**: 好きなフォルダに置いて実行するだけで動作。
- **オールインワン**: GUI 設定画面、左右Ctrl IME制御、US/JIS 配列変換エンジンを1つの EXE に統合。
- **管理者権限不要**: 標準ユーザー権限で動作。
- **即時反映**: GUI で配列モードや IME 切替を変更すると、その場で即座に有効化。
- **バックグラウンド常駐**: 「Hide to Tray」ボタンで最小化して作業を継続可能。

---

## 主な機能

1. **デバイス別キーボード配列変換**
   - **US → JIS**: US物理キーボードを、WindowsのJIS環境でキートップ印字通りに入力できるよう変換（`@`, `^`, `&`, `*`, `(`, `)`, `=`, `+`, `_`, `[ ]`, `{ }`, `; :`, `' "`, `\`, `|`, `` ` ``, `~` など）。
   - **JIS → US**: JIS物理キーボードを、US配列相当のキー配置として入力。
   - **Passthrough**: 入力を変更せずそのまま透過。
   - **ショートカット保護**: Ctrl, Alt, Win を用いる通常のショートカット（Ctrl+C, Ctrl+V, Ctrl+[ 等）を文字変換から除外し完全保護。
   - **安全なShift同期**: 仮想Shiftの注入・解除ステートマシンにより、Shiftの押しっぱなしや重複入力を防止。

2. **左右Ctrl単体タップによるIME制御**
   - **左Ctrl単体タップ**: IME OFF（半角英数入力）
   - **右Ctrl単体タップ**: IME ON（日本語入力）
   - **誤動作防止**: Ctrl+C などのショートカット時や長押し（設定値、既定1000ms以上）時はIMEを切り替えない。
   - **再変換抑止**: すでに目的の状態にある場合は再送せず、意図しない再変換を防止。
   - **選べる方式**: IMM32 API メッセージ（既定）、VK_IME_ON/OFF、変換/無変換キー、Hybrid。

3. **モダン Slint GUI & トレイ・サービス**
   - Windows 11 に調和する Fluent スタイルの Slint GUI。
   - 接続中・登録済みのキーボード一覧、配列モード、Ctrl IME の個別トグルスイッチ。
   - Windows サービスによる設定永続化と、ユーザーセッションエージェントによる IME 連携。

---

## アーキテクチャ

システムは要件定義書に従い、以下の疎結合コンポーネントで構成されています：

```
+-------------------------------------------------------+
|                 Slint GUI Application                 |
|             (crates/hid-remap-gui)                   |
+---------------------------+---------------------------+
                            | 設定保存・更新
+---------------------------v---------------------------+
|               Rust Windows Service Daemon             |
|            (crates/hid-remap-service)                 |
+---------------------------+---------------------------+
                            | IOCTL
+---------------------------v---------------------------+
|              KMDF Keyboard Filter Driver              |
|                     (driver/)                         |
|  - デバイスインスタンスID別入力判定 (Kbdclass Filter) |
|  - スキャンコード・Shift状態変換                      |
|  - 左右Ctrl単体タップのキューイング                   |
+---------------------------+---------------------------+
                            | イベント通知 (IOCTL)
+---------------------------v---------------------------+
|                Rust User Session Agent                |
|             (crates/hid-remap-agent)                  |
|  - IMM32 API / SendInput による IME ON/OFF 実行      |
|  - (ドライバー未導入時のフックフォールバック内蔵)     |
+-------------------------------------------------------+
```

---

## ワークスペース構成

- [`crates/hid-remap-core`](crates/hid-remap-core): 状態付き変換エンジン、設定モデル、スキャンコードモデル、IMEタップトラッカー。
- [`crates/hid-remap-device`](crates/hid-remap-device): Windows Raw Input / PnP API による接続キーボードの列挙・同定。
- [`crates/hid-remap-ime`](crates/hid-remap-ime): IMM32 ウィンドウメッセージおよび仮想キーストロークによる IME 切り替え。
- [`crates/hid-remap-service`](crates/hid-remap-service): バックグラウンドサービスおよびドライバー通信クライアント。
- [`crates/hid-remap-agent`](crates/hid-remap-agent): ユーザーセッション内で動作する IME 制御常駐エージェント。
- [`crates/hid-remap-gui`](crates/hid-remap-gui): Slint ベースのモダンデスクトップ設定ツール。
- [`crates/hid-remap-cli`](crates/hid-remap-cli): デバイス列挙、変換シミュレーション、IMEテスト用 CLI ツール。
- [`driver/`](driver/): KMDF キーボードフィルタードライバー（Cソース、ヘッダー、INF）。
- [`installer/`](installer/): インストール、アンインストール、緊急復旧スクリプト。

---

## ビルド方法

### 必要環境
- Rust 1.80+ (stable-x86_64-pc-windows-msvc)
- Visual Studio Build Tools 2022 (C++ コンパイラ、Windows SDK 10/11)
- PowerShell 5.1 / 7+

### リリースビルド
ルートディレクトリで以下を実行すると、すべてのバイナリがコンパイルされ `dist/` にパッケージ化されます：

```powershell
powershell -ExecutionPolicy Bypass -File .\build-release.ps1
```

---

## 実行・インストール

### 1. MSI インストーラーによる導入（推奨）
[WiX Toolset](https://wixtoolset.org/) でビルドされた Windows 標準インストーラーを提供しています：

```
dist\hid-remap-setup.msi
```

- **全ユーザー / 現在のユーザーの選択可能**:
  - **「このコンピュータを使用するすべてのユーザー用にインストール」**（All Users: `Program Files`、要管理者権限）
  - **「現在のユーザーのみにインストール」**（Current User: `%LOCALAPPDATA%\Programs`、**管理者権限不要**）
- **Windows 起動時の自動起動（スタートアップ）**: セットアップ時に自動登録。
- **既存設定の完全保護**: バージョン更新時も `config.toml` の設定を安全に維持。
- **Windows「設定」からクリーンアンインストール**: スタートアップ登録やショートカットを含め完全消去。

### 2. ポータブル版（インストール不要）
USBメモリや任意のフォルダーからダブルクリックするだけで即座に動作します：

```
dist\hid-remap.exe
```

- トレイアイコン常駐型、設定画面はトレイアイコンまたは多重起動時に復元可能。
- 終了はシステムトレイの右クリックメニュー「終了」から安全に終了できます。

---

## 受け入れテスト

以下のすべてのテスト項目（AT-05 〜 AT-21）をユニットテストおよび結合テストで自動検証済みです：

```powershell
cargo test --workspace
```
35件のテストが全件パスします。


