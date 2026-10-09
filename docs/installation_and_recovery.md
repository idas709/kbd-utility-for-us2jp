# インストール・署名・障害復旧ガイド

本書では、HID Keyboard Layout Remapper のドライバー署名、インストール、アンインストール、および緊急時の復旧手順について解説します。

---

## 1. ドライバーテスト署名の手順（開発・検証用）

Windows 64-bit では、カーネルドライバーをロードするためにデジタル署名が必須です。  
開発およびテスト環境では、以下の手順でテスト署名モードを有効化し、自己署名証明書でドライバーに署名します。

### 1.1 テスト署名モードの有効化（要管理者権限・再起動）

管理者権限の PowerShell / コマンドプロンプトで実行します：

```powershell
bcdedit /set testsigning on
```

PC を再起動すると、デスクトップ右下に「テスト モード」と表示されます。

### 1.2 自己署名証明書の作成と信頼されたストアへの追加

```powershell
# 自己署名コード署名証明書の生成
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=HID Remapper Driver Test Cert" -CertStoreLocation "Cert:\LocalMachine\My"

# 信頼されたルート証明機関 (Root) および信頼された発行元 (TrustedPublisher) に追加
$rootStore = [System.Security.Cryptography.X509Certificates.X509Store]::new("Root", "LocalMachine")
$rootStore.Open("ReadWrite")
$rootStore.Add($cert)
$rootStore.Close()

$pubStore = [System.Security.Cryptography.X509Certificates.X509Store]::new("TrustedPublisher", "LocalMachine")
$pubStore.Open("ReadWrite")
$pubStore.Add($cert)
$pubStore.Close()
```

### 1.3 ドライバーバイナリへの署名

WDK / Windows SDK の `signtool.exe` を使用して署名します：

```powershell
signtool.exe sign /v /s "My" /n "HID Remapper Driver Test Cert" /fd SHA256 /t http://timestamp.digicert.com dist\driver\kbfiltr.sys
```

---

## 2. インストール手順

管理者権限の PowerShell で以下のスクリプトを実行します：

```powershell
cd c:\Users\sgt23\Documents\Projects\kbd-utility-for-us2jp
powershell -ExecutionPolicy Bypass -File .\installer\install.ps1
```

スクリプトが自動的に以下を実施します：
1. `C:\Program Files\HID Keyboard Remapper\` にバイナリと設定ファイルを配置
2. Windows サービス `HidKeyboardRemapper` を登録・自動起動設定
3. ユーザーセッションエージェント（`hid-remap-agent.exe`）をスタートアップに登録・起動
4. 設定ファイル（`config.toml`）を初期化

---

## 3. 通常アンインストール

アンインストールする場合も、管理者権限の PowerShell で以下を実行します：

```powershell
powershell -ExecutionPolicy Bypass -File .\installer\uninstall.ps1
```

サービス、常駐エージェント、およびレジストリ内の `UpperFilters` 設定が安全に削除され、OS 標準のキーボードドライバー構成に即座に戻ります。

---

## 4. 緊急復旧（Emergency Recovery: AT-19, AT-23）

万が一、ドライバーの不具合や強制終了等によりキーボード入力が停止・応答しなくなった場合、以下のいずれかの方法で安全に復旧できます。

### 方法 A：緊急復旧スクリプトの実行

管理者権限の PowerShell で以下を実行します：

```powershell
powershell -ExecutionPolicy Bypass -File .\installer\emergency-recover.ps1
```

このスクリプトは、実行中の関連プロセス・サービスを強制停止し、キーボードクラス（`{4d36e96b-e325-11ce-bfc1-08002be10318}`）の `UpperFilters` から `kbfiltr` を確実に削除して既定値に復旧します。

### 方法 B：セーフモードまたは回復環境からのレジストリ復旧

Windows が起動しない、または入力が完全に効かない場合：
1. PC を再起動し、Shift キーを押しながら再起動して「セーフ モード」に入る。
2. レジストリエディター（`regedit.exe`）を起動。
3. `HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e96b-e325-11ce-bfc1-08002be10318}` を開く。
4. `UpperFilters` 値から `kbfiltr` を削除する（通常は `kbdclass` または空）。
5. PC を通常再起動する。
