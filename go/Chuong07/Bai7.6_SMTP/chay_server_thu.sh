#!/bin/sh
# Bài 7.6 – Khởi động server SMTP thử nghiệm (tạo chứng chỉ nếu chưa có, biên dịch nếu chưa có out/app).
# Dùng: sh chay_server_thu.sh [host] [port]
cd "$(dirname "$0")"
[ -f cert.pem ] && [ -f key.pem ] || sh tao_chung_chi.sh
[ -x out/app ] || go build -o out/app .
exec out/app server "${1:-127.0.0.1}" "${2:-587}"
