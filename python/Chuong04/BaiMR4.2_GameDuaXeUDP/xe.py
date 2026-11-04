"""Bài mở rộng MR4.2 – Client đua xe UDP: người lái (bàn phím) hoặc bot.

Mỗi khi nhận ảnh chụp tick k (chỉ nhận ảnh MỚI HƠN ảnh đang có), client:
  1. đối chiếu: so vị trí xe mình mà client đã dự đoán với vị trí server gửi (lệch khi đầu vào bị mất);
  2. dự đoán phía client: từ ảnh chụp k, chạy lại vật lý với các đầu vào của chính mình để biết xe
     sẽ ở đâu ở tick k+1 (xe khác thì ngoại suy theo vận tốc cuối);
  3. chọn đầu vào cho tick k+2 và gửi kèm 2 đầu vào trước đó (dư thừa).
Khi một ảnh chụp bị mất, khung hình của tick đó được vẽ bằng dự đoán – người chơi không thấy xe "đứng hình".

Chạy:  python3 xe.py <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]
       Người lái: gõ g (ga), n (nhả), p (phanh) rồi Enter; đầu vào giữ nguyên tới khi đổi.
"""
import argparse
import json
import socket
import sys
import threading
import time

from dua_xe import (INPUT_TEXT, LEAD, START, STYLES, TICK_MS, TRACK, WINDOW, bot_input, fmt2, step,
                    track_line)


class RaceView:
    """Những gì client biết về cuộc đua: ảnh chụp mới nhất, đầu vào của mình, thống kê."""

    def __init__(self, me):
        self.me = me
        self.my_id = None
        self.last_tick = -1
        self.snap = None
        self.my_inputs = {}                # tick -> đầu vào mình đã chọn
        self.got = self.stale = self.missed = self.max_gap = 0
        self.err_n = self.err_max = 0      # dự đoán xe mình sai (server đã dùng đầu vào khác)
        self.oerr_n = self.oerr_max = 0    # ngoại suy xe khác sai sau một quãng hụt ảnh chụp

    def input_at(self, t):
        """Đầu vào mình dùng ở tick t (nếu chưa chọn thì giữ đầu vào gần nhất – giống server)."""
        while t >= 0:
            if t in self.my_inputs:
                return self.my_inputs[t]
            t -= 1
        return 0

    def car(self, snap, cid):
        return next(c for c in snap["cars"] if c[0] == cid)

    def predict_own(self, snap, T):
        """Dự đoán phía client: chạy lại vật lý từ ảnh chụp với chính các đầu vào của mình."""
        _, x, v, done = self.car(snap, self.my_id)
        for t in range(snap["tick"] + 1, T + 1):
            if t < START or done >= 0:
                continue
            x, v, _ = step(x, v, self.input_at(t))
            if x >= TRACK:
                x, v, done = TRACK, 0, t - START + 1
        return x, v, done

    def predict_other(self, snap, cid, T):
        """Xe khác: ngoại suy tuyến tính (dead reckoning) theo vận tốc trong ảnh chụp."""
        _, x, v, done = self.car(snap, cid)
        ticks = T - max(snap["tick"], START - 1)
        if done >= 0 or ticks <= 0:
            return x, v, done
        return min(TRACK, x + v * ticks), v, done

    def state_at(self, T):
        """Trạng thái mọi xe ở tick T theo hiểu biết hiện tại (ảnh chụp thật nếu có, không thì dự đoán)."""
        s = self.snap
        out = []
        for cid, name in enumerate(s["names"]):
            if s["tick"] == T:
                _, x, v, done = self.car(s, cid)
            elif cid == self.my_id:
                x, v, done = self.predict_own(s, T)
            else:
                x, v, done = self.predict_other(s, cid, T)
            out.append((name, x, v, done))
        return out

    def frame(self, T, predicted):
        lines = [f"[{self.me}] Khung t={T - START}" + (" (dự đoán: thiếu ảnh chụp)" if predicted else "")]
        for name, x, v, done in self.state_at(T):
            tail = f" về đích ({done} tick)" if done >= 0 else f" x={fmt2(x)} v={fmt2(v)}"
            lines.append(f"  {name[0].upper()}|{track_line(name[0].upper(), x)}{tail}")
        return lines

    def accept(self, s):
        """Nhận một ảnh chụp; trả False nếu là ảnh cũ (đến muộn) – bị bỏ, ảnh mới nhất thắng."""
        k = s["tick"]
        if k <= self.last_tick:
            self.stale += 1
            return False
        if self.my_id is None:
            self.my_id = s["names"].index(self.me)
        if self.last_tick >= 0 and k - self.last_tick > 1:
            gap = k - self.last_tick - 1
            self.missed += gap
            self.max_gap = max(self.max_gap, gap)
            if k >= START:                                 # xe khác: ngoại suy trong quãng hụt lệch bao nhiêu?
                for cid in range(len(s["names"])):
                    if cid != self.my_id:
                        err = abs(self.predict_other(self.snap, cid, k)[0] - self.car(s, cid)[1])
                        if err:
                            self.oerr_n += 1
                            self.oerr_max = max(self.oerr_max, err)
        if self.snap is not None and k >= START:          # đối chiếu dự đoán với sự thật của server
            px, _, _ = self.predict_own(self.snap, k)
            err = abs(px - self.car(s, self.my_id)[1])
            if err:
                self.err_n += 1
                self.err_max = max(self.err_max, err)
        self.got += 1
        return True

    def decide(self, choose):
        """Chọn đầu vào cho tick k+LEAD, trả gói đầu vào cần gửi (mang kèm WINDOW-1 đầu vào trước đó)."""
        k = self.snap["tick"]
        T = k + LEAD
        x, v, done = self.predict_own(self.snap, T - 1)
        inp = choose(x, v, done)
        start = (max(self.my_inputs) + 1) if self.my_inputs else T
        for t in range(start, T):                          # tick bị bỏ qua (do mất ảnh chụp): giữ đầu vào cũ
            self.my_inputs[t] = self.input_at(t)
        self.my_inputs[T] = inp
        return {"t": "in", "seq": T, "in": [self.input_at(T - j) for j in range(WINDOW)]}


def send(sock, addr, obj):
    sock.sendto(json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8"), addr)


def join(sock, addr, me):
    """Gửi join (lặp lại mỗi 250 ms vì UDP có thể mất) tới khi nhận welcome."""
    sock.settimeout(0.25)
    while True:
        send(sock, addr, {"t": "join", "name": me})
        try:
            data, _ = sock.recvfrom(4096)
        except socket.timeout:
            continue
        p = json.loads(data.decode("utf-8"))
        if p.get("t") == "welcome":
            return p


def race_once(sock, addr, me, a, human):
    w = join(sock, addr, me)
    print(f"[{me}] Vào cuộc đua {w['race']} (cần {w['need']} xe; mất gói giả lập {w['mat']} %, "
          f"đảo thứ tự {w['dao']} %)", flush=True)
    view = RaceView(me)
    next_frame = START
    sock.settimeout(0.05 if human else 3.0)
    last_draw = 0.0
    while True:
        try:
            data, _ = sock.recvfrom(4096)
            p = json.loads(data.decode("utf-8"))
        except socket.timeout:
            if not human:
                print(f"[{me}] Mất liên lạc với server", flush=True)
                return
            p = None
        if p and p.get("t") == "snap" and p.get("race") == w["race"] and view.accept(p):
            k = p["tick"]
            if human:
                human["t_last"] = time.monotonic()
            if not human and a.khung > 0 and p["phase"] == "DUA" and view.snap is not None:
                while next_frame <= k:                     # khung cho mọi tick cần vẽ đến hết tick k
                    if next_frame == k:
                        view.snap = p
                        out = view.frame(k, False)
                    else:
                        out = view.frame(next_frame, True)  # dùng ảnh chụp cũ + dự đoán
                    print("\n".join(out), flush=True)
                    next_frame += a.khung
            view.snap, view.last_tick = p, k
            if p["phase"] == "XONG":
                rank = ", ".join(f"{i + 1}. {n} {d if d >= 0 else '-'} tick" for i, (n, d) in enumerate(p["rank"]))
                print(f"[{me}] Kết quả: {rank}", flush=True)
                print(f"[{me}] Ảnh chụp: nhận {view.got}, hụt {view.missed} tick (dài nhất {view.max_gap}), "
                      f"bỏ {view.stale} ảnh cũ", flush=True)
                print(f"[{me}] Dự đoán xe mình: lệch {view.err_n} lần (lớn nhất {fmt2(view.err_max)} ô); "
                      f"ngoại suy xe khác: lệch {view.oerr_n} lần (lớn nhất {fmt2(view.oerr_max)} ô)", flush=True)
                return
            if human:
                choose = lambda x, v, done: human["inp"]
            else:
                choose = lambda x, v, done: bot_input(x, v, a.bot) if done < 0 else 0
            send(sock, addr, view.decide(choose))
        if human and view.snap and time.monotonic() - last_draw >= 0.1:   # vẽ lại 10 lần/giây
            last_draw = time.monotonic()
            est = view.last_tick + int((time.monotonic() - human["t_last"]) * 1000 // TICK_MS)
            if est >= START:
                print("\033[H\033[2J" + "\n".join(view.frame(est, est != view.last_tick)), flush=True)
                print(f"Đầu vào: {INPUT_TEXT[human['inp']]}  (g = ga, n = nhả, p = phanh, Enter)", flush=True)
            else:
                print(f"\033[H\033[2JXuất phát sau {(START - est) * TICK_MS / 1000:.1f} s", flush=True)


def keyboard(state):
    """Luồng đọc bàn phím: g/n/p + Enter đổi đầu vào hiện tại."""
    for line in sys.stdin:
        c = line.strip().lower()[:1]
        if c in ("g", "n", "p"):
            state["inp"] = {"g": 1, "n": 0, "p": -1}[c]


def main():
    ap = argparse.ArgumentParser(description="Client đua xe UDP")
    ap.add_argument("name")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("port", nargs="?", type=int, default=9200)
    ap.add_argument("--bot", choices=list(STYLES))
    ap.add_argument("--khung", type=int, default=10, help="bot in một khung mỗi bao nhiêu tick (0 = không in)")
    ap.add_argument("--so-van", type=int, default=1)
    a = ap.parse_args()
    me = a.name
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    addr = (a.host, a.port)
    human = None
    if not a.bot:
        human = {"inp": 0, "t_last": time.monotonic()}
        threading.Thread(target=keyboard, args=(human,), daemon=True).start()
    try:
        for _ in range(a.so_van):
            race_once(sock, addr, me, a, human)
    except KeyboardInterrupt:
        pass
    finally:
        sock.close()


if __name__ == "__main__":
    main()
