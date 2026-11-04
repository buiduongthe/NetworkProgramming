"""Bài mở rộng MR3.3 – Server trò chơi đoán số nhiều người chơi trên TCP.

Server chọn một số bí mật; người chơi lần lượt đoán, server trả lời "thấp quá", "cao quá" hoặc "đúng".
Thiết kế:
  - Mỗi kết nối có một luồng đọc (reader) chỉ làm một việc: nhận khung JSON và đẩy sự kiện vào hàng đợi.
  - MỘT luồng trò chơi (vòng lặp sự kiện) giữ toàn bộ trạng thái, xử lý tuần tự từng sự kiện,
    nên không cần khóa (lock). Hết giờ mỗi lượt = thời gian chờ của queue.get().
  - Máy trạng thái của server: CHO (chờ đủ người) -> DANG_CHOI -> (kết thúc ván) -> CHO.
  - Server là bên quyết định: kiểm tra mọi thông điệp (đúng lượt, kiểu số, khoảng hợp lệ), không tin client.

Chạy:  python3 server_doan_so.py [host] [port] [--so-nguoi 2] [--het-gio-ms 30000]
                                 [--seed 0] [--min 1] [--max 100] [--so-van 0]
       (--seed 0: lấy hạt giống theo thời gian; --so-van 0: chơi mãi)
"""
import argparse
import queue
import socket
import threading
import time

from giao_thuc import ConnectionClosed, Lcg, ProtocolError, decode_json, recv_frame, send_json

CHO, DANG_CHOI = "CHO", "DANG_CHOI"            # hai trạng thái của server
HINT_TEXT = {"THAP": "thấp quá", "CAO": "cao quá", "DUNG": "ĐÚNG"}
ERRORS = {
    "BAD_JSON": "Khung không phải đối tượng JSON",
    "NEED_HELLO": "Phải gửi hello trước",
    "BAD_NAME": "Tên phải là chuỗi 1–16 ký tự",
    "NAME_TAKEN": "Tên đã có người dùng",
    "GAME_IN_PROGRESS": "Ván đang diễn ra, hãy vào lại sau",
    "GAME_NOT_STARTED": "Ván chưa bắt đầu",
    "NOT_YOUR_TURN": "Chưa đến lượt bạn",
    "BAD_VALUE": "Giá trị đoán phải là số nguyên",
    "OUT_OF_RANGE": "Số đoán nằm ngoài khoảng cho phép",
    "UNKNOWN_TYPE": "Loại thông điệp không hỗ trợ",
}


def reader(conn, addr, events):
    """Luồng đọc của một kết nối: chuyển mỗi khung thành sự kiện ("msg", conn, obj) cho vòng trò chơi."""
    try:
        while True:
            obj = decode_json(recv_frame(conn))
            events.put(("msg", conn, obj))          # obj = None nghĩa là khung hỏng
    except (ConnectionClosed, ProtocolError, OSError) as e:
        events.put(("closed", conn, str(e)))


def acceptor(srv, events):
    """Luồng chấp nhận kết nối: mỗi client mới có một luồng đọc riêng."""
    while True:
        try:
            conn, (ip, port) = srv.accept()
        except OSError:
            return                                   # socket nghe đã đóng
        addr = f"{ip}:{port}"
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)   # tắt Nagle: phản hồi nhỏ đi ngay (Go bật sẵn)
        events.put(("open", conn, addr))
        threading.Thread(target=reader, args=(conn, addr, events), daemon=True).start()


class Game:
    """Toàn bộ trạng thái trò chơi; chỉ luồng trò chơi được đọc/ghi."""

    def __init__(self, a):
        self.a = a
        self.rng = Lcg(a.seed if a.seed else int(time.time()))
        self.state = CHO
        self.names = {}                   # conn -> tên (người đã gửi hello hợp lệ)
        self.order = []                   # thứ tự lượt (danh sách conn)
        self.turn = 0                     # chỉ số người đang có lượt trong order
        self.turn_no = 0
        self.deadline = None              # hạn chót của lượt hiện tại (time.monotonic())
        self.secret = 0
        self.lo, self.hi = a.min, a.max
        self.round = 0

    # ----- gửi -----
    def send(self, conn, obj):
        try:
            send_json(conn, obj)
        except OSError:
            pass                          # client đã mất: luồng đọc sẽ báo "closed"

    def broadcast(self, obj):
        for conn in list(self.names):
            self.send(conn, obj)

    def error(self, conn, code, who, extra=""):
        print(f"[server] Từ chối {who}: {code}{extra}", flush=True)
        self.send(conn, {"type": "error", "code": code, "msg": ERRORS[code]})

    def drop(self, conn):
        """Đóng hẳn kết nối (luồng đọc sẽ thoát)."""
        self.names.pop(conn, None)
        try:
            conn.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        conn.close()

    # ----- máy trạng thái -----
    def start_round(self):
        self.round += 1
        self.state = DANG_CHOI
        self.secret = self.rng.next_int(self.a.min, self.a.max)
        self.lo, self.hi = self.a.min, self.a.max
        self.order = list(self.names)
        self.turn, self.turn_no = 0, 0
        order_names = [self.names[c] for c in self.order]
        print(f"[server] Bắt đầu ván {self.round}: số bí mật = {self.secret} (chỉ server biết), "
              f"thứ tự: {', '.join(order_names)}", flush=True)
        self.broadcast({"type": "start", "round": self.round, "order": order_names,
                        "min": self.a.min, "max": self.a.max})
        self.next_turn()

    def next_turn(self):
        """Giao lượt cho người kế tiếp và đặt hạn chót."""
        self.turn_no += 1
        name = self.names[self.order[self.turn]]
        self.deadline = time.monotonic() + self.a.het_gio_ms / 1000
        self.broadcast({"type": "turn", "player": name, "lo": self.lo, "hi": self.hi,
                        "turn_no": self.turn_no, "timeout_ms": self.a.het_gio_ms})

    def advance(self):
        self.turn = (self.turn + 1) % len(self.order)
        self.next_turn()

    def end_round(self, winner):
        if winner:
            print(f"[server] Kết thúc ván {self.round}: {winner} thắng sau {self.turn_no} lượt", flush=True)
        else:
            print(f"[server] Kết thúc ván {self.round}: không còn người chơi", flush=True)
        self.broadcast({"type": "game_over", "winner": winner, "secret": self.secret,
                        "turns": self.turn_no})
        for conn in list(self.names):              # mỗi ván một phiên: đóng mọi kết nối
            self.send(conn, {"type": "bye"})
            self.drop(conn)
        self.state, self.order, self.deadline = CHO, [], None

    def remove_player(self, conn):
        name = self.names.pop(conn)
        print(f"[server] {name} rời trò chơi (còn {len(self.names)} người)", flush=True)
        self.broadcast({"type": "left", "player": name})
        if self.state != DANG_CHOI:
            return
        i = self.order.index(conn)
        was_current = (i == self.turn)
        self.order.pop(i)
        if not self.order:
            self.end_round(None)
            return
        if i < self.turn:
            self.turn -= 1                           # giữ nguyên người đang có lượt
        if was_current:
            self.turn %= len(self.order)
            self.next_turn()

    def on_timeout(self):
        name = self.names[self.order[self.turn]]
        print(f"[server] {name} hết giờ ở lượt {self.turn_no}", flush=True)
        self.broadcast({"type": "timeout", "player": name})
        self.advance()

    # ----- xử lý thông điệp -----
    def on_message(self, conn, addr, msg):
        who = self.names.get(conn, addr)
        if msg is None:
            return self.error(conn, "BAD_JSON", who)
        kind = msg.get("type")
        if conn not in self.names:                   # chưa vào phòng: chỉ chấp nhận hello
            if kind != "hello":
                return self.error(conn, "NEED_HELLO", who)
            return self.on_hello(conn, addr, msg)
        if kind == "guess":
            return self.on_guess(conn, who, msg.get("value"))
        if kind == "bye":
            self.remove_player(conn)
            self.drop(conn)
            return
        self.error(conn, "UNKNOWN_TYPE", who, f" ({kind!r})")

    def on_hello(self, conn, addr, msg):
        name = msg.get("name")
        if not isinstance(name, str) or not 1 <= len(name.strip()) <= 16:
            return self.error(conn, "BAD_NAME", addr)
        name = name.strip()
        if name in self.names.values():
            return self.error(conn, "NAME_TAKEN", addr, f" ({name})")
        if self.state == DANG_CHOI:
            self.error(conn, "GAME_IN_PROGRESS", name)
            self.send(conn, {"type": "bye"})
            self.drop(conn)
            return
        self.names[conn] = name
        print(f"[server] {name} vào phòng chờ ({len(self.names)}/{self.a.so_nguoi})", flush=True)
        self.send(conn, {"type": "welcome", "name": name, "waiting": len(self.names),
                         "need": self.a.so_nguoi, "min": self.a.min, "max": self.a.max})
        if len(self.names) >= self.a.so_nguoi:
            self.start_round()

    def on_guess(self, conn, who, value):
        if self.state != DANG_CHOI:
            return self.error(conn, "GAME_NOT_STARTED", who)
        if self.order[self.turn] is not conn:
            return self.error(conn, "NOT_YOUR_TURN", who)
        if not isinstance(value, int) or isinstance(value, bool):    # true/false không phải số
            return self.error(conn, "BAD_VALUE", who, f" ({value!r})")
        if not self.lo <= value <= self.hi:
            return self.error(conn, "OUT_OF_RANGE", who, f" ({value} ngoài {self.lo}–{self.hi})")
        hint = "DUNG" if value == self.secret else ("THAP" if value < self.secret else "CAO")
        print(f"[server] Lượt {self.turn_no}: {who} đoán {value} -> {HINT_TEXT[hint]}", flush=True)
        self.broadcast({"type": "result", "player": who, "value": value, "hint": hint})
        if hint == "DUNG":
            self.end_round(who)
            return
        if hint == "THAP":
            self.lo = value + 1
        else:
            self.hi = value - 1
        self.advance()


def main():
    ap = argparse.ArgumentParser(description="Server trò chơi đoán số (TCP, khung JSON)")
    ap.add_argument("host", nargs="?", default="0.0.0.0")
    ap.add_argument("port", nargs="?", type=int, default=9000)
    ap.add_argument("--so-nguoi", type=int, default=2, help="số người cần để bắt đầu ván")
    ap.add_argument("--het-gio-ms", type=int, default=30000, help="thời gian tối đa của một lượt")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--min", type=int, default=1)
    ap.add_argument("--max", type=int, default=100)
    ap.add_argument("--so-van", type=int, default=0, help="dừng sau số ván này (0 = chơi mãi)")
    a = ap.parse_args()

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((a.host, a.port))
    srv.listen(16)
    print(f"[server] Trò chơi đoán số tại {a.host}:{a.port}; cần {a.so_nguoi} người; "
          f"mỗi lượt tối đa {a.het_gio_ms} ms", flush=True)
    events = queue.Queue()
    threading.Thread(target=acceptor, args=(srv, events), daemon=True).start()
    game = Game(a)
    addrs = {}                                       # conn -> "ip:port"
    try:
        while True:
            if game.deadline is not None and time.monotonic() >= game.deadline:
                game.on_timeout()                    # kiểm tra trước: thông điệp đến liên tục không "hoãn" được hết giờ
            timeout = None
            if game.deadline is not None:
                timeout = max(0.0, game.deadline - time.monotonic())
            try:
                kind, conn, data = events.get(timeout=timeout)
            except queue.Empty:                      # hết giờ của lượt hiện tại
                game.on_timeout()
                continue
            if kind == "open":
                addrs[conn] = data
                print(f"[server] {data} kết nối", flush=True)
            elif kind == "msg":
                game.on_message(conn, addrs.get(conn, "?"), data)
            elif kind == "closed":
                addrs.pop(conn, None)
                if conn in game.names:               # mất kết nối giữa chừng
                    game.remove_player(conn)
                conn.close()
            if a.so_van and game.round >= a.so_van and game.state == CHO:
                print(f"[server] Đã chơi {game.round} ván, dừng server", flush=True)
                break
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
