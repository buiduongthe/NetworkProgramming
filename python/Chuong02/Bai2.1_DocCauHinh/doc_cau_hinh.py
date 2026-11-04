"""Bài 2.1 – Đọc địa chỉ và cổng từ tham số dòng lệnh, biến môi trường.

Cài đặt Mã giả 2.1 PARSE_ENDPOINT (Mục 2.1.3, Giáo trình Lập trình mạng).
Thứ tự ưu tiên: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định.
Chương trình chỉ in cấu hình nhận được, không mở socket.

Chạy:  python3 doc_cau_hinh.py [host] [port]        (mặc định 0.0.0.0 9000)
Ví dụ: APP_PORT=9100 python3 doc_cau_hinh.py       (Linux/macOS)
       set APP_PORT=9100 && python doc_cau_hinh.py (Windows cmd)
"""
import os
import sys


class ConfigError(Exception):
    """Lỗi cấu hình: giá trị địa chỉ hoặc cổng không dùng được."""


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1 – PARSE_ENDPOINT(args, default_host, default_port)."""
    host = default_host                                  # dòng 2
    port = default_port                                  # dòng 3
    if os.environ.get("APP_HOST") is not None:           # dòng 4: biến môi trường ghi đè mặc định
        host = os.environ["APP_HOST"]
    if os.environ.get("APP_PORT") is not None:           # dòng 5
        port = os.environ["APP_PORT"]
    if len(args) >= 1:                                   # dòng 6: tham số dòng lệnh ghi đè tất cả
        host = args[0]
    if len(args) >= 2:                                   # dòng 7
        port = args[1]
    try:                                                 # dòng 8–12: TO_INT
        port = int(port)                                 # "abc" → ValueError (FormatError)
    except ValueError:
        raise ConfigError(f"Cổng phải là số nguyên: {port}") from None
    if port < 1 or port > 65535:                         # dòng 13–15: cổng là số 16 bit, khác 0
        raise ConfigError(f"Cổng không hợp lệ: {port}")
    return host, port                                    # dòng 16


def main():
    args = sys.argv[1:]
    # In dữ liệu vào để thấy rõ giá trị nào thắng
    print(f"[cấu hình] APP_HOST={os.environ.get('APP_HOST', '(không đặt)')}, "
          f"APP_PORT={os.environ.get('APP_PORT', '(không đặt)')}, "
          f"tham số: {' '.join(args) if args else '(không có)'}", flush=True)
    try:
        host, port = parse_endpoint(args, "0.0.0.0", 9000)
    except ConfigError as e:
        print(f"[cấu hình] ConfigError: {e}", flush=True)
        sys.exit(2)
    print(f"[cấu hình] Kết quả: host={host}, port={port}", flush=True)


if __name__ == "__main__":
    main()
