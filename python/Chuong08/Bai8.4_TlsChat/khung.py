"""Bài 8.4 – Gửi, nhận khung JSON bất đồng bộ (chép gọn Mã giả 5.1, kế thừa Mã giả 3.6).

Khung = 4 byte độ dài (big-endian) + JSON UTF-8. Logic không đổi khi chạy trên TLS:
StreamReader/StreamWriter sau start_tls đọc ghi giống trên socket gốc (Mục 8.4.5).
"""
import json
import struct

MAX_FRAME = 1048576                                           # 1 MiB


class ProtocolError(Exception):
    pass


async def recv_json_async(reader):
    """RECV_JSON_ASYNC: readexactly = RECV_EXACT_ASYNC; kết nối đóng -> asyncio.IncompleteReadError."""
    n = struct.unpack("!I", await reader.readexactly(4))[0]  # FROM_UINT32_BE
    if n > MAX_FRAME:
        raise ProtocolError("Khung quá lớn")
    data = await reader.readexactly(n)
    return json.loads(data.decode("utf-8"))


async def send_json_async(writer, obj):
    """SEND_JSON_ASYNC: ghép tiêu đề độ dài với nội dung rồi gửi; drain chờ bộ đệm gửi."""
    p = json.dumps(obj, ensure_ascii=False).encode("utf-8")
    writer.write(struct.pack("!I", len(p)) + p)
    await writer.drain()
