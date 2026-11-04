"""Bài mở rộng MR6.4 – Client Caro: người chơi thật (dòng lệnh), bot, hoặc người xem tự động.

Kết nối: chỉ định host, port; hoặc --tim CỔNG để tìm server trong LAN bằng UDP broadcast (Bài 6.5).

Chế độ người chơi (không có --bot, --xem): gõ lệnh
    ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> (ví dụ h8) | lai | roi | thoat
Chế độ bot (--bot tan_cong|phong_thu|can_bang|tap_su|im_lang, kèm --tao hoặc --vao):
    --thu-loi: cố tình gửi nước sai (chưa đến lượt, ô đã có quân, ngoài bàn) để xem server kiểm tra.
Chế độ người xem tự động (--xem <phòng>): in tiến trình và bàn cờ cuối mỗi ván, thoát khi phòng đóng.

Chạy:  python3 nguoi_choi.py <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P]
                             [--bot KIỂU] [--seed 1] [--thu-loi] [--so-van 1] [--tre-ms 300]
"""
import argparse
import socket
import sys
import threading
import time

from ban_co import N, EMPTY, STYLES, Lcg, cell_name, choose_move, new_board, parse_cell, render
from giao_thuc import ConnectionClosed, ProtocolError, discover, recv_json, send_json

REASON_TEXT = {"five": "5 quân liên tiếp", "timeout": "đối thủ hết giờ",
               "left": "đối thủ rời phòng", "draw": "hòa (kín bàn)"}


def over_text(msg):
    if msg["winner"]:
        result = f"{msg['winner']} ({msg['mark']}) thắng – {REASON_TEXT[msg['reason']]}"
    else:
        result = REASON_TEXT[msg["reason"]]
    return f"Ván {msg['game_no']} kết thúc sau {msg['moves']} nước: {result}"


def line_text(msg):
    return " ".join(cell_name(r, c) for r, c in msg["line"])


class View:
    """Bàn cờ phía client – chỉ là bản sao để hiển thị, được cập nhật từ thông điệp của server."""

    def __init__(self):
        self.board, self.last, self.mark, self.turn = new_board(), None, None, "X"

    def apply(self, msg, me):
        t = msg.get("type")
        if t == "start":
            self.board, self.last, self.turn = new_board(), None, "X"
            self.mark = "X" if msg["x"] == me else ("O" if msg["o"] == me else None)
        elif t == "board":                         # ảnh chụp bàn cờ cho người xem đến muộn
            self.board = [list(row) for row in msg["rows"]]
            self.turn = msg["turn"]
        elif t == "moved":
            self.board[msg["r"]][msg["c"]] = msg["mark"]
            self.last = (msg["r"], msg["c"])
            self.turn = msg["next"]                # lượt kế do server quyết định (None: ván đã hết)
        elif t == "over":
            self.turn = None


def connect(a, me):
    host, port = a.host, a.port
    if a.tim:
        print(f"[{me}] Tìm server Caro bằng broadcast tới cổng {a.tim}...", flush=True)
        found = discover(a.tim, 800)
        if not found:
            sys.exit(f"[{me}] Không tìm thấy server nào")
        host, port, name = found[0][0], found[0][1], found[0][2]
        print(f"[{me}] Thấy {name} tại {host}:{port}, kết nối", flush=True)
    s = socket.create_connection((host, port), timeout=3)
    s.settimeout(None)
    s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)    # tắt Nagle: gửi thông điệp nhỏ ngay (Go bật sẵn)
    if not a.tim:
        print(f"[{me}] Đã kết nối {host}:{port}", flush=True)
    return s


# ---------------------------------------------------------------- bot
def quit_room(s, me, games):
    print(f"[{me}] Đã chơi {games} ván, rời phòng", flush=True)
    send_json(s, {"type": "bye"})                     # bye = rời phòng + rời sảnh trong một thông điệp


def run_bot(s, me, a):
    """Bot phản ứng theo từng thông điệp của server.

    Sau mỗi ván, người THUA (hoặc quân O nếu hòa) lên tiếng trước: xin chơi lại hoặc rời phòng; người thắng
    chờ quyết định đó rồi mới trả lời. Nhờ quy ước này hai bot không "nói cùng lúc", nhật ký có thứ tự cố định.
    """
    view, games, tried = View(), 0, set()
    rng = Lcg(a.seed)
    waiting = False                                   # người thắng đang chờ đối thủ quyết định
    in_room = False
    while True:
        msg = recv_json(s)
        if msg is None:
            continue
        t = msg.get("type")
        view.apply(msg, me)
        want_more = games < a.so_van and a.bot != "im_lang"
        if t == "welcome":
            print(f"[{me}] Vào sảnh; số phòng hiện có: {len(msg['rooms'])}", flush=True)
            send_json(s, {"type": "create", "room": a.tao} if a.tao else {"type": "join", "room": a.vao})
        elif t == "joined":
            in_room = True
            print(f"[{me}] Vào phòng {msg['room']}, cầm quân {msg['role']}", flush=True)
        elif t == "error":
            print(f"[{me}] Server báo lỗi {msg['code']}: {msg['msg']}", flush=True)
            if not in_room:                              # không tạo/vào được phòng: bot dừng
                send_json(s, {"type": "bye"})
                return
        elif t == "start":
            print(f"[{me}] Ván {msg['game_no']} bắt đầu: X = {msg['x']}, O = {msg['o']} (tôi cầm {view.mark})",
                  flush=True)
            if a.thu_loi and view.mark == "O" and "early" not in tried:
                tried.add("early")
                print(f"[{me}] (thử lỗi) đánh h8 khi chưa đến lượt", flush=True)
                send_json(s, {"type": "move", "r": 7, "c": 7})
        elif t == "rematch_wait" and msg["by"] != me and waiting:
            waiting = False
            if not want_more:
                return quit_room(s, me, games)
            print(f"[{me}] {msg['by']} xin chơi lại – đồng ý", flush=True)
            send_json(s, {"type": "rematch"})
        elif t == "left":
            print(f"[{me}] {msg['player']} rời phòng", flush=True)
            if waiting:
                return quit_room(s, me, games)
        elif t == "over":
            games += 1
            want_more = games < a.so_van and a.bot != "im_lang"
            print(f"[{me}] {over_text(msg)}", flush=True)
            leader = msg["winner"] == me or (msg["winner"] is None and view.mark == "X")
            if msg["reason"] == "left":                 # đối thủ đã đi: không còn ai để chơi
                return quit_room(s, me, games)
            if leader:
                waiting = True
            elif want_more:
                print(f"[{me}] Xin chơi lại", flush=True)
                send_json(s, {"type": "rematch"})
            else:
                return quit_room(s, me, games)
        if t in ("start", "moved") and view.mark and view.turn == view.mark and a.bot != "im_lang":
            time.sleep(a.tre_ms / 1000)                     # "suy nghĩ" để người xem kịp theo dõi
            if a.thu_loi and view.last and "bad_cells" not in tried:
                tried.add("bad_cells")
                r, c = view.last
                print(f"[{me}] (thử lỗi) đánh vào ô đã có quân {cell_name(r, c)} và ô ngoài bàn (15, 3)",
                      flush=True)
                send_json(s, {"type": "move", "r": r, "c": c})
                send_json(s, {"type": "move", "r": N, "c": 3})
            r, c = choose_move(view.board, view.mark, a.bot, rng)
            send_json(s, {"type": "move", "r": r, "c": c})


# ---------------------------------------------------------------- người xem tự động
def run_watcher(s, me, a):
    view, games = View(), 0
    while True:
        msg = recv_json(s)
        if msg is None:
            continue
        t = msg.get("type")
        view.apply(msg, me)
        if t == "welcome":
            send_json(s, {"type": "watch", "room": a.xem})
        elif t == "joined":
            info = msg["info"]
            print(f"[{me}] Xem phòng {msg['room']} (X = {info['x'] or 'chưa có'}, O = {info['o'] or 'chưa có'})",
                  flush=True)
        elif t == "error":
            print(f"[{me}] Server báo lỗi {msg['code']}: {msg['msg']}", flush=True)
            return
        elif t == "board":
            print(f"[{me}] Nhận ảnh chụp bàn cờ: ván {msg['game_no']}, đã đi {msg['moves']} nước", flush=True)
        elif t == "start":
            print(f"[{me}] Ván {msg['game_no']}: X = {msg['x']}, O = {msg['o']}", flush=True)
        elif t == "moved" and msg["move_no"] % 10 == 0:
            print(f"[{me}] Nước {msg['move_no']}: {msg['by']} ({msg['mark']}) {cell_name(msg['r'], msg['c'])}",
                  flush=True)
        elif t == "over":
            games += 1
            print(f"[{me}] {over_text(msg)}", flush=True)
            for line in render(view.board, view.last):
                print(line, flush=True)
            if msg["line"]:
                print(f"[{me}] Dãy thắng: {line_text(msg)}", flush=True)
        elif t == "left":
            print(f"[{me}] {msg['player']} rời phòng", flush=True)
        elif t == "room_closed":
            print(f"[{me}] Phòng {msg['room']} đã đóng", flush=True)
            send_json(s, {"type": "bye"})
            return



# ---------------------------------------------------------------- người chơi thật
HELP = "Lệnh: ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> ví dụ h8 | lai | roi | thoat"


def run_human(s, me, a):
    view = View()
    quitting = threading.Event()

    def show_board():
        for line in render(view.board, view.last):
            print(line)
        if view.mark:
            print(f"[{me}] Bạn cầm {view.mark}; " + ("ĐẾN LƯỢT BẠN" if view.turn == view.mark
                                                    else "chờ đối thủ"), flush=True)

    def receiver():
        try:
            while True:
                msg = recv_json(s)
                if msg is None:
                    continue
                t = msg.get("type")
                view.apply(msg, me)
                if t in ("welcome", "rooms"):
                    rooms = msg["rooms"]
                    print(f"[{me}] Có {len(rooms)} phòng" + (":" if rooms else ""), flush=True)
                    for r in rooms:
                        print(f"    {r['room']}: X = {r['x'] or 'chưa có'}, O = {r['o'] or 'chưa có'}, "
                              f"{r['watchers']} người xem, {r['state']}", flush=True)
                elif t == "joined":
                    print(f"[{me}] Vào phòng {msg['room']} với vai trò {msg['role']}", flush=True)
                elif t == "start":
                    print(f"[{me}] Ván {msg['game_no']}: X = {msg['x']}, O = {msg['o']}; "
                          f"mỗi nước tối đa {msg['timeout_ms'] // 1000} s", flush=True)
                    show_board()
                elif t in ("moved", "board"):
                    show_board()
                elif t == "over":
                    print(f"[{me}] {over_text(msg)}" + (f"; dãy {line_text(msg)}" if msg["line"] else ""))
                    print(f"[{me}] Gõ 'lai' để xin chơi lại hoặc 'roi' để rời phòng", flush=True)
                elif t == "rematch_wait":
                    print(f"[{me}] {msg['by']} xin chơi lại", flush=True)
                elif t == "left":
                    print(f"[{me}] {msg['player']} rời phòng", flush=True)
                elif t == "room_closed":
                    print(f"[{me}] Phòng {msg['room']} đã đóng", flush=True)
                elif t == "error":
                    print(f"[{me}] Lỗi {msg['code']}: {msg['msg']}", flush=True)
        except (ConnectionClosed, ProtocolError, OSError):
            if not quitting.is_set():                 # người dùng gõ thoat: lỗi đọc sau đó là bình thường
                print(f"[{me}] Mất kết nối tới server", flush=True)

    threading.Thread(target=receiver, daemon=True).start()
    print(f"[{me}] {HELP}", flush=True)
    if a.tao or a.vao:
        send_json(s, {"type": "create", "room": a.tao} if a.tao else {"type": "join", "room": a.vao})
    for line in sys.stdin:
        parts = line.split()
        if not parts:
            continue
        cmd, arg = parts[0].lower(), (parts[1] if len(parts) > 1 else "")
        if cmd == "thoat":
            break
        elif cmd == "ds":
            send_json(s, {"type": "list"})
        elif cmd in ("tao", "vao", "xem"):
            kind = {"tao": "create", "vao": "join", "xem": "watch"}[cmd]
            send_json(s, {"type": kind, "room": arg})
        elif cmd == "lai":
            send_json(s, {"type": "rematch"})
        elif cmd == "roi":
            send_json(s, {"type": "leave"})
        elif parse_cell(cmd):
            r, c = parse_cell(cmd)
            if view.board[r][c] != EMPTY:            # kiểm tra sớm phía client; server vẫn kiểm tra lại
                print(f"[{me}] Ô {cmd} đã có quân", flush=True)
                continue
            send_json(s, {"type": "move", "r": r, "c": c})
        else:
            print(f"[{me}] {HELP}", flush=True)
    quitting.set()
    send_json(s, {"type": "bye"})


def main():
    ap = argparse.ArgumentParser(description="Client Caro")
    ap.add_argument("name")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=9100)
    ap.add_argument("--tim", type=int, metavar="CỔNG", help="tìm server bằng UDP broadcast tới cổng này")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--tao", metavar="PHÒNG")
    g.add_argument("--vao", metavar="PHÒNG")
    g.add_argument("--xem", metavar="PHÒNG")
    ap.add_argument("--bot", choices=list(STYLES) + ["im_lang"])
    ap.add_argument("--seed", type=int, default=1, help="hạt giống của bot tap_su")
    ap.add_argument("--thu-loi", action="store_true")
    ap.add_argument("--so-van", type=int, default=1)
    ap.add_argument("--tre-ms", type=int, default=300)
    a = ap.parse_args()
    me = a.name
    if a.bot and not (a.tao or a.vao):
        sys.exit("Bot cần --tao hoặc --vao")
    try:
        s = connect(a, me)
    except OSError as e:
        sys.exit(f"[{me}] Không kết nối được: {e}")
    send_json(s, {"type": "hello", "name": me})
    try:
        if a.xem:
            run_watcher(s, me, a)
        elif a.bot:
            run_bot(s, me, a)
        else:
            run_human(s, me, a)
    except (ConnectionClosed, ProtocolError, OSError):
        print(f"[{me}] Server đóng kết nối", flush=True)
    except KeyboardInterrupt:
        pass
    finally:
        s.close()


if __name__ == "__main__":
    main()
