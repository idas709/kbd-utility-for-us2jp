Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$rtb = New-Object System.Windows.Forms.RichTextBox
$rtb.Font = New-Object System.Drawing.Font("Meiryo", 9.5)

# Japanese License
$jaText = @"
HID Keyboard Layout Remapper
ソフトウェア利用規約 (MIT License)

本ソフトウェアは MIT ライセンス (オープンソース・フリーソフトウェア) の下で提供されています。商用・非商用を問わず、誰でも無償で自由に使用、複製、改変、配布することができます。

--------------------------------------------------
MIT License

Copyright (c) 2026 HID Keyboard Layout Remapper Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
--------------------------------------------------
【日本語要約・免責事項】
1. 利用権限: 本ソフトウェアの複製、改変、再配布、商用利用等を無制限に許可します。
2. 著作権表示: 複製や配布を行う場合は、上記の著作権表示および許諾表示を保持してください。
3. 無保証・免責: 本ソフトウェアは「現状有姿」で提供され、動作保証や特定目的への適合性を含め一切の保証はありません。作者または著作権者は利用に伴ういかなる責任も負いません。
"@

$rtb.Text = $jaText
$rtb.SaveFile("installer\license-ja.rtf", [System.Windows.Forms.RichTextBoxStreamType]::RichText)

# English License
$enText = @"
HID Keyboard Layout Remapper
Software License Agreement (MIT License)

Copyright (c) 2026 HID Keyboard Layout Remapper Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"@

$rtb.Font = New-Object System.Drawing.Font("Segoe UI", 9.5)
$rtb.Text = $enText
$rtb.SaveFile("installer\license-en.rtf", [System.Windows.Forms.RichTextBoxStreamType]::RichText)

Copy-Item "installer\license-ja.rtf" "installer\license.rtf" -Force
Write-Host "License RTF files generated successfully."

