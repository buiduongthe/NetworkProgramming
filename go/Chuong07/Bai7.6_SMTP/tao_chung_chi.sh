#!/bin/sh
# Bài 7.6 – Tạo chứng chỉ tự ký cho server SMTP thử nghiệm (chỉ dùng trong phòng thí nghiệm).
# Tạo cert.pem (chứng chỉ, client dùng làm nơi tin cậy) và key.pem (khóa riêng, chỉ server dùng).
# Dùng: sh tao_chung_chi.sh
openssl req -x509 -newkey rsa:2048 -nodes -days 30 -keyout key.pem -out cert.pem \
  -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" 2>/dev/null \
  && echo "[chứng chỉ] Đã tạo cert.pem, key.pem (CN=localhost, SAN: localhost, 127.0.0.1)"
