"""Bài mở rộng MR3.3 – Client trò chơi đoán số: người chơi thật (bàn phím) hoặc bot.

Chế độ người chơi: một luồng nhận thông điệp của server và in ra; luồng chính chỉ hỏi số khi đến lượt.
Client cũng kiểm tra đầu vào (phải là số nguyên) để báo lỗi sớm cho người dùng, nhưng server vẫn
kiểm tra lại mọi thứ – server không bao giờ tin client.

Các bot (dùng cho demo tất định):
  chia_doi   – tìm kiếm nhị phân: đoán điểm giữa khoảng [lo, hi] server gửi kèm lượt.
  ngau_nhien – đoán ngẫu nhiên trong [lo, hi] (bộ sinh LCG, hạt giống --seed).
  nghich     – cố tình gửi thông điệp sai để xem server kiểm tra: đoán khi ván chưa bắt đầu,
               khung không phải JSON, đoán khi chưa đến lượt, giá trị là chuỗi, số ngoài khoảng.
  im_lang    – không bao giờ trả lời: bị server tính hết giờ, rồi tự ngắt kết nối.

Chạy:  python3 nguoi_choi.py <tên> [host] [port] [--bot KIỂU] [--seed N] [--nghi-ms 300]
"""
import argparse
import queue
import socket
import sys
import threading
import time

from giao_thuc import ConnectionClosed, Lcg, ProtocolError, recv_json, send_frame, send_json

HINT_TEXT = {"THAP": "thấp quá", "CAO": "cao quá", "DUNG": "ĐÚNG"}


def describe(me, msg):
    """Chuyển thông điệp của server thành dòng chữ cho người chơi; trả None nếu không cần in."""
    t = msg.get("type")
    if t == "welcome":
        return (f"[{me}] Vào phòng chờ: {msg['waiting']}/{msg['need']} người, "
                f"đoán số trong {msg['min']}–{msg['max']}")
    if t == "start":
        return f"[{me}] Ván {msg['round']} bắt đầu, thứ tự: {', '.join(msg['order'])}"
    if t == "turn" and msg["player"] == me:
        return f"[{me}] Đến lượt bạn (lượt {msg['turn_no']}), khoảng {msg['lo']}–{msg['hi']}"
    if t == "result":
        return f"[{me}] {msg['player']} đoán {msg['value']}: {HINT_TEXT.get(msg['hint'], msg['hint'])}"
    if t == "timeout":
        return f"[{me}] {msg['player']} hết giờ, mất lượt"
    if t == "left":
        return f"[{me}] {msg['player']} đã rời trò chơi"
    if t == "error":
        return f"[{me}] Server báo lỗi {msg.get('code')}: {msg.get('msg')}"
    if t == "game_over":
        who = msg["winner"] or "không ai"
        return f"[{me}] Kết thúc: {who} thắng, số bí mật là {msg['secret']} ({msg['turns']} lượt)"
    if t == "bye":
        return f"[{me}] Server kết thúc phiên"
    return None


def say(line):
    if line:
        print(line, flush=True)


# ---------------------------------------------------------------- bot
def run_bot(s, me, kind, seed, think_ms):
    rng = Lcg(seed)
    tricks_done = set()                        # các "trò nghịch" đã làm (mỗi trò một lần)
    while True:
        msg = recv_json(s)
        if msg is None:
            continue
        say(describe(me, msg))
        t = msg.get("type")
        if t in ("game_over", "bye"):
            return
        if kind == "nghich" and t == "welcome":
            print(f"[{me}] (nghịch) đoán khi ván chưa bắt đầu, rồi gửi khung không phải JSON", flush=True)
            send_json(s, {"type": "guess", "value": 1})
            send_frame(s, "xin chào".encode("utf-8"))
        if t == "timeout" and msg["player"] == me and kind == "im_lang":
            print(f"[{me}] (im lặng) bị hết giờ, ngắt kết nối", flush=True)
            return
        if t != "turn":
            continue
        if msg["player"] != me:
            if kind == "nghich" and "out_of_turn" not in tricks_done:
                tricks_done.add("out_of_turn")
                print(f"[{me}] (nghịch) đoán khi đang là lượt của {msg['player']}", flush=True)
                send_json(s, {"type": "guess", "value": msg["lo"]})
            continue
        if kind == "im_lang":
            continue
        lo, hi = msg["lo"], msg["hi"]
        time.sleep(think_ms / 1000)            # "suy nghĩ" để người xem kịp theo dõi
        if kind == "nghich" and "bad_values" not in tricks_done:
            tricks_done.add("bad_values")
            print(f"[{me}] (nghịch) gửi giá trị chuỗi và số ngoài khoảng", flush=True)
            send_json(s, {"type": "guess", "value": "năm mươi"})
            send_json(s, {"type": "guess", "value": hi + 50})
        guess = rng.next_int(lo, hi) if kind == "ngau_nhien" else (lo + hi) // 2
        print(f"[{me}] Đoán {guess}", flush=True)
        send_json(s, {"type": "guess", "value": guess})


# ---------------------------------------------------------------- người chơi thật
def run_human(s, me):
    turns = queue.Queue()                      # luồng nhận báo cho luồng chính: "turn" hoặc "end"

    def receiver():
        try:
            while True:
                msg = recv_json(s)
                if msg is None:
                    continue
                say(describe(me, msg))
                if msg.get("type") == "turn" and msg["player"] == me:
                    turns.put("turn")
                if msg.get("type") in ("game_over", "bye"):
                    break
        except (ConnectionClosed, ProtocolError, OSError):
            pass
        turns.put("end")

    threading.Thread(target=receiver, daemon=True).start()
    echo = not sys.stdin.isatty()              # đầu vào từ tệp/ống: in lại để nhật ký dễ đọc
    while turns.get() == "turn":
        while True:
            print(f"[{me}] Nhập số: ", end="", flush=True)
            line = sys.stdin.readline()
            if not line:                       # hết đầu vào (Ctrl+D / Ctrl+Z): rời trò chơi
                print(flush=True)
                send_json(s, {"type": "bye"})
                return
            line = line.strip()
            if echo:
                print(line, flush=True)
            try:
                value = int(line)              # kiểm tra phía client: chỉ để báo lỗi sớm
            except ValueError:
                print(f"[{me}] Vui lòng nhập một số nguyên", flush=True)
                continue
            send_json(s, {"type": "guess", "value": value})
            break


def main():
    ap = argparse.ArgumentParser(description="Client trò chơi đoán số")
    ap.add_argument("name")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=9000)
    ap.add_argument("--bot", choices=["chia_doi", "ngau_nhien", "nghich", "im_lang"])
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--nghi-ms", type=int, default=300, help="thời gian bot 'suy nghĩ' mỗi lượt")
    a = ap.parse_args()
    me = a.name
    try:
        s = socket.create_connection((a.host, a.port), timeout=3)
    except OSError as e:
        sys.exit(f"[{me}] Không kết nối được {a.host}:{a.port}: {e}")
    s.settimeout(None)                         # đã kết nối: chờ thông điệp không giới hạn
    s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)   # tắt Nagle: gửi thông điệp nhỏ ngay (Go bật sẵn)
    print(f"[{me}] Đã kết nối {a.host}:{a.port}", flush=True)
    send_json(s, {"type": "hello", "name": me})
    try:
        if a.bot:
            run_bot(s, me, a.bot, a.seed, a.nghi_ms)
        else:
            run_human(s, me)
    except (ConnectionClosed, ProtocolError, OSError):
        print(f"[{me}] Server đóng kết nối", flush=True)
    except KeyboardInterrupt:
        pass
    finally:
        s.close()


if __name__ == "__main__":
    main()
