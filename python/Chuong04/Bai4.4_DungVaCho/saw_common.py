"""Bài 4.4 – Hằng số và tiện ích dùng chung cho bên gửi, bên nhận dừng và chờ (Mã giả 4.6, 4.7)."""
import hashlib
import os
import random
import sys

DATA, ACK, FIN = 0, 1, 2                  # byte loại trong tiêu đề 5 byte (Mã giả 4.6, dòng 1)
MAX_TRIES = 8
MAX_DGRAM = 1200
CHUNK = MAX_DGRAM - 5                     # khối dữ liệu ≤ 1195 byte
RTO_INIT, RTO_MIN = 200, 100              # ms; giá trị cho mạng nội bộ (Mục 4.4.2), không phải 1 s của TCP
MAX_RTO = 60000                           # ms; Mã giả 4.6 dòng 1 – trần của RTO (60 s, RFC 6298)
# Ghi đè CHỈ để chạy demo nhanh: biến môi trường SAW_MAX_RTO (ms). Không đặt biến này thì MAX_RTO = 60000
# như giáo trình, và bên nhận chờ 2 × MAX_RTO = 120 s sau FIN.
if os.environ.get("SAW_MAX_RTO"):
    MAX_RTO = int(os.environ["SAW_MAX_RTO"])


def max_rto_note():
    """Dòng nhật ký cho biết MAX_RTO đang dùng (để thấy rõ khi giá trị bị ghi đè cho demo)."""
    if os.environ.get("SAW_MAX_RTO"):
        return f"MAX_RTO = {MAX_RTO} ms (ghi đè cho demo bằng biến môi trường SAW_MAX_RTO; mặc định 60000 ms)"
    return f"MAX_RTO = {MAX_RTO} ms"


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định."""
    host = os.environ.get("APP_HOST", default_host)
    port = os.environ.get("APP_PORT", default_port)
    if len(args) >= 1:
        host = args[0]
    if len(args) >= 2:
        port = args[1]
    try:
        port = int(port)
    except ValueError:
        sys.exit(f"Cổng phải là số nguyên: {port}")
    if not 1 <= port <= 65535:
        sys.exit(f"Cổng không hợp lệ: {port}")
    return host, port


class LossSim:
    """Mô phỏng mất gói ở chiều gửi đi (thêm cho bài thực hành, không có trong mã giả).
    spec = ""      : không mất gói;
    spec = "0.1"   : mỗi datagram gửi đi mất ngẫu nhiên với xác suất 0,1;
    spec = "3,7"   : mất đúng datagram gửi đi thứ 3 và thứ 7 (đếm từ 1, kể cả lần gửi lại)."""

    def __init__(self, spec):
        self.spec, self.count = spec or "", 0
        self.rate = float(self.spec) if "." in self.spec else 0.0
        self.picks = set() if "." in self.spec else {int(x) for x in self.spec.split(",") if x}

    def drop(self):
        self.count += 1
        return self.count in self.picks or random.random() < self.rate


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(65536), b""):
            h.update(block)
    return h.hexdigest()
