"""Bài 7.8 – Client MQTT 5.0 tối giản trên socket TCP (cài đặt các lệnh mức thư viện của Mã giả 7.9).

Hỗ trợ: CONNECT (kèm di chúc – Last Will) / CONNACK, PUBLISH QoS 0 và QoS 1 (PUBACK), SUBSCRIBE / SUBACK,
PINGREQ / PINGRESP, DISCONNECT. Không hỗ trợ QoS 2, TLS, thuộc tính MQTT 5.0 (luôn gửi độ dài thuộc tính 0).
Định khung MQTT: 1 byte loại gói + cờ, "độ dài còn lại" mã hóa varint 1–4 byte, rồi đúng chừng ấy byte
(kỹ thuật tiền tố độ dài của Mục 3.4, đọc bằng RECV_EXACT – Mã giả 3.4). Đặc tả: OASIS MQTT 5.0.
"""
import socket
import struct
import time

CONNECT, CONNACK, PUBLISH, PUBACK = 1, 2, 3, 4
SUBSCRIBE, SUBACK, PINGREQ, PINGRESP, DISCONNECT = 8, 9, 12, 13, 14


def mqtt_str(s):
    b = s.encode("utf-8")
    return struct.pack("!H", len(b)) + b                  # chuỗi UTF-8: 2 byte độ dài + dữ liệu


def varint(n):
    out = b""
    while True:                                           # 7 bit mỗi byte, bit cao = còn byte tiếp
        n, d = n >> 7, n & 0x7F
        out += bytes([d | (0x80 if n else 0)])
        if not n:
            return out


def topic_matches(flt, topic):
    """Bộ lọc chủ đề: '+' khớp đúng một cấp, '#' khớp mọi cấp còn lại (Mục 7.5.5)."""
    f, t = flt.split("/"), topic.split("/")
    for i, part in enumerate(f):
        if part == "#":
            return True
        if i >= len(t) or (part != "+" and part != t[i]):
            return False
    return len(f) == len(t)


class MqttClient:
    def __init__(self):
        self.sock, self.next_id, self.handlers = None, 1, []

    def _send(self, ptype, flags, body):
        self.sock.sendall(bytes([(ptype << 4) | flags]) + varint(len(body)) + body)   # SEND_ALL
        self.last_send = time.monotonic()                 # keep alive tính theo gói client GỬI đi

    def _recv_exact(self, n):
        buf = b""
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("Broker đóng kết nối")
            buf += chunk
        return buf

    def read_packet(self, first_timeout=5.0):
        """Đọc một gói; chỉ chờ byte đầu với first_timeout, phần còn lại chờ tối đa 5 giây."""
        self.sock.settimeout(first_timeout)
        b0 = self._recv_exact(1)[0]                       # hết giờ ở đây: chưa mất byte nào của gói
        self.sock.settimeout(5.0)
        n, shift = 0, 0
        while True:                                       # giải mã varint "độ dài còn lại"
            d = self._recv_exact(1)[0]
            n |= (d & 0x7F) << shift
            shift += 7
            if not d & 0x80:
                break
        return b0 >> 4, b0 & 0x0F, self._recv_exact(n)

    def _dispatch(self, flags, body):
        """Gói PUBLISH đến: tách chủ đề, mã gói (QoS > 0), bỏ thuộc tính, gọi hàm xử lý khớp bộ lọc."""
        qos, retain = (flags >> 1) & 3, flags & 1
        tlen = struct.unpack("!H", body[:2])[0]
        topic, i = body[2:2 + tlen].decode("utf-8"), 2 + tlen
        if qos > 0:
            pid, i = body[i:i + 2], i + 2
            self._send(PUBACK, 0, pid)                    # QoS 1: xác nhận cho broker
        plen, shift = 0, 0
        while True:                                       # độ dài thuộc tính (varint) rồi bỏ qua thuộc tính
            d = body[i]; i += 1
            plen |= (d & 0x7F) << shift; shift += 7
            if not d & 0x80:
                break
        payload = body[i + plen:].decode("utf-8")
        for flt, fn in self.handlers:
            if topic_matches(flt, topic):
                fn(topic, payload, qos, bool(retain))

    def _wait(self, ptype):
        """Chờ gói loại ptype; gói PUBLISH đến trong lúc chờ vẫn được xử lý."""
        while True:
            t, flags, body = self.read_packet()
            if t == ptype:
                return body
            if t == PUBLISH:
                self._dispatch(flags, body)

    def connect(self, host, port, client_id, keep_alive=60, will=None):
        """MQTT_CONNECT; will = (chủ_đề, nội_dung, qos, retain)."""
        self.sock = socket.create_connection((host, port), timeout=5)
        self.keep_alive = keep_alive
        flags = 0x02                                      # Clean Start
        payload = mqtt_str(client_id)
        if will:
            topic, msg, qos, retain = will
            flags |= 0x04 | (qos << 3) | (0x20 if retain else 0)
            payload += varint(0) + mqtt_str(topic) + mqtt_str(msg)   # thuộc tính di chúc, chủ đề, nội dung
        body = mqtt_str("MQTT") + bytes([5, flags]) + struct.pack("!H", keep_alive) + varint(0) + payload
        self._send(CONNECT, 0, body)
        ack = self._wait(CONNACK)
        if ack[1] != 0:                                   # mã lý do 0x00: thành công
            raise ConnectionError(f"CONNACK mã lý do 0x{ack[1]:02X}")

    def publish(self, topic, payload, qos=0, retain=False):
        body = mqtt_str(topic)
        if qos > 0:
            pid = self.next_id; self.next_id += 1
            body += struct.pack("!H", pid)
        self._send(PUBLISH, (qos << 1) | (1 if retain else 0), body + varint(0) + payload.encode("utf-8"))
        if qos == 1:
            self._wait(PUBACK)                            # QoS 1: chờ broker xác nhận

    def subscribe(self, flt, qos, handler):
        self.handlers.append((flt, handler))              # đăng ký trước: tin giữ lại có thể đến ngay
        pid = self.next_id; self.next_id += 1
        self._send(SUBSCRIBE, 0x2, struct.pack("!H", pid) + varint(0) + mqtt_str(flt) + bytes([qos]))
        ack = self._wait(SUBACK)
        code = ack[3 + ack[2]] if len(ack) > 3 else ack[-1]   # sau mã gói và thuộc tính (thường rỗng)
        if code >= 0x80:
            raise ConnectionError(f"SUBACK từ chối {flt}: 0x{code:02X}")
        return code

    def run_event_loop(self, on_ping=None):
        """RUN_EVENT_LOOP: nhận gói, gọi hàm xử lý; chưa gửi gì trong keep_alive/2 giây thì gửi PINGREQ."""
        while True:
            wait = self.keep_alive / 2 - (time.monotonic() - self.last_send)
            if wait <= 0:
                self._send(PINGREQ, 0, b"")
                continue
            try:
                t, flags, body = self.read_packet(first_timeout=wait)
            except socket.timeout:
                continue
            if t == PUBLISH:
                self._dispatch(flags, body)
            elif t == PINGRESP and on_ping:
                on_ping()

    def disconnect(self):
        self._send(DISCONNECT, 0, b"")                    # mã lý do 0 (đóng bình thường): broker bỏ di chúc
        self.sock.close()
