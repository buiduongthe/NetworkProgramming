# Chạy kịch bản demo.json trên Windows – nay do tools\demo_windows.py đảm nhiệm (xem đầu tệp đó).
# Tệp này giữ lại để các lệnh cũ trong README vẫn chạy được:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\demo_windows.ps1 csharp\Chuong06\BaiMR6.6_DauGia\GiaoDien
# tương đương:  py -3 tools\demo_windows.py csharp\Chuong06\BaiMR6.6_DauGia\GiaoDien
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args2)
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$tool = Join-Path $PSScriptRoot 'demo_windows.py'
if (Get-Command py -ErrorAction SilentlyContinue) { & py -3 $tool @Args2 }
elseif (Get-Command python -ErrorAction SilentlyContinue) { & python $tool @Args2 }
else { Write-Host 'Thiếu Python 3: winget install -e --id Python.Python.3.13'; exit 1 }
exit $LASTEXITCODE
