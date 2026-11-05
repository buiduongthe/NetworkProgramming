# Chạy demo.json của các bài trên Windows (bản PowerShell rút gọn của tools/demo.py), dùng cho các bài
# cần Windows hoặc cần tải thư viện (NuGet, Maven, Go modules) mà môi trường biên soạn không có.
# Mỗi tiến trình ghi nhật ký vào <thư mục bài>\.log_<tên>.txt; ảnh ket_qua.png được dựng sau từ nhật ký
# bằng tools/render_output.py.
# Cách chạy (từ thư mục gốc kho mã):
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\demo_windows.ps1                 (các bài mặc định)
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\demo_windows.ps1 go\Chuong07\Bai7.5_gRPC
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Lessons)
if (-not $Lessons) {
    $Lessons = @(
        'csharp\Chuong05\Bai5.3_GiaoDienBatDongBo',
        'csharp\Chuong07\Bai7.5_gRPC',
        'java\Chuong07\Bai7.5_gRPC',
        'go\Chuong07\Bai7.5_gRPC')
}
$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$root = Split-Path -Parent $PSScriptRoot

# Gợi ý cài đặt khi thiếu công cụ (winget có sẵn trên Windows 10/11)
$hints = @{
    'dotnet' = 'winget install Microsoft.DotNet.SDK.10'
    'mvn'    = 'cần JDK trước: winget install EclipseAdoptium.Temurin.21.JDK (script tự tải Maven khi đã có Java)'
    'java'   = 'winget install EclipseAdoptium.Temurin.21.JDK'
    'go'     = 'winget install GoLang.Go'
    'protoc' = 'winget install Google.Protobuf'
}

function Quote([string]$a) { if ($a -match '[\s"]') { '"' + ($a -replace '"', '\"') + '"' } else { $a } }
function WinArg([string]$a) { if ($a -eq 'out/app') { 'out\app.exe' } else { $a } }   # Go: tệp chạy cần đuôi .exe
function CmdLine($arr) { ($arr | ForEach-Object { Quote (WinArg $_) }) -join ' ' }

function Start-Logged($line, $log, $dir) {
    # chcp 65001: chương trình ghi UTF-8 khi đầu ra được chuyển hướng vào tệp
    $full = "chcp 65001 >nul & $line > `"$log`" 2>&1"
    Start-Process -FilePath cmd.exe -ArgumentList '/d', '/c', $full -WorkingDirectory $dir -NoNewWindow -PassThru
}
function Stop-Tree($p) { if ($p -and -not $p.HasExited) { taskkill /T /F /PID $p.Id *> $null } }

# Maven không có trong winget: nếu đã có Java mà thiếu mvn, tải Apache Maven từ trang lưu trữ chính thức của
# Apache, kiểm tra SHA-512 rồi giải nén vào %LOCALAPPDATA%\NetworkProgramming (chỉ dùng cho phiên chạy này)
function Ensure-Maven {
    if (Get-Command mvn -ErrorAction SilentlyContinue) { return }
    if (-not (Get-Command java -ErrorAction SilentlyContinue)) { return }
    $ver  = '3.9.9'
    $base = Join-Path $env:LOCALAPPDATA 'NetworkProgramming'
    $mvnHome = Join-Path $base "apache-maven-$ver"
    if (-not (Test-Path (Join-Path $mvnHome 'bin\mvn.cmd'))) {
        New-Item -ItemType Directory -Force $base | Out-Null
        $url = "https://archive.apache.org/dist/maven/maven-3/$ver/binaries/apache-maven-$ver-bin.zip"
        $zip = Join-Path $base "apache-maven-$ver-bin.zip"
        Write-Host "  Tải Maven $ver từ $url"
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing $url -OutFile $zip
        $expected = ((Invoke-WebRequest -UseBasicParsing "$url.sha512").Content -split '\s+')[0].Trim().ToLower()
        $actual = (Get-FileHash $zip -Algorithm SHA512).Hash.ToLower()
        if ($expected -ne $actual) { Write-Host '  SHA-512 của Maven không khớp – dừng'; Remove-Item $zip; return }
        Expand-Archive $zip -DestinationPath $base -Force
        Remove-Item $zip
    }
    $env:PATH = "$(Join-Path $mvnHome 'bin');$env:PATH"
    Write-Host "  Dùng Maven tại $mvnHome"
}
Ensure-Maven

# Bài Go gRPC cần plugin protoc-gen-go, protoc-gen-go-grpc trong GOPATH\bin
if (Get-Command go -ErrorAction SilentlyContinue) {
    $gobin = Join-Path (go env GOPATH) 'bin'
    $env:PATH = "$gobin;$env:PATH"
}

$summary = @()
foreach ($rel in $Lessons) {
    $dir = Join-Path $root $rel
    Write-Host ''
    Write-Host "================ $rel"
    $spec = Get-Content (Join-Path $dir 'demo.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    Get-ChildItem $dir -Filter '.log_*.txt' -Force | Remove-Item -Force
    $tools = @($spec.setup | ForEach-Object { $_[0] }) + @($spec.processes | ForEach-Object { $_.cmd[0] }) |
             Where-Object { $_ -notlike 'out/*' -and $_ -notlike 'bin*' } | Sort-Object -Unique
    $missing = $tools | Where-Object { -not (Get-Command $_ -ErrorAction SilentlyContinue) }
    if ($missing) {
        foreach ($m in $missing) { Write-Host "  Thiếu công cụ '$m'. Cài: $($hints[$m])" }
        $summary += "$rel : BỎ QUA (thiếu $($missing -join ', '))"; continue
    }
    if ($rel -like 'go\*' -and (Select-String -Path (Join-Path $dir 'demo.json') -Pattern 'go-grpc_out' -Quiet)) {
        if (-not (Get-Command protoc-gen-go -ErrorAction SilentlyContinue)) { go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.35.2 }
        if (-not (Get-Command protoc-gen-go-grpc -ErrorAction SilentlyContinue)) { go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.5.1 }
    }
    if ($spec.env) { $spec.env.PSObject.Properties | ForEach-Object { Set-Item "env:$($_.Name)" $_.Value } }

    # protoc (gọi qua protobuf-maven-plugin) không đọc được đường dẫn có ký tự Unicode như "Lập trình mạng":
    # với bài khai báo "ascii_path": true, chép bài sang thư mục tạm có đường dẫn ASCII rồi chạy ở đó
    $srcDir = $dir
    if ($spec.ascii_path -and ($dir -match '[^\x00-\x7F]')) {
        $dir = Join-Path $env:TEMP ('NetworkProgramming\' + ($rel -replace '[\\/]', '_'))
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
        New-Item -ItemType Directory -Force $dir | Out-Null
        Get-ChildItem $srcDir -Force | Where-Object { $_.Name -notin @('target', 'out', 'bin', 'obj') -and $_.Name -notlike '.log_*' } |
            Copy-Item -Destination $dir -Recurse -Force
        Write-Host "  Đường dẫn có ký tự Unicode – chạy tại bản chép $dir"
    }

    $ok = $true
    foreach ($s in $spec.setup) {
        $line = CmdLine $s
        Write-Host "  [setup] $line"
        cmd /d /c "chcp 65001 >nul & cd /d `"$dir`" & $line >> .log_setup.txt 2>&1"
        if ($LASTEXITCODE -ne 0) {
            if ($dir -ne $srcDir) { Copy-Item (Join-Path $dir '.log_setup.txt') $srcDir -Force }
            Write-Host "  Lỗi ở bước setup – xem $srcDir\.log_setup.txt"; $ok = $false; break
        }
    }
    if (-not $ok) { $summary += "$rel : LỖI SETUP"; continue }

    $bg = @()
    try {
        foreach ($p in $spec.processes) {
            $line = CmdLine $p.cmd
            $log = Join-Path $dir ".log_$($p.name).txt"
            Write-Host "  [$($p.name)] $line"
            $proc = Start-Logged $line $log $dir
            if ($p.background) { $bg += $proc }
            else {
                $t = if ($p.timeout) { [int]$p.timeout } else { 60 }
                if (-not $proc.WaitForExit($t * 1000)) { Write-Host "    quá $t s – dừng"; Stop-Tree $proc }
            }
            if ($p.wait) { Start-Sleep -Milliseconds ([int](1000 * [double]$p.wait)) }
        }
    } finally {
        Start-Sleep -Seconds 1
        foreach ($b in $bg) { Stop-Tree $b }
    }
    if ($dir -ne $srcDir) {                                   # chép nhật ký, ảnh về thư mục bài
        Get-ChildItem $dir -Force | Where-Object { $_.Name -like '.log_*' -or $_.Name -like 'ket_qua*' } |
            Copy-Item -Destination $srcDir -Force
        $dir = $srcDir
    }
    Get-ChildItem $dir -Filter '.log_*.txt' -Force | ForEach-Object {
        Write-Host "  --- $($_.Name)"; Get-Content $_.FullName -Encoding UTF8 -TotalCount 40 | ForEach-Object { Write-Host "    $_" }
    }
    $summary += "$rel : ĐÃ CHẠY"
}
Write-Host ''
Write-Host '================ Tóm tắt'
$summary | ForEach-Object { Write-Host "  $_" }
