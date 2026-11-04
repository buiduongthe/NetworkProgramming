"""Bài 6.7 – Các hàm gửi, nhận khung JSON bất đồng bộ, chép gọn từ Mã giả 5.1 (Mục 5.2.4).

Trong asyncio, StreamReader.readexactly(n) thực hiện RECV_EXACT_ASYNC; việc gửi gồm
writer.write(data) và await writer.drain() (chờ bộ đệm gửi giải phóng).
"""
import asyncio
import json
import struct

MAX_FRAME = 1048576                                     # Mã giả 3.6: 1 MiB


class ConnectionClosed(Exception):
    """Đối tác đóng kết nối; received = số byte của khung đang nhận dở."""
    def __init__(self, received):
        super().__init__(f"Kết nối bị đóng sau {received} byte")
        self.received = received


class ProtocolError(Exception):
    """Vi phạm định dạng khung (ví dụ khung quá lớn)."""


async def recv_exact_async(reader, n):
    """RECV_EXACT_ASYNC: nhận đủ n byte; đóng giữa chừng → ConnectionClosed(số byte đã nhận)."""
    try:
        return await reader.readexactly(n)
    except asyncio.IncompleteReadError as e:
        raise ConnectionClosed(len(e.partial))


async def recv_json_async(reader):
    """RECV_JSON_ASYNC (Mã giả 5.1, dòng 12–20). JSON sai gây ValueError."""
    (n,) = struct.unpack("!I", await recv_exact_async(reader, 4))
    if n > MAX_FRAME:
        raise ProtocolError("Khung quá lớn")
    try:
        data = await recv_exact_async(reader, n)
    except ConnectionClosed as e:                       # đã nhận xong tiêu đề
        raise ConnectionClosed(4 + e.received)
    obj = json.loads(data.decode("utf-8"))
    if not isinstance(obj, dict):
        raise ValueError("Thông điệp không phải đối tượng JSON")
    return obj


async def send_json_async(writer, obj):
    """SEND_JSON_ASYNC (Mã giả 5.1, dòng 22–24): ghép 4 byte độ dài rồi gửi cả khung."""
    p = json.dumps(obj, ensure_ascii=False).encode("utf-8")
    writer.write(struct.pack("!I", len(p)) + p)         # SEND_ALL_ASYNC = write + drain
    await writer.drain()
