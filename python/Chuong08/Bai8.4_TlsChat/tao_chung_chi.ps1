# Bài 8.4 – Tạo CA nội bộ của phòng lab và chứng chỉ cho server chat (Mục 8.4.5 Giáo trình Lập trình mạng)
# Bản PowerShell của tao_chung_chi.sh; cần openssl trong PATH (ví dụ openssl đi kèm Git for Windows).
# Kết quả trong certs\: ca.crt, chat.crt, chat.key (PEM, cho Python), chat.p12 (PKCS12, cho C# và Java).
# Chạy:  $env:TLS_KEY_PASS = "<mật khẩu>"; powershell -ExecutionPolicy Bypass -File tao_chung_chi.ps1
# Lưu ý: certs\ chứa khóa riêng – không đưa vào kho mã (đã có trong .gitignore).
if (-not $env:TLS_KEY_PASS) { Write-Error "Cần đặt biến môi trường TLS_KEY_PASS (mật khẩu bảo vệ tệp chat.p12)"; exit 1 }
function Run-OpenSsl { & openssl @args; if ($LASTEXITCODE -ne 0) { throw "openssl lỗi: $args" } }
New-Item -ItemType Directory -Force certs | Out-Null
Push-Location certs
try {
    # Tạo CA nội bộ (khóa và chứng chỉ tự ký); keyUsage cần cho Python >= 3.13 (kiểm tra X.509 chặt chẽ)
    Run-OpenSsl req -x509 -newkey rsa:3072 -nodes -days 365 -keyout ca.key -out ca.crt -subj "/CN=Lab CA" `
        -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
    # Tạo khóa và yêu cầu cấp chứng chỉ cho server chat
    Run-OpenSsl req -newkey rsa:2048 -nodes -keyout chat.key -out chat.csr -subj "/CN=chat.lab.local"
    # CA ký chứng chỉ server, kèm tên trong SAN (thêm localhost và 127.0.0.1 để chạy thử trên một máy)
    Set-Content -Path chat.ext -Encoding ascii -Value @(
        "subjectAltName=DNS:chat.lab.local,DNS:localhost,IP:127.0.0.1",
        "authorityKeyIdentifier=keyid,issuer", "basicConstraints=CA:FALSE",
        "keyUsage=digitalSignature,keyEncipherment", "extendedKeyUsage=serverAuth")
    Run-OpenSsl x509 -req -in chat.csr -CA ca.crt -CAkey ca.key -CAcreateserial -days 90 -out chat.crt -extfile chat.ext
    # Gộp chứng chỉ và khóa riêng thành tệp PKCS12 (PFX) cho .NET và Java
    Run-OpenSsl pkcs12 -export -in chat.crt -inkey chat.key -certfile ca.crt -name chat -out chat.p12 -passout env:TLS_KEY_PASS
    Remove-Item chat.csr, ca.srl, chat.ext -ErrorAction SilentlyContinue
    Write-Output "[cert] Đã tạo certs\ca.crt, certs\chat.crt, certs\chat.key, certs\chat.p12"
} finally {
    Pop-Location
}
