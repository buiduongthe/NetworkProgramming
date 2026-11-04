"""Bài mở rộng MR6.4 – Server Caro nhiều phòng trên TCP, khám phá trong LAN bằng UDP broadcast.

Thiết kế (mở rộng từ MR3.3):
  - Luồng chấp nhận + một luồng đọc cho mỗi kết nối: chỉ nhận khung JSON, đẩy sự kiện vào hàng đợi.
  - Luồng khám phá (RESPONDER của Bài 6.5): nhận yêu cầu discover trên UDP, đẩy sự kiện vào hàng đợi.
  - MỘT luồng trò chơi giữ toàn bộ trạng thái (sảnh, các phòng, bàn cờ), xử lý tuần tự từng sự kiện.
    Hạn chót của nước đi trong mọi phòng được gom lại: chờ hàng đợi đến hạn chót sớm nhất.
  - Server là trọng tài: kiểm tra lượt, tọa độ, ô trống, phát hiện thắng; client chỉ hiển thị.
  - Mỗi nước đi hợp lệ được phát (broadcast) tới cả hai người chơi và mọi người xem trong phòng.

Trạng thái của một phòng: CHO (thiếu người) -> DANG_CHOI -> KET_THUC -(cả hai xin chơi lại)-> DANG_CHOI.

Chạy:  python3 server_caro.py [host] [port] [--kham-pha 9999] [--ten Caro] [--het-gio-ms 30000]
"""
import argparse
import json
import queue
import socket
import threading
import time

from ban_co import N, EMPTY, cell_name, five_line, new_board
from giao_thuc import (ConnectionClosed, DISC_SERVICE, ProtocolError, decode_json, recv_frame,
                       send_json)

CHO, DANG_CHOI, KET_THUC = "CHO", "DANG_CHOI", "KET_THUC"
REASON_TEXT = {"five": "5 quân liên tiếp", "timeout": "đối thủ hết giờ",
               "left": "đối thủ rời phòng", "draw": "hòa (kín bàn)"}
ERRORS = {
    "BAD_JSON": "Khung không phải đối tượng JSON",
    "NEED_HELLO": "Phải gửi hello trước",
    "BAD_NAME": "Tên phải là chuỗi 1–16 ký tự",
    "NAME_TAKEN": "Tên đã có người dùng",
    "UNKNOWN_TYPE": "Loại thông điệp không hỗ trợ",
    "ALREADY_IN_ROOM": "Bạn đang ở trong một phòng",
    "BAD_ROOM_NAME": "Tên phòng phải là chuỗi 1–16 ký tự",
    "ROOM_EXISTS": "Phòng đã tồn tại",
    "NO_SUCH_ROOM": "Không có phòng này",
    "ROOM_FULL": "Phòng đã đủ hai người chơi",
    "NOT_IN_ROOM": "Bạn chưa vào phòng nào",
    "NOT_PLAYER": "Người xem không được đánh",
    "GAME_NOT_RUNNING": "Ván chưa bắt đầu hoặc đã kết thúc",
    "NOT_YOUR_TURN": "Chưa đến lượt bạn",
    "BAD_MOVE": "Nước đi phải có r, c là số nguyên",
    "OUT_OF_BOARD": "Ô nằm ngoài bàn cờ",
    "CELL_TAKEN": "Ô đã có quân",
}


# ---------------------------------------------------------------- các luồng nhận
def reader(conn, events):
    try:
        while True:
            events.put(("msg", conn, decode_json(recv_frame(conn))))
    except (ConnectionClosed, ProtocolError, OSError):
        events.put(("closed", conn, None))


def acceptor(srv, events):
    while True:
        try:
            conn, (ip, port) = srv.accept()
        except OSError:
            return
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)   # tắt Nagle: nước đi đến ngay (Go bật sẵn)
        events.put(("open", conn, f"{ip}:{port}"))
        threading.Thread(target=reader, args=(conn, events), daemon=True).start()


def responder(udp, events):
    """RESPONDER (Mã giả 6.7): nhận discover, để luồng trò chơi trả lời (nó mới biết số phòng)."""
    while True:
        try:
            data, addr = udp.recvfrom(2048)
        except OSError:
            return
        q = decode_json(data)
        if q and q.get("type") == "discover" and q.get("service") == DISC_SERVICE:
            events.put(("discover", addr, q))


# ---------------------------------------------------------------- trạng thái
class Client:
    def __init__(self, conn, addr):
        self.conn, self.addr = conn, addr
        self.name = None
        self.room = None              # tên phòng đang ở
        self.role = None              # "X", "O" hoặc "xem"


class Room:
    def __init__(self, name):
        self.name = name
        self.seats = {"X": None, "O": None}      # Client của hai người chơi
        self.watchers = []
        self.board = new_board()
        self.state = CHO
        self.turn = "X"
        self.moves = 0
        self.game_no = 0
        self.deadline = None
        self.rematch = set()
        self.last = None

    def members(self):
        return [c for c in (self.seats["X"], self.seats["O"]) if c] + self.watchers

    def summary(self):
        x, o = self.seats["X"], self.seats["O"]
        return {"room": self.name, "x": x.name if x else None, "o": o.name if o else None,
                "watchers": len(self.watchers), "state": self.state}


class Server:
    def __init__(self, a, udp):
        self.a, self.udp = a, udp
        self.clients = {}             # conn -> Client
        self.rooms = {}               # tên -> Room (giữ thứ tự tạo)

    # ----- gửi -----
    def send(self, cl, obj):
        try:
            send_json(cl.conn, obj)
        except OSError:
            pass

    def to_room(self, room, obj):
        for cl in room.members():
            self.send(cl, obj)

    def error(self, cl, code, extra=""):
        print(f"[server] Từ chối {cl.name or cl.addr}: {code}{extra}", flush=True)
        self.send(cl, {"type": "error", "code": code, "msg": ERRORS[code]})

    def room_list(self):
        return [r.summary() for r in self.rooms.values()]

    # ----- ván cờ -----
    def start_game(self, room):
        room.game_no += 1
        room.board, room.turn, room.moves, room.last = new_board(), "X", 0, None
        room.state, room.rematch = DANG_CHOI, set()
        room.deadline = time.monotonic() + self.a.het_gio_ms / 1000
        x, o = room.seats["X"].name, room.seats["O"].name
        print(f"[server] Phòng {room.name}, ván {room.game_no}: X = {x}, O = {o}", flush=True)
        self.to_room(room, {"type": "start", "room": room.name, "game_no": room.game_no, "x": x, "o": o,
                            "size": N, "timeout_ms": self.a.het_gio_ms})

    def finish(self, room, winner_mark, reason, line=None):
        room.state, room.deadline = KET_THUC, None
        winner = room.seats[winner_mark].name if winner_mark and room.seats[winner_mark] else None
        result = f"{winner} ({winner_mark}) thắng" if winner else "hòa"
        print(f"[server] Phòng {room.name}, ván {room.game_no} kết thúc sau {room.moves} nước: "
              f"{result} – {REASON_TEXT[reason]}", flush=True)
        self.to_room(room, {"type": "over", "room": room.name, "game_no": room.game_no, "winner": winner,
                            "mark": winner_mark, "reason": reason, "moves": room.moves,
                            "line": [list(p) for p in line or []]})

    def on_move(self, cl, msg):
        room = self.rooms.get(cl.room)
        if room is None:
            return self.error(cl, "NOT_IN_ROOM")
        if cl.role == "xem":
            return self.error(cl, "NOT_PLAYER")
        if room.state != DANG_CHOI:
            return self.error(cl, "GAME_NOT_RUNNING")
        if cl.role != room.turn:
            return self.error(cl, "NOT_YOUR_TURN")
        r, c = msg.get("r"), msg.get("c")
        if not all(isinstance(v, int) and not isinstance(v, bool) for v in (r, c)):
            return self.error(cl, "BAD_MOVE")
        if not (0 <= r < N and 0 <= c < N):
            return self.error(cl, "OUT_OF_BOARD", f" ({r}, {c})")
        if room.board[r][c] != EMPTY:
            return self.error(cl, "CELL_TAKEN", f" ({cell_name(r, c)})")
        room.board[r][c] = cl.role                          # nước đi hợp lệ: ghi vào bàn cờ của server
        room.moves += 1
        room.last = (r, c)
        line = five_line(room.board, r, c)                  # phán xử TRƯỚC khi báo nước đi
        ended = line is not None or room.moves == N * N
        nxt = None if ended else ("O" if cl.role == "X" else "X")
        self.to_room(room, {"type": "moved", "room": room.name, "r": r, "c": c, "mark": cl.role,
                            "by": cl.name, "move_no": room.moves, "next": nxt})   # next = null: ván đã hết
        if line:
            return self.finish(room, cl.role, "five", line)
        if ended:
            return self.finish(room, None, "draw")
        room.turn = nxt
        room.deadline = time.monotonic() + self.a.het_gio_ms / 1000   # mỗi nước có hạn chót riêng

    def on_timeout(self, room):
        loser = room.seats[room.turn]
        print(f"[server] Phòng {room.name}: {loser.name} ({room.turn}) hết giờ", flush=True)
        self.finish(room, "O" if room.turn == "X" else "X", "timeout")

    # ----- phòng -----
    def enter(self, cl, room, role):
        cl.room, cl.role = room.name, role
        if role == "xem":
            room.watchers.append(cl)
        else:
            room.seats[role] = cl
        print(f"[server] {cl.name} vào phòng {room.name} ({'người xem' if role == 'xem' else 'quân ' + role})",
              flush=True)
        self.send(cl, {"type": "joined", "room": room.name, "role": role, "info": room.summary()})
        if role == "xem" and room.state != CHO:              # người xem đến muộn: gửi ảnh chụp bàn cờ
            self.send(cl, {"type": "board", "room": room.name, "game_no": room.game_no,
                           "rows": ["".join(row) for row in room.board], "moves": room.moves,
                           "turn": room.turn if room.state == DANG_CHOI else None, "state": room.state})

    def leave_room(self, cl):
        room = self.rooms.get(cl.room)
        if room is None:
            return
        role = cl.role
        cl.room = cl.role = None
        if role == "xem":
            room.watchers.remove(cl)
            return
        if room.state == DANG_CHOI:                         # bỏ dở ván: đối thủ thắng
            self.finish(room, "O" if role == "X" else "X", "left")
        room.seats[role] = None
        print(f"[server] {cl.name} rời phòng {room.name}", flush=True)
        self.to_room(room, {"type": "left", "room": room.name, "player": cl.name, "role": role})
        room.state, room.deadline, room.rematch = CHO, None, set()
        if room.seats["X"] is None and room.seats["O"] is None:
            print(f"[server] Phòng {room.name} đóng (không còn người chơi)", flush=True)
            for w in room.watchers:
                w.room = w.role = None
                self.send(w, {"type": "room_closed", "room": room.name})
            del self.rooms[room.name]

    # ----- xử lý thông điệp -----
    def on_message(self, cl, msg):
        if msg is None:
            return self.error(cl, "BAD_JSON")
        kind = msg.get("type")
        if cl.name is None:
            if kind != "hello":
                return self.error(cl, "NEED_HELLO")
            name = msg.get("name")
            if not isinstance(name, str) or not 1 <= len(name.strip()) <= 16:
                return self.error(cl, "BAD_NAME")
            if any(o.name == name.strip() for o in self.clients.values()):
                return self.error(cl, "NAME_TAKEN", f" ({name.strip()})")
            cl.name = name.strip()
            print(f"[server] {cl.name} vào sảnh ({cl.addr})", flush=True)
            return self.send(cl, {"type": "welcome", "name": cl.name, "rooms": self.room_list()})
        if kind == "list":
            return self.send(cl, {"type": "rooms", "rooms": self.room_list()})
        if kind in ("create", "join", "watch"):
            return self.on_room_request(cl, kind, msg.get("room"))
        if kind == "move":
            return self.on_move(cl, msg)
        if kind == "rematch":
            return self.on_rematch(cl)
        if kind == "leave":
            if cl.room is None:
                return self.error(cl, "NOT_IN_ROOM")
            return self.leave_room(cl)
        if kind == "bye":
            self.leave_room(cl)
            print(f"[server] {cl.name} tạm biệt", flush=True)
            cl.name = None                                  # không còn trong sảnh; "closed" đến sau sẽ im lặng
            try:
                cl.conn.shutdown(socket.SHUT_RDWR)          # luồng đọc sẽ báo "closed"
            except OSError:
                pass
            return
        self.error(cl, "UNKNOWN_TYPE", f" ({kind!r})")

    def on_room_request(self, cl, kind, name):
        if cl.room is not None:
            return self.error(cl, "ALREADY_IN_ROOM")
        if not isinstance(name, str) or not 1 <= len(name.strip()) <= 16:
            return self.error(cl, "BAD_ROOM_NAME")
        name = name.strip()
        room = self.rooms.get(name)
        if kind == "create":
            if room:
                return self.error(cl, "ROOM_EXISTS", f" ({name})")
            room = self.rooms[name] = Room(name)
            print(f"[server] {cl.name} tạo phòng {name}", flush=True)
            return self.enter(cl, room, "X")
        if room is None:
            return self.error(cl, "NO_SUCH_ROOM", f" ({name})")
        if kind == "watch":
            return self.enter(cl, room, "xem")
        free = [m for m in ("X", "O") if room.seats[m] is None]
        if not free:
            return self.error(cl, "ROOM_FULL", f" ({name})")
        self.enter(cl, room, free[0])
        if room.seats["X"] and room.seats["O"]:
            self.start_game(room)

    def on_rematch(self, cl):
        room = self.rooms.get(cl.room)
        if room is None or cl.role not in ("X", "O"):
            return self.error(cl, "NOT_PLAYER")
        if room.state != KET_THUC:
            return self.error(cl, "GAME_NOT_RUNNING")
        room.rematch.add(cl.role)
        if len(room.rematch) < 2:
            return self.to_room(room, {"type": "rematch_wait", "room": room.name, "by": cl.name})
        room.seats["X"], room.seats["O"] = room.seats["O"], room.seats["X"]   # đổi màu quân: người kia đi trước
        for m in ("X", "O"):
            room.seats[m].role = m
        self.start_game(room)

    def on_discover(self, addr, q):
        players = sum(1 for c in self.clients.values() if c.name)
        reply = {"id": q.get("id"), "name": self.a.ten, "port": self.a.port,
                 "rooms": len(self.rooms), "players": players}
        print(f"[server] Trả lời discover từ {addr[0]}:{addr[1]}", flush=True)
        self.udp.sendto(json.dumps(reply, ensure_ascii=False).encode("utf-8"), addr)

    def on_closed(self, conn):
        cl = self.clients.pop(conn, None)
        if cl is None:
            return
        if cl.name:
            self.leave_room(cl)
            print(f"[server] {cl.name} ngắt kết nối", flush=True)
        conn.close()

    def expired_room(self):
        """Phòng có hạn chót sớm nhất (hoặc None) và số giây còn lại."""
        rooms = [r for r in self.rooms.values() if r.deadline is not None]
        if not rooms:
            return None, None
        room = min(rooms, key=lambda r: r.deadline)
        return room, room.deadline - time.monotonic()


def main():
    ap = argparse.ArgumentParser(description="Server Caro nhiều phòng")
    ap.add_argument("host", nargs="?", default="0.0.0.0")
    ap.add_argument("port", nargs="?", type=int, default=9100)
    ap.add_argument("--kham-pha", type=int, default=9999, help="cổng UDP nhận yêu cầu khám phá")
    ap.add_argument("--ten", default="Caro", help="tên server hiển thị khi khám phá")
    ap.add_argument("--het-gio-ms", type=int, default=30000, help="thời gian tối đa cho mỗi nước đi")
    a = ap.parse_args()

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((a.host, a.port))
    srv.listen(16)
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    udp.bind(("0.0.0.0", a.kham_pha))                     # nhận broadcast: phải bind 0.0.0.0
    print(f"[server] {a.ten}: Caro {N}×{N} tại {a.host}:{a.port}, khám phá UDP cổng {a.kham_pha}, "
          f"mỗi nước tối đa {a.het_gio_ms} ms", flush=True)
    events = queue.Queue()
    threading.Thread(target=acceptor, args=(srv, events), daemon=True).start()
    threading.Thread(target=responder, args=(udp, events), daemon=True).start()
    s = Server(a, udp)
    try:
        while True:
            room, left = s.expired_room()
            if room is not None and left <= 0:              # có phòng hết giờ: xử lý trước
                s.on_timeout(room)
                continue
            try:
                kind, key, data = events.get(timeout=left)
            except queue.Empty:
                continue                                     # vòng sau sẽ thấy phòng hết giờ
            if kind == "open":
                s.clients[key] = Client(key, data)
            elif kind == "msg":
                if key in s.clients:
                    s.on_message(s.clients[key], data)
            elif kind == "closed":
                s.on_closed(key)
            elif kind == "discover":
                s.on_discover(key, data)
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        srv.close()
        udp.close()


if __name__ == "__main__":
    main()
