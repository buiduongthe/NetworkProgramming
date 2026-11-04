#!/bin/sh
# Bài 7.6 (Rust) – Tạo CA nội bộ và chứng chỉ cho server SMTP thử nghiệm (chỉ dùng trong phòng thí nghiệm).
# rustls (webpki) không chấp nhận chứng chỉ tự ký có CA:TRUE làm chứng chỉ server (lỗi CaUsedAsEndEntity),
# nên khác bản Python: tạo ca.pem (CA, client dùng làm nơi tin cậy) rồi dùng CA ký cert.pem + key.pem
# (chứng chỉ và khóa riêng của server, CN và SAN: localhost, 127.0.0.1). Tên tệp server giữ nguyên.
# Dùng: sh tao_chung_chi.sh
set -e
openssl req -x509 -newkey rsa:2048 -nodes -days 30 -keyout ca.key -out ca.pem -subj "/CN=Lab CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -keyout key.pem -out server.csr -subj "/CN=localhost" 2>/dev/null
printf "subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n" > server.ext
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 30 -out cert.pem -extfile server.ext 2>/dev/null
rm -f server.csr server.ext ca.srl ca.key
echo "[chứng chỉ] Đã tạo ca.pem (CA), cert.pem, key.pem (CN=localhost, SAN: localhost, 127.0.0.1)"
