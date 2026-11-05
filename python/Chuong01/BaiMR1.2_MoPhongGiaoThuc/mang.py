"""Bài mở rộng MR1.2 – Bộ mô phỏng mạng theo sự kiện rời rạc (discrete-event simulation).

Mô-đun này là "lõi" của bộ mô phỏng: topo mạng, hàng đợi sự kiện, định tuyến theo chi phí
(Dijkstra) và quy tắc chuyển tiếp của từng loại nút. Không dùng socket thật: mỗi gói tin là một
đối tượng Python, "thời gian" là đồng hồ mô phỏng tính bằng micro giây (µs).

Topo (hai đoạn LAN nối qua ba router):

    H1 ─┐                                   ┌─ H4
    H2 ─┼─ SW1 ── R1 ── R2 ── R3 ── SW2 ─┼─ H5      (R1 ── R3: đường dự phòng, chi phí 3)
    H3 ─┘   LAN A 192.168.1.0/24   LAN B 192.168.2.0/24  └─ H6

Quy tắc chuyển tiếp:
- host: nhận gói nếu gói dành cho mình (unicast đúng tên, quảng bá trong LAN, nhóm đã tham gia,
  địa chỉ anycast mình phục vụ); host không chuyển tiếp gói của người khác;
- switch: quảng bá → gửi tràn (flood) mọi cổng trừ cổng vào; multicast → chỉ cổng có thành viên
  (IGMP snooping) và cổng router; unicast → đúng một cổng theo bảng định tuyến;
- router: chặn quảng bá (biên của miền quảng bá); unicast/anycast → giảm TTL, chọn chặng kế theo
  chi phí nhỏ nhất; multicast → chỉ chuyển tới các LAN có thành viên.
"""
import copy
import heapq

M32 = 0xFFFFFFFF


def fmt_t(us):
    """Định dạng thời gian mô phỏng (µs) thành mili giây, một chữ số thập phân."""
    return f"{us // 1000}.{us % 1000 // 100}"


class Rng:
    """Bộ sinh số giả ngẫu nhiên xorshift32: cùng hạt giống (seed) → cùng dãy số ở Python và Go."""

    def __init__(self, seed):
        self.x = (seed * 2654435761 + 1) & M32 or 1

    def next(self):
        x = self.x
        x ^= (x << 13) & M32
        x ^= x >> 17
        x ^= (x << 5) & M32
        self.x = x
        return x

    def below(self, n):
        """Số nguyên ngẫu nhiên trong [0, n)."""
        return self.next() % n


class Node:
    def __init__(self, name, kind, ip, lan, x, y):
        self.name, self.kind, self.ip, self.lan = name, kind, ip, lan
        self.x, self.y = x, y                      # tọa độ để vẽ hoạt hình HTML
        self.up = True
        self.sent = self.recv = self.deliv = self.fwd = self.drop = 0


class Link:
    def __init__(self, a, b, cost, delay, bw):
        self.a, self.b = a, b
        self.cost = cost                           # chi phí định tuyến (số bước, có trọng số)
        self.delay = delay                         # trễ lan truyền (µs)
        self.bw = bw                               # băng thông: byte mỗi mili giây
        self.up = True
        self.loss = 0                              # xác suất mất gói, phần nghìn
        self.jitter = 0                            # trễ ngẫu nhiên thêm tối đa (µs)
        self.free = {a: 0, b: 0}                   # thời điểm mỗi chiều rảnh (hàng đợi FIFO)


class Packet:
    def __init__(self, cast, kind, src, dst, label, size=100, data=None):
        self.cast = cast       # uni | bc | mc | igmp | any
        self.kind = kind       # nhãn hiển thị: TCP, UDP, UNI, BC, MC, IGMP, ANY
        self.src, self.dst = src, dst
        self.label = label
        self.size = size
        self.ttl = 64
        self.data = data or {}
        self.drop_on = None    # (a, b): kịch bản ép mất gói trên liên kết a→b


# Topo: (tên, loại, địa chỉ IP, LAN, x, y)
NODES = [
    ("H1", "host", "192.168.1.11", "A", 60, 80), ("H2", "host", "192.168.1.12", "A", 60, 210),
    ("H3", "host", "192.168.1.13", "A", 60, 340), ("SW1", "switch", "-", "A", 190, 210),
    ("R1", "router", "192.168.1.1", "A", 330, 210), ("R2", "router", "10.0.0.2", "-", 450, 80),
    ("R3", "router", "192.168.2.1", "B", 570, 210), ("SW2", "switch", "-", "B", 710, 210),
    ("H4", "host", "192.168.2.14", "B", 840, 80), ("H5", "host", "192.168.2.15", "B", 840, 210),
    ("H6", "host", "192.168.2.16", "B", 840, 340),
]
LAN_US, WAN_US = 500, 5000          # trễ lan truyền: trong LAN 0,5 ms; giữa router 5 ms
LAN_BW, WAN_BW = 12500, 1250        # 100 Mbit/s và 10 Mbit/s (byte/ms)
# Liên kết: (a, b, chi phí, trễ µs, băng thông)
LINKS = [
    ("H1", "SW1", 1, LAN_US, LAN_BW), ("H2", "SW1", 1, LAN_US, LAN_BW),
    ("H3", "SW1", 1, LAN_US, LAN_BW), ("SW1", "R1", 1, LAN_US, LAN_BW),
    ("R1", "R2", 1, WAN_US, WAN_BW), ("R2", "R3", 1, WAN_US, WAN_BW),
    ("R1", "R3", 3, 8000, WAN_BW), ("R3", "SW2", 1, LAN_US, LAN_BW),
    ("SW2", "H4", 1, LAN_US, LAN_BW), ("SW2", "H5", 1, LAN_US, LAN_BW),
    ("SW2", "H6", 1, LAN_US, LAN_BW),
]


class Net:
    def __init__(self, seed):
        self.rng = Rng(seed)
        self.now, self.q, self.seq = 0, [], 0
        self.order = [n[0] for n in NODES]
        self.nodes = {n[0]: Node(*n) for n in NODES}
        self.adj = {n: [] for n in self.order}
        self.links = {}
        for a, b, cost, delay, bw in LINKS:
            L = Link(a, b, cost, delay, bw)
            self.links[(a, b)] = self.links[(b, a)] = L
            self.adj[a].append((b, L))
            self.adj[b].append((a, L))
        self.anycast = {}        # địa chỉ anycast -> danh sách host phục vụ
        self.snoop = {}          # (switch, nhóm) -> danh sách host thành viên (IGMP snooping)
        self.mroute = {}         # nhóm -> danh sách switch (LAN) có thành viên (router biết)
        self.member = {n: [] for n in self.order}  # host -> các nhóm đã tham gia
        self.hop_trace = True    # in từng bước chuyển (tắt với TCP/UDP cho gọn)
        self.quiet = False       # không in nhật ký (dùng khi chỉ cần chỉ số, ví dụ lệnh so_sanh)
        self.on_deliver = None   # hàm(node, pkt) – tầng ứng dụng của kịch bản
        self.on_loss = None      # hàm(pkt, a, b) – kịch bản được báo khi mất gói
        self.hops, self.notes = [], []   # dữ liệu cho hoạt hình HTML
        self.changes = []                # thay đổi trạng thái nút/liên kết (hỏng, phục hồi)
        self.compute_routes()

    # ---------- hàng đợi sự kiện ----------
    def at(self, t, fn):
        self.seq += 1
        heapq.heappush(self.q, (t, self.seq, fn))

    def run(self):
        while self.q:
            t, _, fn = heapq.heappop(self.q)
            self.now = t
            fn()

    def log(self, text, node=""):
        if not self.quiet:
            print(f"{fmt_t(self.now):>7} ms  {text}")
        self.notes.append([self.now, node, text])

    # ---------- định tuyến: Dijkstra từ mọi nút ----------
    def compute_routes(self):
        self.dist, self.nh = {}, {}
        for s in self.order:
            if not self.nodes[s].up:
                continue
            dist, first, done = {s: 0}, {s: None}, set()
            while True:
                u = None
                for n in self.order:              # chọn nút chưa xét có khoảng cách nhỏ nhất
                    if n in dist and n not in done and (u is None or dist[n] < dist[u]):
                        u = n
                if u is None:
                    break
                done.add(u)
                if u != s and self.nodes[u].kind == "host":
                    continue                      # host không chuyển tiếp cho nút khác
                for v, L in self.adj[u]:
                    if not L.up or not self.nodes[v].up:
                        continue
                    d = dist[u] + L.cost
                    if v not in dist or d < dist[v]:
                        dist[v] = d
                        first[v] = v if u == s else first[u]
            self.dist[s], self.nh[s] = dist, first

    def path(self, a, b):
        p = [a]
        while p[-1] != b and self.nh.get(p[-1], {}).get(b):
            p.append(self.nh[p[-1]][b])
        return p

    def target(self, node, pkt):
        """Đích thật của gói: với anycast là máy phục vụ gần nhất (chi phí nhỏ nhất) còn hoạt động."""
        if pkt.cast != "any":
            return pkt.dst
        best = None
        for h in self.anycast.get(pkt.dst, []):
            d = self.dist.get(node, {}).get(h)
            if self.nodes[h].up and d is not None and (best is None or d < best[0]):
                best = (d, h)
        return best[1] if best else None

    # ---------- gửi, truyền trên liên kết, nhận ----------
    def send(self, node, pkt):
        """Host `node` phát gói `pkt` vào mạng."""
        self.nodes[node].sent += 1
        nbr = self.adj[node][0][0]                # host chỉ có một liên kết (tới switch)
        self.transmit(node, nbr, pkt)

    def transmit(self, a, b, pkt):
        L = self.links[(a, b)]
        if not L.up or not self.nodes[b].up:
            self.nodes[a].drop += 1
            if self.hop_trace:
                self.log(f"{a}: liên kết tới {b} không hoạt động, bỏ gói", a)
            return
        p = copy.copy(pkt)
        start = max(self.now, L.free[a])
        tx = p.size * 1000 // L.bw                  # thời gian đẩy gói lên đường truyền
        L.free[a] = start + tx
        arr = start + tx + L.delay
        if L.jitter:
            arr += self.rng.below(L.jitter + 1)
        lost = False
        if p.drop_on == (a, b):
            lost, p.drop_on = True, None
        elif L.loss and self.rng.below(1000) < L.loss:
            lost = True
        self.hops.append([start, arr, a, b, p.kind, p.label, lost])
        if lost:
            self.at((start + arr) // 2, lambda: self.lost(p, a, b))
        else:
            self.at(arr, lambda: self.arrive(b, p, a))

    def lost(self, pkt, a, b):
        self.nodes[a].drop += 1
        if self.on_loss:
            self.on_loss(pkt, a, b)
        else:
            self.log(f"× {pkt.label}: mất trên {a}→{b}", a)

    def arrive(self, node, pkt, frm):
        N = self.nodes[node]
        N.recv += 1
        if self.hop_trace:
            self.log(f"{frm:>3} → {node:<3} {pkt.label}", node)
        if N.kind == "host":
            self.host_in(N, pkt)
        elif N.kind == "switch":
            self.switch_in(N, pkt, frm)
        else:
            self.router_in(N, pkt, frm)

    def host_in(self, N, pkt):
        if pkt.cast == "uni":
            mine = pkt.dst == N.name
        elif pkt.cast == "any":
            mine = N.name in self.anycast.get(pkt.dst, [])
        elif pkt.cast == "bc":
            mine = True                               # khung quảng bá tới được thì thuộc LAN này
        elif pkt.cast == "mc":
            mine = pkt.dst in self.member[N.name]
        else:
            mine = False
        if mine:
            N.deliv += 1
            if self.on_deliver:
                self.on_deliver(N.name, pkt)
        else:
            N.drop += 1
            if self.hop_trace:
                self.log(f"{N.name} bỏ qua: gói không dành cho mình", N.name)

    def switch_in(self, N, pkt, frm):
        ports = [v for v, _ in self.adj[N.name]]
        if pkt.cast == "bc":
            outs = [v for v in ports if v != frm]                     # gửi tràn
        elif pkt.cast == "igmp":
            key = (N.name, pkt.dst)
            mem = self.snoop.setdefault(key, [])
            if pkt.data.get("join") and frm not in mem:
                mem.append(frm)
            if not pkt.data.get("join") and frm in mem:
                mem.remove(frm)
            outs = [v for v in ports if self.nodes[v].kind == "router"]
        elif pkt.cast == "mc":
            mem = self.snoop.get((N.name, pkt.dst), [])
            outs = [v for v in ports if v != frm and (v in mem or self.nodes[v].kind == "router")]
        else:
            t = self.target(N.name, pkt)
            nh = self.nh[N.name].get(t) if t else None
            outs = [nh] if nh else []
        for v in outs:
            N.fwd += 1
            self.transmit(N.name, v, pkt)

    def router_in(self, N, pkt, frm):
        if pkt.cast == "bc":
            N.drop += 1
            self.log(f"{N.name} chặn quảng bá: không chuyển ra ngoài LAN {self.nodes[frm].lan}", N.name)
            return
        if pkt.cast == "igmp":
            N.deliv += 1
            lans = self.mroute.setdefault(pkt.dst, [])
            if pkt.data.get("join") and frm not in lans:
                lans.append(frm)
                self.log(f"{N.name} ghi nhận: LAN {self.nodes[frm].lan} có thành viên {pkt.dst}", N.name)
            elif not pkt.data.get("join") and not self.snoop.get((frm, pkt.dst)) and frm in lans:
                lans.remove(frm)
                self.log(f"{N.name} ghi nhận: LAN {self.nodes[frm].lan} hết thành viên {pkt.dst}", N.name)
            return
        pkt.ttl -= 1
        if pkt.ttl <= 0:
            N.drop += 1
            self.log(f"{N.name} bỏ gói: TTL = 0", N.name)
            return
        if pkt.cast == "mc":
            outs = []
            for sw in self.mroute.get(pkt.dst, []):
                nh = self.nh[N.name].get(sw)
                if nh and nh != frm and nh not in outs:
                    outs.append(nh)
            if not outs:
                self.log(f"{N.name} không chuyển tiếp: phía sau không có thành viên", N.name)
        else:
            t = self.target(N.name, pkt)
            nh = self.nh[N.name].get(t) if t else None
            outs = [nh] if nh else []
            if not outs:
                N.drop += 1
                self.log(f"{N.name} bỏ gói: không có đường tới {pkt.dst}", N.name)
        for v in outs:
            N.fwd += 1
            self.transmit(N.name, v, pkt)

    # ---------- tiện ích cho kịch bản ----------
    def set_link(self, a, b, up):
        self.links[(a, b)].up = up
        self.changes.append([self.now, "link", a + "-" + b, up])
        self.compute_routes()

    def set_node(self, n, up):
        self.nodes[n].up = up
        self.changes.append([self.now, "node", n, up])
        self.compute_routes()

    def summary(self):
        print("Tổng kết theo nút (Gửi = phát ra, Nhận = gói tới, Giao = đưa lên")
        print("ứng dụng, Ch.tiếp = số bản sao chuyển đi, Bỏ = bỏ qua/chặn/mất):")
        print(f"  {'Nút':<4} {'Địa chỉ':<13} {'Gửi':>4} {'Nhận':>5} {'Giao':>5} {'Ch.tiếp':>8} {'Bỏ':>4}")
        for n in self.order:
            N = self.nodes[n]
            print(f"  {n:<4} {N.ip:<13} {N.sent:>4} {N.recv:>5} {N.deliv:>5} {N.fwd:>8} {N.drop:>4}")
