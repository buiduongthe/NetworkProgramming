"""Bài mở rộng MR1.2 – Bên trong máy chủ: đồng bộ, đa luồng, nhóm luồng, bất đồng bộ và phối hợp.

Sáu yêu cầu từ các client (H1, H2, H3 ở LAN A; H4, H6 ở LAN B) đi qua mạng mô phỏng tới máy chủ H5.
Mỗi yêu cầu cần đọc CSDL 30 ms (chờ I/O: luồng chờ nhưng không tốn CPU) rồi xử lý 10 ms CPU;
máy chủ có 2 lõi CPU. Cùng khối việc đó, máy chủ được tổ chức theo năm mô hình:

  dong_bo      một luồng, xử lý lần lượt: yêu cầu sau phải chờ yêu cầu trước xong hẳn;
  da_luong     mỗi yêu cầu một luồng (tạo luồng 1 ms); luồng nhiều hơn lõi thì phải chờ lõi;
               cập nhật bộ đếm chung trong đoạn găng có khóa (3 ms) – luồng khác phải chờ khóa;
  nhom_luong   nhóm 2 luồng thợ + hàng đợi tối đa 2 yêu cầu; hàng đợi đầy thì từ chối ngay (503);
  bat_dong_bo  một luồng chạy vòng lặp sự kiện: I/O không chặn (nhiều I/O chờ cùng lúc), nhưng phần
               CPU chạy ngay trên vòng lặp nên vòng lặp bị chặn – nhịp tim 10 ms chạy muộn;
  phoi_hop     vòng lặp sự kiện lo I/O, đẩy phần CPU sang nhóm 2 luồng (run_in_executor); luồng thợ
               trả kết quả về vòng lặp (call_soon_threadsafe) – vòng lặp luôn rảnh, nhịp tim đúng giờ.

Mô hình giảng dạy đã đơn giản hóa: lõi CPU cấp theo thứ tự đến, không chia lát thời gian (không
preemption); đoạn găng chỉ tính thời gian giữ khóa; thời gian chuyển ngữ cảnh bằng 0.
"""
from collections import deque

from mang import Packet, fmt_t

MS = 1000
IO_US = 30 * MS          # đọc CSDL: chờ I/O
CPU_US = 10 * MS         # xử lý: dùng một lõi CPU
KHOA_US = 3 * MS         # đoạn găng (mô hình đa luồng)
TAO_US = 1 * MS          # tạo một luồng mới (mô hình đa luồng)
NHIP_US = 10 * MS        # chu kỳ nhịp tim của vòng lặp sự kiện
SO_LOI = 2               # số lõi CPU của máy chủ
SO_THO = 2               # số luồng của nhóm luồng
HANG_TOI_DA = 2          # sức chứa hàng đợi của nhóm luồng
SERVER = "H5"
# (mã yêu cầu, client, thời điểm gửi µs)
YEU_CAU = [("A", "H4", 0), ("B", "H6", 0), ("C", "H4", 2 * MS),
           ("D", "H1", 0), ("E", "H2", 0), ("F", "H3", 0)]

TITLES = {
    "dong_bo": "Đồng bộ, một luồng – xử lý lần lượt, yêu cầu sau phải chờ",
    "da_luong": "Đa luồng – mỗi yêu cầu một luồng, chờ lõi CPU, khóa",
    "nhom_luong": "Nhóm luồng – 2 luồng thợ + hàng đợi 2, đầy thì từ chối (503)",
    "bat_dong_bo": "Bất đồng bộ – một vòng lặp sự kiện, việc CPU chặn vòng lặp",
    "phoi_hop": "Phối hợp – vòng lặp sự kiện lo I/O, nhóm luồng lo việc CPU",
}
TEN_NGAN = {"dong_bo": "đồng bộ", "da_luong": "đa luồng", "nhom_luong": "nhóm luồng",
            "bat_dong_bo": "bất đồng bộ", "phoi_hop": "phối hợp"}
NHAN_XET = {
    "dong_bo": ["Nhận xét: một luồng làm lần lượt; khi chờ CSDL luồng bị chặn,",
                "  CPU rảnh mà không phục vụ được ai; yêu cầu sau chờ cộng dồn."],
    "da_luong": ["Nhận xét: mỗi yêu cầu một luồng nên các lần chờ I/O chồng lên",
                 "  nhau; nhưng 6 luồng chỉ có 2 lõi (-: chờ lõi), đoạn găng chỉ",
                 "  cho một luồng vào mỗi lúc (~: chờ khóa); mỗi luồng tốn bộ",
                 "  nhớ và thời gian tạo (+)."],
    "nhom_luong": ["Nhận xét: số luồng cố định nên tài nguyên được giới hạn; khi",
                   "  2 thợ đều bận và hàng đợi đầy, yêu cầu bị từ chối ngay (503)",
                   "  thay vì làm máy chủ quá tải."],
    "bat_dong_bo": ["Nhận xét: một luồng vẫn phục vụ nhiều yêu cầu vì I/O không",
                    "  chặn (hàng I/O đang chờ có tới 6 lệnh cùng lúc); nhưng việc",
                    "  CPU chạy trên vòng lặp làm mọi việc khác, kể cả nhịp tim,",
                    "  phải chờ."],
    "phoi_hop": ["Nhận xét: vòng lặp chỉ điều phối (nhận, gửi lệnh I/O, trả lời);",
                 "  việc CPU chạy trên nhóm luồng, song song trên 2 lõi, rồi báo",
                 "  về vòng lặp: vòng lặp luôn rảnh, nhịp tim đúng giờ."],
}
KY_HIEU = {"io": None, "cpu": None, "khoa": "#", "cho_khoa": "~", "cho_cpu": "-", "tao": "+"}


class Lane:
    """Một luồng của máy chủ và các đoạn hoạt động của nó: [t0, t1, trạng thái, mã yêu cầu]."""

    def __init__(self, name):
        self.name = name
        self.segs = []
        self.marks = []          # thời điểm vòng lặp chạy một việc rất ngắn (nhận, chuyển giao, nhịp tim)

    def seg(self, t0, t1, kind, label):
        if t1 > t0:
            self.segs.append([t0, t1, kind, label])


class MayChu:
    def __init__(self, net, mode):
        self.net, self.mode = net, mode
        self.lanes = []
        self.free_core, self.cpu_wait = SO_LOI, deque()      # lõi CPU rảnh, hàng chờ lõi (FIFO)
        self.lock_busy, self.lock_wait, self.dem = False, deque(), 0
        self.io, self.beats = [], []          # I/O bất đồng bộ đang chờ; nhịp tim [dự kiến, thực tế]
        self.req = {}
        self.cho_xu_ly = len(YEU_CAU)         # số yêu cầu máy chủ chưa trả lời
        self.so_luong = self.max_luong = 0
        self.tu_choi = 0

    def log(self, text):
        self.net.log(text, SERVER)

    def lane(self, name):
        L = Lane(name)
        self.lanes.append(L)
        return L

    # ---------- tài nguyên dùng chung: lõi CPU và khóa ----------
    def cpu(self, lane, label, done):
        """Chạy 10 ms CPU trên `lane`; hết lõi thì chờ theo thứ tự đến."""
        if self.free_core > 0:
            self.free_core -= 1
            self._cpu_start(lane, label, done)
        else:
            self.log(f"{lane.name}: {label} chờ lõi CPU (cả {SO_LOI} lõi đang bận)")
            self.cpu_wait.append((lane, label, done, self.net.now))

    def _cpu_start(self, lane, label, done):
        t = self.net.now
        lane.seg(t, t + CPU_US, "cpu", label)
        self.net.at(t + CPU_US, lambda: self._cpu_end(done))

    def _cpu_end(self, done):
        if self.cpu_wait:                     # trao lõi vừa rảnh cho luồng chờ lâu nhất
            lane, label, d2, tw = self.cpu_wait.popleft()
            lane.seg(tw, self.net.now, "cho_cpu", label)
            self._cpu_start(lane, label, d2)
        else:
            self.free_core += 1
        done()

    def khoa(self, lane, label, done):
        """Vào đoạn găng: giữ khóa 3 ms để cập nhật bộ đếm chung; khóa đang bị giữ thì chờ."""
        if not self.lock_busy:
            self.lock_busy = True
            self._khoa_giu(lane, label, done)
        else:
            self.log(f"{lane.name}: {label} chờ khóa (luồng khác đang trong đoạn găng)")
            self.lock_wait.append((lane, label, done, self.net.now))

    def _khoa_giu(self, lane, label, done):
        t = self.net.now
        lane.seg(t, t + KHOA_US, "khoa", label)

        def tha():
            self.dem += 1
            self.log(f"{lane.name}: {label} vào đoạn găng, bộ đếm chung = {self.dem}, nhả khóa")
            if self.lock_wait:
                l2, lb2, d2, tw = self.lock_wait.popleft()
                l2.seg(tw, self.net.now, "cho_khoa", lb2)
                self._khoa_giu(l2, lb2, d2)
            else:
                self.lock_busy = False
            done()
        self.net.at(t + KHOA_US, tha)

    # ---------- mạng: gửi yêu cầu, nhận yêu cầu, trả lời ----------
    def bat_dau(self):
        self.net.on_deliver = self.nhan
        for label, client, t in YEU_CAU:
            self.req[label] = {"client": client, "gui": t, "den": 0, "nhan": 0, "ma": 0}

            def go(label=label, client=client):
                self.net.log(f"{client} gửi yêu cầu {label} tới máy chủ {SERVER}", client)
                self.net.send(client, Packet("uni", "YC", client, SERVER, f"YC {label} {client}→{SERVER}",
                                             200, {"id": label}))
            self.net.at(t, go)
        getattr(self, "khoi_tao_" + self.mode)()

    def nhan(self, node, pkt):
        label = pkt.data["id"]
        r = self.req[label]
        if node == SERVER:
            r["den"] = self.net.now
            self.log(f"yêu cầu {label} từ {r['client']} tới {SERVER}")
            getattr(self, "den_" + self.mode)(label)
        else:
            r["nhan"], r["ma"] = self.net.now, pkt.data["code"]
            self.net.log(f"{node} nhận trả lời {label} ({pkt.data['code']}) sau "
                         f"{fmt_t(self.net.now - r['gui'])} ms", node)

    def tra_loi(self, label, code=200):
        c = self.req[label]["client"]
        self.cho_xu_ly -= 1
        text = f"TL {label} {SERVER}→{c}" if code == 200 else f"503 {label} {SERVER}→{c}"
        self.net.send(SERVER, Packet("uni", "TL", SERVER, c, text, 1000 if code == 200 else 100,
                                     {"id": label, "code": code}))

    # ---------- 1. đồng bộ, một luồng ----------
    def khoi_tao_dong_bo(self):
        self.chinh = self.lane("luồng chính")
        self.so_luong = self.max_luong = 1
        self.cho_ket_noi, self.ban = deque(), False

    def den_dong_bo(self, label):
        if self.ban:
            self.cho_ket_noi.append(label)
            self.log(f"luồng chính bận → {label} chờ trong hàng đợi kết nối ({len(self.cho_ket_noi)})")
        else:
            self.dong_bo_xu_ly(label)

    def dong_bo_xu_ly(self, label):
        self.ban = True
        t = self.net.now
        self.log(f"luồng chính xử lý {label}: đọc CSDL, luồng bị chặn {IO_US // MS} ms")
        self.chinh.seg(t, t + IO_US, "io", label)
        self.net.at(t + IO_US, lambda: self.cpu(self.chinh, label, lambda: self.dong_bo_xong(label)))

    def dong_bo_xong(self, label):
        self.log(f"luồng chính xong {label} (CPU {CPU_US // MS} ms), gửi trả lời")
        self.tra_loi(label)
        if self.cho_ket_noi:
            self.dong_bo_xu_ly(self.cho_ket_noi.popleft())
        else:
            self.ban = False

    # ---------- 2. đa luồng: mỗi yêu cầu một luồng ----------
    def khoi_tao_da_luong(self):
        pass

    def den_da_luong(self, label):
        L = self.lane(f"T{len(self.lanes) + 1}")
        self.so_luong += 1
        self.max_luong = max(self.max_luong, self.so_luong)
        self.log(f"tạo luồng {L.name} cho {label} ({self.so_luong} luồng đang chạy)")
        t = self.net.now
        L.seg(t, t + TAO_US, "tao", label)

        def doc():
            u = self.net.now
            self.log(f"{L.name}: {label} đọc CSDL ({L.name} bị chặn, các luồng khác vẫn chạy)")
            L.seg(u, u + IO_US, "io", label)
            self.net.at(u + IO_US, lambda: self.cpu(L, label, lambda: self.khoa(L, label, xong)))

        def xong():
            self.so_luong -= 1
            self.log(f"{L.name}: gửi trả lời {label}, luồng kết thúc ({self.so_luong} luồng còn chạy)")
            self.tra_loi(label)
        self.net.at(t + TAO_US, doc)

    # ---------- 3. nhóm luồng + hàng đợi giới hạn ----------
    def khoi_tao_nhom_luong(self):
        self.tho = [self.lane(f"W{i + 1}") for i in range(SO_THO)]
        self.ranh = [True] * SO_THO
        self.hang = deque()
        self.so_luong = self.max_luong = SO_THO

    def den_nhom_luong(self, label):
        for i in range(SO_THO):
            if self.ranh[i]:
                self.nhom_giao(i, label)
                return
        if len(self.hang) < HANG_TOI_DA:
            self.hang.append((label, self.net.now))
            self.log(f"W1, W2 đều bận → {label} vào hàng đợi ({len(self.hang)}/{HANG_TOI_DA})")
        else:
            self.tu_choi += 1
            self.log(f"hàng đợi đầy → từ chối {label} ngay (503)")
            self.tra_loi(label, 503)

    def nhom_giao(self, i, label):
        self.ranh[i] = False
        L = self.tho[i]
        t = self.net.now
        self.log(f"{L.name} nhận {label}: đọc CSDL ({L.name} bị chặn {IO_US // MS} ms)")
        L.seg(t, t + IO_US, "io", label)
        self.net.at(t + IO_US, lambda: self.cpu(L, label, lambda: self.nhom_xong(i, label)))

    def nhom_xong(self, i, label):
        self.log(f"W{i + 1} xong {label}, gửi trả lời")
        self.tra_loi(label)
        if self.hang:
            lb, tw = self.hang.popleft()
            self.log(f"W{i + 1} lấy {lb} từ hàng đợi (đã chờ {fmt_t(self.net.now - tw)} ms)")
            self.nhom_giao(i, lb)
        else:
            self.ranh[i] = True

    # ---------- vòng lặp sự kiện (dùng cho 4 và 5) ----------
    def khoi_tao_vong_lap(self):
        self.vl = self.lane("vòng lặp")
        self.ready, self.vl_ban = deque(), False
        self.net.at(0, lambda: self.nhip(0))

    def post(self, fn, dur=0, label=""):
        """Đưa một việc vào hàng đợi của vòng lặp (dur > 0: việc CPU chạy ngay trên vòng lặp)."""
        self.ready.append((fn, dur, label))
        if not self.vl_ban:
            self.vl_ban = True
            self.net.at(self.net.now, self.chay)

    def chay(self):
        """Vòng lặp lấy lần lượt từng việc; việc CPU giữ vòng lặp tới khi xong."""
        while self.ready:
            fn, dur, label = self.ready.popleft()
            if dur:
                self.cpu(self.vl, label, lambda fn=fn: (fn(), self.chay()))
                return
            self.vl.marks.append(self.net.now)
            fn()
        self.vl_ban = False

    def nhip(self, p):
        """Nhịp tim mỗi 10 ms: một việc rất ngắn trên vòng lặp; đo nó chạy muộn bao nhiêu."""
        if self.cho_xu_ly == 0:
            return

        def beat():
            self.beats.append([p, self.net.now])
            if self.net.now - p >= MS:
                self.log(f"nhịp tim {fmt_t(p)} ms chạy muộn {fmt_t(self.net.now - p)} ms: vòng lặp bận")
        self.post(beat)
        self.net.at(p + NHIP_US, lambda: self.nhip(p + NHIP_US))

    def vl_nhan(self, label, xong_io):
        def go():
            t = self.net.now
            self.log(f"vòng lặp nhận {label}, gửi lệnh đọc CSDL không chặn")
            self.io.append([t, t + IO_US, label])
            self.net.at(t + IO_US, lambda: xong_io(label))
        self.post(go)

    # ---------- 4. bất đồng bộ: một vòng lặp, CPU trên vòng lặp ----------
    def khoi_tao_bat_dong_bo(self):
        self.so_luong = self.max_luong = 1
        self.khoi_tao_vong_lap()

    def den_bat_dong_bo(self, label):
        self.vl_nhan(label, self.bat_io_xong)

    def bat_io_xong(self, label):
        self.log(f"CSDL xong {label} → việc CPU {CPU_US // MS} ms vào hàng đợi vòng lặp")

        def xong():
            self.log(f"vòng lặp tính xong {label} (CPU trên vòng lặp), trả lời")
            self.tra_loi(label)
        self.post(xong, CPU_US, label)

    # ---------- 5. phối hợp: vòng lặp + nhóm luồng cho việc CPU ----------
    def khoi_tao_phoi_hop(self):
        self.so_luong = self.max_luong = 1 + SO_THO
        self.khoi_tao_vong_lap()
        self.tho = [self.lane(f"P{i + 1}") for i in range(SO_THO)]
        self.ranh = [True] * SO_THO
        self.hang = deque()

    def den_phoi_hop(self, label):
        self.vl_nhan(label, self.phoi_io_xong)

    def phoi_io_xong(self, label):
        def giao():
            self.log(f"vòng lặp giao việc CPU của {label} cho nhóm luồng")
            for i in range(SO_THO):
                if self.ranh[i]:
                    self.phoi_chay(i, label)
                    return
            self.hang.append(label)
            self.log(f"P1, P2 đều bận → việc CPU của {label} chờ trong hàng đợi")
        self.post(giao)

    def phoi_chay(self, i, label):
        self.ranh[i] = False
        self.cpu(self.tho[i], label, lambda: self.phoi_xong(i, label))

    def phoi_xong(self, i, label):
        self.log(f"P{i + 1} tính xong {label} → gửi kết quả về vòng lặp")

        def tra():
            self.log(f"vòng lặp nhận kết quả {label} từ nhóm luồng, trả lời")
            self.tra_loi(label)
        self.post(tra)
        if self.hang:
            self.phoi_chay(i, self.hang.popleft())
        else:
            self.ranh[i] = True

    # ---------- báo cáo ----------
    def gantt(self):
        end = 0
        for L in self.lanes:
            for s in L.segs:
                end = max(end, s[1])
        for s in self.io:
            end = max(end, s[1])
        step = max(MS, (end + 46 * MS - 1) // (46 * MS) * MS)    # tối đa 46 ô
        width = (end + step - 1) // step
        print(f"Bên trong máy chủ {SERVER}, 1 ô = {step // MS} ms:")
        print("  CHỮ HOA dùng CPU, chữ thường bị chặn chờ I/O, + tạo luồng,")
        print("  - chờ lõi CPU, # giữ khóa, ~ chờ khóa, · việc rất ngắn")
        for L in self.lanes:
            row = ""
            for i in range(width):
                mid = i * step + step // 2
                ch = " "
                for t0, t1, kind, label in L.segs:
                    if t0 <= mid < t1:
                        ch = label if kind == "cpu" else label.lower() if kind == "io" else KY_HIEU[kind]
                        break
                if ch == " " and any(i * step <= m < (i + 1) * step for m in L.marks):
                    ch = "·"
                row += ch
            print(f"  {L.name:<12}|{row}|")
        if self.io:
            row = ""
            for i in range(width):
                mid = i * step + step // 2
                n = sum(1 for t0, t1, _ in self.io if t0 <= mid < t1)
                row += str(min(n, 9)) if n else " "
            print(f"  {'I/O đang chờ':<12}|{row}|")
        right = f"{width * step // MS} ms"
        print(f"  {'':<12} 0{right.rjust(width - 1)}")

    def chi_so(self):
        """Chỉ số so sánh: hoàn tất, phản hồi trung bình/lớn nhất, số luồng, từ chối, nhịp tim trễ."""
        tong, lon, hoan_tat = 0, 0, 0
        for label, _, _ in YEU_CAU:
            r = self.req[label]
            ph = r["nhan"] - r["gui"]
            tong += ph
            lon = max(lon, ph)
            hoan_tat = max(hoan_tat, r["nhan"])
        nhip = max(b[1] - b[0] for b in self.beats) if self.beats else -1
        return {"hoan_tat": hoan_tat, "tb": tong // len(YEU_CAU), "lon": lon, "luong": self.max_luong,
                "tu_choi": self.tu_choi, "nhip": nhip}

    def report(self):
        print()
        self.gantt()
        print()
        print(f"  {'Mã':<3} {'Client':<6} {'Gửi':>6} {'Tới H5':>7} {'Nhận TL':>8} {'Phản hồi':>9}  Kết quả")
        for label, client, _ in YEU_CAU:
            r = self.req[label]
            kq = "200 OK" if r["ma"] == 200 else "503 từ chối"
            print(f"  {label:<3} {client:<6} {fmt_t(r['gui']):>6} {fmt_t(r['den']):>7} "
                  f"{fmt_t(r['nhan']):>8} {fmt_t(r['nhan'] - r['gui']):>9}  {kq}")
        s = self.chi_so()
        print(f"  Hoàn tất lúc {fmt_t(s['hoan_tat'])} ms; phản hồi TB {fmt_t(s['tb'])} ms, "
              f"lớn nhất {fmt_t(s['lon'])} ms")
        print(f"  Luồng tối đa {s['luong']}; từ chối {s['tu_choi']}")
        if self.beats:
            tre = [b[1] - b[0] for b in self.beats]
            print(f"  Nhịp tim {NHIP_US // MS} ms: {len(tre)} nhịp, trễ lớn nhất {fmt_t(s['nhip'])} ms, "
                  f"TB {fmt_t(sum(tre) // len(tre))} ms")
        print()
        for line in NHAN_XET[self.mode]:
            print(line)

    def data(self):
        """Dữ liệu cho hoạt hình HTML: các luồng, I/O đang chờ, nhịp tim."""
        d = {"lanes": [[L.name, L.segs, L.marks] for L in self.lanes]}
        if self.io:
            d["io"] = self.io
        if self.beats:
            d["beats"] = self.beats
        return d


def chay(net, mode):
    """Chạy một mô hình máy chủ trên `net`, in nhật ký và báo cáo; trả về dữ liệu hoạt hình."""
    net.hop_trace = False
    print(f"6 yêu cầu tới {SERVER}, mỗi yêu cầu: đọc CSDL {IO_US // MS} ms (chờ I/O)")
    print(f"  + xử lý {CPU_US // MS} ms CPU; máy chủ có {SO_LOI} lõi CPU")
    m = MayChu(net, mode)
    m.bat_dau()
    net.run()
    m.report()
    return m.data()


def chi_so(net, mode):
    """Chạy một mô hình máy chủ không in gì (net.quiet); trả về chỉ số so sánh."""
    net.hop_trace, net.quiet = False, True
    m = MayChu(net, mode)
    m.bat_dau()
    net.run()
    return m.chi_so()


def so_sanh(cac_ket_qua):
    """In bảng so sánh năm mô hình từ chỉ số đã đo: danh sách (mô hình, chỉ số)."""
    print("So sánh năm mô hình máy chủ: cùng 6 yêu cầu, mỗi yêu cầu")
    print(f"  {IO_US // MS} ms I/O + {CPU_US // MS} ms CPU, máy chủ {SO_LOI} lõi. TB, Lớn nhất: thời gian")
    print("  phản hồi; 503: số yêu cầu bị từ chối; Nhịp tim: trễ lớn nhất.")
    print(f"  {'Mô hình':<11} {'Luồng':>5} {'Hoàn tất':>9} {'TB':>9} {'Lớn nhất':>9} {'503':>3} {'Nhịp tim':>9}")
    for mode, s in cac_ket_qua:
        nhip = fmt_t(s["nhip"]) + " ms" if s["nhip"] >= 0 else "–"
        print(f"  {TEN_NGAN[mode]:<11} {s['luong']:>5} {fmt_t(s['hoan_tat']) + ' ms':>9} "
              f"{fmt_t(s['tb']) + ' ms':>9} {fmt_t(s['lon']) + ' ms':>9} {s['tu_choi']:>3} {nhip:>9}")
    print()
    print("Ghi nhớ: luồng giúp chồng thời gian chờ I/O nhưng tốn tài nguyên")
    print("  và cần khóa khi dùng chung dữ liệu; bất đồng bộ phục vụ nhiều")
    print("  I/O bằng một luồng nhưng không được làm việc CPU trên vòng")
    print("  lặp; phối hợp vòng lặp + nhóm luồng có được cả hai ưu điểm.")
