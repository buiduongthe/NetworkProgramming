#!/usr/bin/env bash
# Bài 8.4 – Tạo CA nội bộ của phòng lab và chứng chỉ cho server chat (Mục 8.4.5 Giáo trình Lập trình mạng)
# Kết quả trong thư mục certs/: ca.crt (client cần tin cậy), chat.crt + chat.key (PEM, cho Python),
# chat.p12 (PKCS12 gộp chứng chỉ và khóa riêng, cho C# và Java; mật khẩu lấy từ biến TLS_KEY_PASS).
# Chạy:  TLS_KEY_PASS=<mật khẩu> bash tao_chung_chi.sh
# Lưu ý: certs/ chứa khóa riêng – không đưa vào kho mã (đã có trong .gitignore).
set -euo pipefail
: "${TLS_KEY_PASS:?Cần đặt biến môi trường TLS_KEY_PASS (mật khẩu bảo vệ tệp chat.p12)}"
mkdir -p certs
cd certs
# Tạo CA nội bộ của phòng lab (khóa và chứng chỉ tự ký)
# (thêm keyUsage cho CA: Python ≥ 3.13 kiểm tra chặt chẽ X.509 và từ chối CA thiếu trường này)
openssl req -x509 -newkey rsa:3072 -nodes -days 365 \
    -keyout ca.key -out ca.crt -subj "/CN=Lab CA" \
    -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
# Tạo khóa và yêu cầu cấp chứng chỉ cho server chat
openssl req -newkey rsa:2048 -nodes -keyout chat.key \
    -out chat.csr -subj "/CN=chat.lab.local" 2>/dev/null
# CA ký chứng chỉ server, kèm tên trong SAN (thêm localhost và 127.0.0.1 để chạy thử trên một máy)
openssl x509 -req -in chat.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -days 90 -out chat.crt \
    -extfile <(printf "subjectAltName=DNS:chat.lab.local,DNS:localhost,IP:127.0.0.1\nauthorityKeyIdentifier=keyid,issuer\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth") 2>/dev/null
# Gộp chứng chỉ và khóa riêng thành tệp PKCS12 (PFX) cho .NET và Java
openssl pkcs12 -export -in chat.crt -inkey chat.key -certfile ca.crt \
    -name chat -out chat.p12 -passout env:TLS_KEY_PASS
rm -f chat.csr ca.srl
echo "[cert] Đã tạo certs/ca.crt, certs/chat.crt, certs/chat.key, certs/chat.p12"
openssl x509 -in chat.crt -noout -subject -issuer -ext subjectAltName | sed 's/^/[cert]   /'
