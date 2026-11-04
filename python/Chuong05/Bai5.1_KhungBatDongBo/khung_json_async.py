"""Bài 5.1 – Các hàm gửi, nhận khung JSON bất đồng bộ (asyncio).

Cài đặt Mã giả 5.1 (Mục 5.2.4, Giáo trình Lập trình mạng).
Kế thừa: Mã giả 3.3 (SEND_ALL), 3.4 (RECV_EXACT), 3.6 (khung JSON), 2.1 (PARSE_ENDPOINT),
2.2 (SAFE_CONNECT) – giữ nguyên logic, thay lời gọi vào/ra bằng phiên bản có await.
Tín hiệu hủy ct: trong asyncio, việc hủy được thực hiện bằng task.cancel(); lỗi
asyncio.CancelledError phát sinh tại điểm await đang chờ, nên các hàm không cần tham số ct.

Tệp này được các chương trình trong cùng thư mục import.
"""
import asyncio
import json
import os
import socket
import struct
import sys

MAX_FRAME = 1048576                          # 1 MiB (Mã giả 3.6)


class ConnectionClosed(Exception):
    """Đối tác đóng kết nối; received = số byte của khung đã nhận được."""
    def __init__(self, received):
        super().__init__(f"Kết nối đã đóng (đã nhận {received} byte)")
        self.received = received


class ProtocolError(Exception):
    """Vi phạm giao thức, ví dụ khung quá lớn."""


class NetError(Exception):
    """Lỗi mạng đã phân loại (Mã giả 2.2): code, retry."""
    def __init__(self, code, retry, cause=None):
        super().__init__(f"{code}: {cause}" if cause else code)
        self.code, self.retry = code, retry


def json_bytes(obj):
    """ENCODE(JSON_ENCODE(obj), "UTF-8"): giữ nguyên chữ có dấu, không thêm khoảng trắng."""
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


async def recv_exact_async(reader, n):                     # Mã giả 5.1, dòng 1–10
    buf = b""
    while len(buf) < n:
        chunk = await reader.read(n - len(buf))            # AWAIT RECV_ASYNC: nhường vòng lặp sự kiện
        if len(chunk) == 0:
            raise ConnectionClosed(len(buf))               # đối tác đã đóng kết nối
        buf += chunk
    return buf


async def recv_json_async(reader):                         # Mã giả 5.1, dòng 12–20
    n = struct.unpack("!I", await recv_exact_async(reader, 4))[0]   # FROM_UINT32_BE
    if n > MAX_FRAME:
        raise ProtocolError("Khung quá lớn")
    try:
        data = await recv_exact_async(reader, n)
    except ConnectionClosed as e:                          # đã nhận xong tiêu đề
        raise ConnectionClosed(4 + e.received) from None
    return json.loads(data.decode("utf-8"))


def make_frame(obj):
    """UINT32_BE(LENGTH(p)) + p."""
    p = json_bytes(obj)
    return struct.pack("!I", len(p)) + p


async def send_json_async(writer, obj):                    # Mã giả 5.1, dòng 22–24
    writer.write(make_frame(obj))                          # SEND_ALL_ASYNC = write + drain
    await writer.drain()                                   # chờ bộ đệm gửi giải phóng


def parse_endpoint(args, default_host, default_port):
    """Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định."""
    host, port = default_host, os.environ.get("APP_PORT", default_port)
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


async def safe_connect_async(host, port, timeout_ms):
    """SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT; trả về (reader, writer)."""
    loop = asyncio.get_running_loop()
    try:
        addrs = await loop.getaddrinfo(host, port, type=socket.SOCK_STREAM)   # RESOLVE
    except socket.gaierror as e:
        raise NetError("NOT_FOUND", retry=False, cause=e)
    refused, last_error = 0, None
    for _, _, _, _, addr in addrs:
        try:
            return await asyncio.wait_for(asyncio.open_connection(addr[0], addr[1]), timeout_ms / 1000)
        except ConnectionRefusedError as e:
            refused, last_error = refused + 1, e
        except (TimeoutError, OSError) as e:                # hết giờ, mạng không tới được
            last_error = e
    if refused > 0 and refused == len(addrs):
        raise NetError("REFUSED", retry=False, cause=last_error)
    raise NetError("UNREACHABLE", retry=True, cause=last_error)
