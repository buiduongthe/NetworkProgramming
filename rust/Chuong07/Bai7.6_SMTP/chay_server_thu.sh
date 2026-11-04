#!/bin/sh
# Bài 7.6 (Rust) – Khởi động server SMTP thử nghiệm (tạo chứng chỉ nếu chưa có). Dùng: sh chay_server_thu.sh [host] [port]
cd "$(dirname "$0")"
[ -f ca.pem ] && [ -f cert.pem ] && [ -f key.pem ] || sh tao_chung_chi.sh
exec python3 smtp_server_thu.py "${1:-127.0.0.1}" "${2:-587}"
