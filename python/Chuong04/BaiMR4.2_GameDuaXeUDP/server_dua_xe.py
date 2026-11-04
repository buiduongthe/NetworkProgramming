"""Bài mở rộng MR4.2 – Server đua xe thời gian thực trên UDP (server có thẩm quyền, vòng tick 20 Hz).

Mô hình:
  - Client gửi ĐẦU VÀO (ga/nhả/phanh) kèm số thứ tự = tick mà đầu vào áp dụng; mỗi gói mang kèm 3 đầu vào
    gần nhất nên mất một gói vẫn còn bản sao ở gói sau.
  - Server chạy vòng lặp tick cố định 50 ms: lấy đầu vào của tick hiện tại cho từng xe; nếu không có
    (mọi bản sao đều mất) thì GIỮ đầu vào cũ; mô phỏng vật lý; gửi ẢNH CHỤP trạng thái tới mọi client.
  - Ảnh chụp mang số tick: client chỉ giữ ảnh mới nhất, bỏ ảnh đến muộn (cũ hơn).
  - --mat, --dao giả lập mất gói và đảo thứ tự (theo hàm băm có hạt giống nên tái lập được).
    Danh sách "0,20" nghĩa là cuộc đua 1 mất 0 %, cuộc đua 2 mất 20 %.

Chạy:  python3 server_dua_xe.py [host] [port] [--so-xe 2] [--mat 0] [--dao 0] [--seed 1] [--so-van 0]
"""
import argparse
import json
import select
import socket
import time

from dua_xe import (END_TICKS, MAX_RACE, START, TICK_MS, TRACK, V_CUA, fmt2, lossy, step)

DIR_IN, DIR_SNAP, DIR_REORDER = 0, 1, 2


class Car:
    def __init__(self, name, addr):
        self.name, self.addr = name, addr
        self.id = -1
        self.x = self.v = 0
        self.done = -1                     # số tick từ lúc xuất phát đến khi về đích (-1: chưa)
        self.inputs = {}                   # tick -> đầu vào đã nhận (bộ đệm đầu vào)
        self.last_in = 0                   # đầu vào áp dụng gần nhất (dùng lại khi thiếu)
        self.recv = self.dropped = self.held = self.crashes = 0
        self.snap_sent = self.snap_dropped = self.snap_reordered = 0
        self.delayed = None                # ảnh chụp đang bị giữ lại để gửi muộn (giả lập đảo thứ tự)


def pct_list(text):
    return [int(p) for p in text.split(",")]


class RaceServer:
    def __init__(self, a, sock):
        self.a, self.sock = a, sock
        self.mat, self.dao = pct_list(a.mat), pct_list(a.dao)
        self.race = 0
        self.reset()

    def reset(self):
        self.cars = {}                     # addr -> Car
        self.state = "CHO"                 # CHO -> DUA (gồm đếm ngược) -> XONG
        self.tick = -1
        self.end_tick = None
        self.next_tick = None

    def send(self, addr, obj):
        self.sock.sendto(json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8"), addr)

    # ----- nhận gói -----
    def on_packet(self, data, addr):
        try:
            p = json.loads(data.decode("utf-8"))
        except ValueError:
            return                                       # datagram hỏng: bỏ qua
        if not isinstance(p, dict):
            return
        if p.get("t") == "join":
            self.on_join(p, addr)
        elif p.get("t") == "in":
            self.on_input(p, addr)

    def on_join(self, p, addr):
        name = p.get("name")
        if not isinstance(name, str) or not 1 <= len(name) <= 12:
            return
        car = self.cars.get(addr)
        if car is None or self.state == "XONG":
            if self.state != "CHO" or any(c.name == name for c in self.cars.values()):
                return self.send(addr, {"t": "busy"})     # đang đua hoặc trùng tên: client thử lại sau
            car = self.cars[addr] = Car(name, addr)
            print(f"[server] Thêm một xe từ {addr[0]} ({len(self.cars)}/{self.a.so_xe})",
                  flush=True)
        # trả lời cả khi client gửi lại join (welcome trước đó có thể đã mất)
        self.send(addr, {"t": "welcome", "name": car.name, "need": self.a.so_xe, "race": self.race + 1,
                         "mat": self.loss(), "dao": self.reorder()})
        if self.state == "CHO" and len(self.cars) == self.a.so_xe:
            self.start_race()

    def on_input(self, p, addr):
        car = self.cars.get(addr)
        seq, inputs = p.get("seq"), p.get("in")
        if car is None or self.state != "DUA" or not isinstance(seq, int) or not isinstance(inputs, list):
            return
        if lossy(self.a.seed, DIR_IN, car.id, seq, self.loss()):
            car.dropped += 1                             # giả lập: gói này "mất trên đường"
            return
        car.recv += 1
        for j, inp in enumerate(inputs[:3]):             # inputs[j] là đầu vào của tick seq - j
            t = seq - j
            if t > self.tick and inp in (-1, 0, 1):      # chỉ nhận đầu vào cho tick chưa mô phỏng
                car.inputs.setdefault(t, inp)

    # ----- cuộc đua -----
    def loss(self):
        return self.mat[min(self.race, len(self.mat) - 1)]

    def reorder(self):
        return self.dao[min(self.race, len(self.dao) - 1)]

    def ordered(self):
        return sorted(self.cars.values(), key=lambda c: c.id)

    def start_race(self):
        for i, car in enumerate(sorted(self.cars.values(), key=lambda c: c.name)):
            car.id = i                                   # mã xe theo thứ tự tên: không phụ thuộc ai vào trước
        self.state, self.tick = "DUA", -1
        self.next_tick = time.monotonic() + TICK_MS / 1000
        names = ", ".join(c.name for c in self.ordered())
        print(f"[server] Cuộc đua {self.race + 1}: {names}; mất gói {self.loss()} %, "
              f"đảo thứ tự {self.reorder()} %; xuất phát sau {START} tick", flush=True)

    def do_tick(self):
        self.tick += 1
        k = self.tick
        racing = self.state == "DUA" and k >= START
        for car in self.ordered():
            inp = car.inputs.pop(k, None)
            for t in [t for t in car.inputs if t < k]:   # dọn đầu vào quá hạn
                del car.inputs[t]
            if inp is None:
                inp = car.last_in                        # thiếu đầu vào: giữ đầu vào cũ
                if racing and car.done < 0:
                    car.held += 1
            car.last_in = inp
            if not racing or car.done >= 0:
                continue
            car.x, car.v, crashed = step(car.x, car.v, inp)
            if crashed:
                car.crashes += 1
                print(f"[server] t={k - START}: {car.name} văng ở khúc cua (quá {fmt2(V_CUA)} ô/tick)", flush=True)
            if car.x >= TRACK:
                car.x, car.v, car.done = TRACK, 0, k - START + 1
        if self.state == "DUA" and (all(c.done >= 0 for c in self.cars.values()) or k >= START + MAX_RACE):
            self.state, self.end_tick = "XONG", k
            rank = "; ".join(f"{i + 1}. {c.name} {c.done if c.done >= 0 else '-'} tick"
                             for i, c in enumerate(self.ranking()))
            print(f"[server] Cuộc đua {self.race + 1} kết thúc: {rank}", flush=True)
        self.broadcast(k)
        if self.state == "XONG" and k >= self.end_tick + END_TICKS:
            self.finish_race()

    def ranking(self):
        return sorted(self.cars.values(), key=lambda c: (c.done < 0, c.done, -c.x, c.id))

    def broadcast(self, k):
        snap = {"t": "snap", "race": self.race + 1, "tick": k, "phase": self.state,
                "names": [c.name for c in self.ordered()],
                "cars": [[c.id, c.x, c.v, c.done] for c in self.ordered()]}
        if self.state == "XONG":
            snap["rank"] = [[c.name, c.done] for c in self.ranking()]
        data = json.dumps(snap, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        for car in self.ordered():
            prev, car.delayed = car.delayed, None
            if lossy(self.a.seed, DIR_SNAP, car.id, k, self.loss()):
                car.snap_dropped += 1
            elif lossy(self.a.seed, DIR_REORDER, car.id, k, self.reorder()):
                car.delayed = data                       # giữ lại, gửi SAU ảnh chụp tick kế tiếp
                car.snap_reordered += 1
            else:
                self.sock.sendto(data, car.addr)
                car.snap_sent += 1
            if prev is not None:
                self.sock.sendto(prev, car.addr)         # ảnh chụp cũ đến sau ảnh mới: client phải bỏ
                car.snap_sent += 1

    def finish_race(self):
        for c in self.ordered():
            total = c.recv + c.dropped
            print(f"[server] {c.name}: nhận {c.recv}/{total} gói đầu vào (giả lập mất {c.dropped}), "
                  f"{c.held} tick phải giữ đầu vào cũ, văng {c.crashes} lần", flush=True)
        self.race += 1
        self.reset()


def main():
    ap = argparse.ArgumentParser(description="Server đua xe UDP")
    ap.add_argument("host", nargs="?", default="0.0.0.0")
    ap.add_argument("port", nargs="?", type=int, default=9200)
    ap.add_argument("--so-xe", type=int, default=2, help="số xe cần để bắt đầu")
    ap.add_argument("--mat", default="0", help="tỉ lệ mất gói giả lập (%%), ví dụ 0,20 cho hai cuộc đua")
    ap.add_argument("--dao", default="0", help="tỉ lệ ảnh chụp bị đảo thứ tự (%%)")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--so-van", type=int, default=0, help="dừng sau số cuộc đua này (0 = mãi)")
    a = ap.parse_args()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind((a.host, a.port))
    print(f"[server] Đua xe UDP tại {a.host}:{a.port}, {1000 // TICK_MS} tick/s, cần {a.so_xe} xe, "
          f"đường dài {TRACK // 100} ô", flush=True)
    srv = RaceServer(a, sock)
    try:
        while True:
            timeout = None if srv.next_tick is None else max(0.0, srv.next_tick - time.monotonic())
            ready, _, _ = select.select([sock], [], [], timeout)
            if ready:
                try:
                    data, addr = sock.recvfrom(2048)
                    srv.on_packet(data, addr)
                except ConnectionResetError:             # Windows: ICMP "port unreachable" từ client đã thoát
                    pass
            if srv.next_tick is not None and time.monotonic() >= srv.next_tick:
                srv.next_tick += TICK_MS / 1000          # lịch cố định: không trôi theo thời gian xử lý
                srv.do_tick()
                if srv.state == "CHO" and a.so_van and srv.race >= a.so_van:
                    print(f"[server] Đã đua {srv.race} cuộc, dừng server", flush=True)
                    break
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
    finally:
        sock.close()


if __name__ == "__main__":
    main()
