"""Bài mở rộng MR6.4 – Giao thức trò chơi Caro: khung JSON có tiền tố độ dài (chép từ MR3.3, Bài 3.2).

Khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8. Mỗi thông điệp có trường "type":
  client -> server: hello {name}, list, create {room}, join {room}, watch {room}, move {r, c},
                    rematch, leave, bye
  server -> client: welcome, rooms, joined, start, board, moved, over, rematch_wait, left,
                    room_closed, error {code, msg}
Khám phá server trong LAN (UDP broadcast, như Bài 6.5):
  client phát {"type": "discover", "service": "caro", "id": n}
  server trả unicast {"id": n, "name", "port", "rooms", "players"}
"""
import json
import random
import socket
import struct
import time

MAX_FRAME = 64 * 1024          # thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ


class ConnectionClosed(Exception):
    """Đối tác đóng kết nối."""


class ProtocolError(Exception):
    """Đối tác vi phạm định dạng khung (khung quá lớn...)."""


def recv_exact(s, n):
    """Mã giả 3.4 (RECV_EXACT): nhận đúng n byte."""
    buf = bytearray()
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise ConnectionClosed()
        buf += chunk
    return bytes(buf)


def send_frame(s, payload):
    """Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi hết bằng sendall (SEND_ALL)."""
    s.sendall(struct.pack("!I", len(payload)) + payload)


def recv_frame(s):
    """Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte."""
    (n,) = struct.unpack("!I", recv_exact(s, 4))
    if n > MAX_FRAME:
        raise ProtocolError(f"Khung quá lớn: {n} byte")
    return recv_exact(s, n)


def send_json(s, obj):
    send_frame(s, json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))


def decode_json(payload):
    """Giải mã nội dung khung; trả None nếu không phải đối tượng JSON hợp lệ."""
    try:
        obj = json.loads(payload.decode("utf-8"))
    except ValueError:                       # UnicodeDecodeError, JSONDecodeError
        return None
    return obj if isinstance(obj, dict) else None


def recv_json(s):
    return decode_json(recv_frame(s))


DISC_SERVICE = "caro"


def discover(disc_port, wait_ms=1000, bcast="255.255.255.255"):
    """Mã giả 6.7 (DISCOVER, Bài 6.5): phát một yêu cầu broadcast, thu thập MỌI phản hồi đến hết thời gian chờ.

    Trả danh sách (ip, port, name, rooms, players), sắp theo tên để kết quả ổn định.
    """
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)        # bắt buộc để gửi tới địa chỉ broadcast
    qid = random.randint(1, 999999)
    s.sendto(json.dumps({"type": "discover", "service": DISC_SERVICE, "id": qid}).encode(), (bcast, disc_port))
    found, deadline = [], time.monotonic() + wait_ms / 1000
    while True:
        left = deadline - time.monotonic()
        if left <= 0:
            break
        s.settimeout(left)
        try:
            data, addr = s.recvfrom(2048)
        except socket.timeout:
            break
        r = decode_json(data)
        if r and r.get("id") == qid:                                # chỉ nhận phản hồi đúng yêu cầu của mình
            found.append((addr[0], r.get("port"), r.get("name"), r.get("rooms"), r.get("players")))
    s.close()
    return sorted(found, key=lambda f: (str(f[2]), f[0], str(f[1])))
