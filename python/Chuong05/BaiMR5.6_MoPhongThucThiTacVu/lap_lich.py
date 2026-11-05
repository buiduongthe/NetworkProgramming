"""Bài mở rộng MR5.6 – Bộ lập lịch mô phỏng: đơn luồng/đa luồng × đồng bộ/bất đồng bộ.

Thời gian rời rạc, 1 nhịp = 100 ms. Mỗi tác vụ T1, T2, T3 có thời lượng D giây, gồm ba bước:
  CPU (D/4) → chờ I/O (D/2, ví dụ đọc mạng hay đĩa) → CPU (phần còn lại).
Tùy chọn:
  - phụ thuộc: T3 chỉ bắt đầu khi T1 và T2 đã xong (join / await gather / Task.WhenAll);
  - chia: bước CPU cuối của T1 được chia đôi, hai nửa chạy cùng lúc trên hai luồng rồi hợp lại;
  - việc khẩn U: đến khi các luồng đang bận (mặc định lúc 0,3 s), cần 0,5 s CPU, ưu tiên cao;
  - số lõi CPU và lát thời gian (quantum) của bộ lập lịch hệ điều hành.

Bốn chế độ:
  don_dong_bo      một luồng, lời gọi chặn: làm lần lượt theo thứ tự đến (T1, T2, T3 rồi U); khi chờ I/O
                   luồng đứng yên, không làm được việc khác;
  da_dong_bo       mỗi tác vụ một luồng hệ điều hành, lời gọi chặn: các luồng tranh nhau lõi CPU; hệ điều hành
                   chia lát thời gian (chạy hết lát mà có luồng cùng hoặc cao hơn ưu tiên đang chờ thì phải nhường)
                   và cho luồng ưu tiên cao CHEN NGANG ngay (preemption); T3 chờ T1, T2 bằng join; nửa sau
                   phần CPU của T1 chạy trên luồng phụ T1b;
  don_bat_dong_bo  một luồng chạy vòng lặp sự kiện: bước CPU chạy trên vòng lặp tới khi xong (đa nhiệm hợp tác,
                   không ai chen ngang), chờ I/O không giữ luồng nên các I/O chồng lên nhau; việc sẵn sàng xếp
                   hàng theo thứ tự đến (FIFO, như asyncio);
  da_bat_dong_bo   nhóm luồng thợ (số thợ = số lõi) + bất đồng bộ: mỗi bước CPU giao cho thợ rảnh đầu tiên, nên
                   sau một lần chờ I/O, cùng một tác vụ có thể chạy tiếp trên MỘT LUỒNG KHÁC; hai nửa của T1 chạy
                   song song trên hai thợ (như Task.Run + await, CompletableFuture, tokio đa luồng).
Mỗi nhịp gồm bốn bước theo thứ tự cố định: (1) tác vụ đến, (2) các luồng/tác vụ tiến sang thao tác kế tiếp,
(3) lập lịch (cấp lõi, giao việc cho thợ), (4) chạy một nhịp và ghi lại. Kết quả tất định: cùng tham số →
cùng biểu đồ, cùng số liệu ở mọi ngôn ngữ.
"""

CHE_DO = {
    "don_dong_bo": "Đơn luồng – đồng bộ (Single-threaded Sync)",
    "da_dong_bo": "Đa luồng – đồng bộ (Multi-threaded Sync)",
    "don_bat_dong_bo": "Đơn luồng – bất đồng bộ (Single-threaded Async)",
    "da_bat_dong_bo": "Đa luồng – bất đồng bộ (Multi-threaded Async)",
}
TEN_NGAN = {"don_dong_bo": "đơn, đồng bộ", "da_dong_bo": "đa, đồng bộ",
            "don_bat_dong_bo": "đơn, bất đ.bộ", "da_bat_dong_bo": "đa, bất đ.bộ"}
# trạng thái một ô (một làn, một nhịp)
CPU, IO, SAN_SANG, CHO, RANH = "cpu", "io", "san_sang", "cho", "ranh"
U_CPU = 5                     # việc khẩn: 0,5 s CPU


def giay(nhip):
    """Số nhịp (100 ms) → chuỗi giây, một chữ số thập phân."""
    return f"{nhip // 10}.{nhip % 10}"


class ThamSo:
    def __init__(self, t1=4.0, t2=3.0, t3=2.0, so_loi=2, lat=0.5, phu_thuoc=True, chia=True,
                 khan=True, t_khan=0.3):
        self.d = [int(round(t1 * 10)), int(round(t2 * 10)), int(round(t3 * 10))]   # nhịp
        self.so_loi = max(1, so_loi)
        self.lat = max(1, int(round(lat * 10)))
        self.phu_thuoc, self.chia, self.khan = phu_thuoc, chia, khan
        self.t_khan = int(round(t_khan * 10))


class Viec:
    """Một tác vụ: các bước [loại, số nhịp], loại = cpu | io | chia (CPU chia đôi chạy song song)."""

    def __init__(self, ten, buoc, uu_tien, den, phu_thuoc):
        self.ten, self.buoc = ten, buoc
        self.uu_tien, self.den, self.phu_thuoc = uu_tien, den, phu_thuoc
        self.bat_dau = self.xong = -1          # nhịp đầu tiên dùng CPU, nhịp hoàn tất
        self.k = 0                             # (bất đồng bộ) bước đang làm
        self.con_cho = 0                       # (bất đồng bộ) số nửa của bước chia còn chưa xong
        self.da_chay = False                   # (bất đồng bộ) đã bắt đầu


def tao_viec(ts):
    ds = []
    for i in range(3):
        n = ts.d[i]
        c1, io = n // 4, n // 2
        c2 = n - c1 - io
        buoc = [["cpu", c1], ["io", io], ["chia" if ts.chia and i == 0 and c2 >= 2 else "cpu", c2]]   # < 2 nhịp: không chia được
        dep = ["T1", "T2"] if ts.phu_thuoc and i == 2 else []
        ds.append(Viec(f"T{i + 1}", [b for b in buoc if b[1] > 0], 0, 0, dep))
    if ts.khan:
        ds.append(Viec("U", [["cpu", U_CPU]], 1, ts.t_khan, []))
    return ds


class KetQua:
    """Kết quả một lần chạy: làn [tên, [[trạng thái, nhãn], …]], số I/O đang chờ mỗi nhịp, nhật ký, tác vụ."""

    def __init__(self, che_do, ts):
        self.che_do, self.ts = che_do, ts
        self.lan, self.io, self.nhat_ky, self.viec = [], [], [], []
        self.chen_ngang = self.het_lat = 0
        self.tong = 0

    def log(self, t, text):
        self.nhat_ky.append([t, text])


# ======================================================================== đồng bộ (luồng hệ điều hành)
class Luong:
    def __init__(self, ten, uu_tien, ct, t):
        self.ten, self.uu_tien = ten, uu_tien
        self.ct = ct                     # chương trình: các thao tác chưa làm
        self.op = None                   # thao tác đang làm: [loại, số nhịp còn lại | điều kiện, nhãn, việc]
        self.ket_thuc = False
        self.trang_thai = RANH
        self.lat_da_chay = 0
        self.o = [[RANH, ""] for _ in range(t)]


class DongBo:
    def __init__(self, ts, nhieu_luong):
        self.ts, self.nhieu = ts, nhieu_luong
        self.kq = KetQua("da_dong_bo" if nhieu_luong else "don_dong_bo", ts)
        self.viec = tao_viec(ts)
        self.kq.viec = self.viec
        self.luong, self.san_sang, self.dang_chay = [], [], []    # san_sang: [luồng, số thứ tự]
        self.stt = 0
        self.hang = []                   # đơn luồng: tác vụ đã đến, chờ luồng chính (FIFO)
        self.chinh = None
        self.t = 0

    def log(self, text):
        self.kq.log(self.t, text)

    def ct_viec(self, v, mot_luong):
        """Chương trình của tác vụ v: [chờ phụ thuộc], các bước, xong."""
        ct = []
        if v.phu_thuoc and not mot_luong:
            ct.append(["cho_viec", list(v.phu_thuoc), v.ten, v])
        for loai, n in v.buoc:
            if loai == "chia":
                a, b = (n + 1) // 2, n // 2
                if mot_luong:
                    ct.append(["ghi", f"{v.ten}: chia CPU cuối 2 nửa, một luồng nên làm lần lượt", v.ten, v])
                    ct += [["cpu", a, v.ten, v], ["cpu", b, v.ten, v]]
                else:
                    ct += [["tao_phu", b, v.ten, v], ["cpu", a, v.ten, v], ["cho_phu", None, v.ten, v]]
            else:
                ct.append([loai, n, v.ten, v])
        ct.append(["xong", 0, v.ten, v])
        return ct

    def them_luong(self, ten, uu_tien, ct):
        L = Luong(ten, uu_tien, ct, self.t)
        self.luong.append(L)
        return L

    def vao_san_sang(self, L):
        self.stt += 1
        self.san_sang.append([L, self.stt])

    def roi_loi(self, L):
        if L in self.dang_chay:
            self.dang_chay.remove(L)

    def tien(self, L):
        """Bước 2: luồng L làm các thao tác tức thời và sang thao tác kế tiếp nếu thao tác hiện tại đã xong."""
        while True:
            op = L.op
            if op is not None:
                loai = op[0]
                if loai in ("cpu", "io") and op[1] > 0:
                    return
                if loai == "cho_viec":
                    if not all(any(v.ten == d and v.xong >= 0 for v in self.viec) for d in op[1]):
                        return
                    self.log(f"{L.ten}: {' và '.join(op[1])} đã xong (join), {op[2]} chạy tiếp")
                if loai == "cho_phu":
                    if not op[1].ket_thuc:
                        return
                    self.log(f"{L.ten}: hai nửa của {op[2]} đã xong (join {op[1].ten})")
            if not L.ct:
                if not self.nhieu and L is self.chinh and self.hang:
                    v = self.hang.pop(0)
                    self.log(f"Luồng chính bắt đầu {v.ten}")
                    L.ct = self.ct_viec(v, True)
                else:
                    L.op = None
                    self.roi_loi(L)
                    if self.nhieu or all(v.den <= self.t for v in self.viec):
                        L.ket_thuc = True
                    L.trang_thai = RANH
                    return
            L.op = L.ct.pop(0)
            loai = L.op[0]
            if loai == "cpu":
                if L not in self.dang_chay and all(s[0] is not L for s in self.san_sang):
                    self.vao_san_sang(L)
                L.trang_thai = CPU if L in self.dang_chay else SAN_SANG
                return
            self.roi_loi(L)
            if loai == "io":
                L.trang_thai = IO
                return
            if loai == "cho_viec":
                L.trang_thai = CHO
                if not all(any(v.ten == d and v.xong >= 0 for v in self.viec) for d in L.op[1]):
                    self.log(f"{L.ten}: {L.op[2]} chờ {' và '.join(L.op[1])} xong (join)")
                continue
            if loai == "tao_phu":
                phu = self.them_luong(f"Luồng {L.op[2]}b", L.uu_tien,
                                      [["cpu", L.op[1], L.op[2], L.op[3]], ["het", 0, L.op[2], L.op[3]]])
                self.log(f"{L.op[2]}: chia CPU cuối 2 nửa, {phu.ten} chạy nửa sau")
                L.ct[1][1] = phu               # thao tác cho_phu chờ đúng luồng phụ này
                continue
            if loai == "cho_phu":
                L.trang_thai = CHO
                continue
            if loai == "ghi":
                self.log(L.op[1])
                continue
            if loai == "xong":
                v = L.op[3]
                v.xong = self.t
                self.log(f"{v.ten} xong lúc {giay(self.t)} s")
                continue
            if loai == "het":
                continue

    def lap_lich(self):
        """Bước 3: bộ lập lịch hệ điều hành cấp lõi CPU cho các luồng sẵn sàng."""
        ts = self.ts

        def tot_nhat():
            return min(self.san_sang, key=lambda s: (-s[0].uu_tien, s[1])) if self.san_sang else None
        # a. hết lát thời gian: nhường lõi nếu có luồng ưu tiên bằng hoặc cao hơn đang chờ
        for r in list(self.dang_chay):
            if r.lat_da_chay >= ts.lat and any(s[0].uu_tien >= r.uu_tien for s in self.san_sang):
                self.dang_chay.remove(r)
                r.lat_da_chay = 0
                self.vao_san_sang(r)
                self.kq.het_lat += 1
                self.log(f"{r.ten} hết lát thời gian {giay(ts.lat)} s, nhường lõi")
        # b. lõi còn trống: cấp cho luồng sẵn sàng tốt nhất (ưu tiên cao, rồi đến trước)
        while len(self.dang_chay) < ts.so_loi and self.san_sang:
            s = tot_nhat()
            self.san_sang.remove(s)
            s[0].lat_da_chay = 0
            self.dang_chay.append(s[0])
        # c. chen ngang: luồng sẵn sàng ưu tiên cao hơn luồng đang chạy thì lấy lõi ngay
        while self.san_sang:
            s = tot_nhat()
            thap = [r for r in self.dang_chay if r.uu_tien < s[0].uu_tien]
            if not thap:
                break
            nan = min(thap, key=lambda r: (r.uu_tien, -r.lat_da_chay, self.dang_chay.index(r)))
            i = self.dang_chay.index(nan)
            self.san_sang.remove(s)
            s[0].lat_da_chay = 0
            self.dang_chay[i] = s[0]
            nan.lat_da_chay = 0
            self.vao_san_sang(nan)
            self.kq.chen_ngang += 1
            self.log(f"{s[0].ten} CHEN NGANG: {nan.ten} tạm dừng, nhường lõi")
        for L in self.luong:
            if L.op is not None and L.op[0] == "cpu" and L.op[1] > 0:
                L.trang_thai = CPU if L in self.dang_chay else SAN_SANG

    def chay(self):
        while True:
            # 1. tác vụ đến
            for v in self.viec:
                if v.den == self.t:
                    if v.uu_tien:
                        self.log(f"{v.ten} (việc khẩn, ưu tiên cao) đến")
                    if self.nhieu:
                        L = self.them_luong(f"Luồng {v.ten}", v.uu_tien, self.ct_viec(v, False))
                        if v.uu_tien:
                            self.log(f"tạo {L.ten}")
                    else:
                        self.hang.append(v)
                        if self.chinh is None:
                            self.chinh = self.them_luong("Luồng chính", 0, [])
                        elif v.uu_tien and (self.chinh.op is not None or self.chinh.ct):
                            self.log(f"{v.ten} xếp hàng: luồng chính bận, không chen ngang được")
            # 2. tiến (theo thứ tự tạo luồng; luồng mới tạo trong bước này cũng được tiến). Làm hai lượt để
            #    luồng đang join thấy ngay luồng có số thứ tự lớn hơn vừa kết thúc trong cùng nhịp.
            for _ in range(2):
                i = 0
                while i < len(self.luong):
                    if not self.luong[i].ket_thuc:
                        self.tien(self.luong[i])
                    i += 1
            if all(L.ket_thuc for L in self.luong) and all(v.den <= self.t for v in self.viec):
                break
            # 3. lập lịch
            self.lap_lich()
            # 4. chạy một nhịp
            for L in self.luong:
                op = L.op
                if L.ket_thuc or op is None:
                    L.o.append([RANH, ""])
                elif L.trang_thai == CPU:
                    op[1] -= 1
                    L.lat_da_chay += 1
                    if op[3].bat_dau < 0:
                        op[3].bat_dau = self.t
                    L.o.append([CPU, op[2]])
                elif L.trang_thai == IO:
                    op[1] -= 1
                    L.o.append([IO, op[2]])
                else:
                    L.o.append([L.trang_thai, op[2]])
            self.t += 1
        self.kq.tong = self.t
        self.kq.lan = [[L.ten, L.o] for L in self.luong]
        return self.kq


# ======================================================================== bất đồng bộ (vòng lặp / nhóm thợ)
class BatDongBo:
    def __init__(self, ts, nhieu_luong):
        self.ts, self.nhieu = ts, nhieu_luong
        self.kq = KetQua("da_bat_dong_bo" if nhieu_luong else "don_bat_dong_bo", ts)
        self.viec = tao_viec(ts)
        self.kq.viec = self.viec
        so_tho = ts.so_loi if nhieu_luong else 1
        self.ten_tho = [f"Thợ {i + 1}" for i in range(so_tho)] if nhieu_luong else ["Vòng lặp"]
        self.tho = [None] * so_tho       # việc đang chạy trên mỗi thợ: [tác vụ, nhịp còn lại, là nửa?]
        self.o = [[] for _ in range(so_tho)]
        self.hang = []                   # các bước CPU sẵn sàng, FIFO: [tác vụ, số nhịp, là nửa?]
        self.cho_io = []                 # [tác vụ, nhịp còn lại]
        self.cuoi_tho = {}               # tác vụ -> thợ chạy bước CPU gần nhất (để ghi "chạy tiếp trên thợ khác")
        self.t = 0

    def log(self, text):
        self.kq.log(self.t, text)

    def bat_dau_buoc(self, v):
        """Tác vụ v bắt đầu bước v.k (hoặc hoàn tất nếu hết bước)."""
        if v.k >= len(v.buoc):
            v.xong = self.t
            self.log(f"{v.ten} xong lúc {giay(self.t)} s")
            for w in self.viec:          # tác vụ đang chờ (await) các tác vụ vừa xong
                if not w.da_chay and w.den <= self.t and w.phu_thuoc and self.du_phu_thuoc(w):
                    self.log(f"{' và '.join(w.phu_thuoc)} đã xong → {w.ten} bắt đầu (await)")
                    self.khoi_dong(w)
            return
        loai, n = v.buoc[v.k]
        if loai == "cpu":
            self.hang.append([v, n, False])
        elif loai == "io":
            self.cho_io.append([v, n])
        else:                            # chia: hai nửa vào hàng cùng lúc
            v.con_cho = 2
            self.hang.append([v, (n + 1) // 2, True])
            self.hang.append([v, n // 2, True])
            if len(self.tho) > 1:
                self.log(f"{v.ten}: chia CPU cuối 2 nửa, chạy song song trên 2 thợ")
            else:
                self.log(f"{v.ten}: chia CPU cuối 2 nửa, một luồng nên làm lần lượt")

    def du_phu_thuoc(self, v):
        return all(any(w.ten == d and w.xong >= 0 for w in self.viec) for d in v.phu_thuoc)

    def khoi_dong(self, v):
        v.da_chay = True
        v.k = 0
        self.bat_dau_buoc(v)

    def chay(self):
        so_tho = len(self.tho)
        while True:
            # 1. tác vụ đến
            for v in self.viec:
                if v.den == self.t:
                    if v.uu_tien:
                        self.log(f"{v.ten} (việc khẩn) đến – xếp hàng, không chen ngang được")
                    if v.phu_thuoc and not self.du_phu_thuoc(v):
                        self.log(f"{v.ten} chờ {' và '.join(v.phu_thuoc)} xong (await), không giữ luồng nào")
                    else:
                        self.khoi_dong(v)
            # 2. tiến: bước CPU xong (theo thứ tự thợ), rồi I/O xong (theo thứ tự bắt đầu chờ)
            for i in range(so_tho):
                x = self.tho[i]
                if x is not None and x[1] == 0:
                    self.tho[i] = None
                    v = x[0]
                    if x[2]:
                        v.con_cho -= 1
                        if v.con_cho == 0:
                            self.log(f"{v.ten}: hai nửa đã xong, hợp kết quả")
                            v.k += 1
                            self.bat_dau_buoc(v)
                    else:
                        v.k += 1
                        self.bat_dau_buoc(v)
            for x in list(self.cho_io):
                if x[1] == 0:
                    self.cho_io.remove(x)
                    x[0].k += 1
                    self.bat_dau_buoc(x[0])
            if all(v.xong >= 0 for v in self.viec):
                break
            # 3. giao việc: thợ rảnh (theo thứ tự) lấy bước CPU đứng đầu hàng
            for i in range(so_tho):
                if self.tho[i] is None and self.hang:
                    v, n, nua = self.hang.pop(0)
                    self.tho[i] = [v, n, nua]
                    truoc = self.cuoi_tho.get(v.ten)
                    if self.nhieu and truoc is not None and truoc != i and not nua:
                        self.log(f"{v.ten} chạy tiếp trên {self.ten_tho[i]} (trước đó ở {self.ten_tho[truoc]})")
                    self.cuoi_tho[v.ten] = i
            # 4. chạy một nhịp
            for i in range(so_tho):
                x = self.tho[i]
                if x is None:
                    self.o[i].append([RANH, ""])
                else:
                    x[1] -= 1
                    if x[0].bat_dau < 0:
                        x[0].bat_dau = self.t
                    self.o[i].append([CPU, x[0].ten])
            self.kq.io.append(len(self.cho_io))
            for x in self.cho_io:
                x[1] -= 1
            self.t += 1
        self.kq.tong = self.t
        self.kq.lan = [[self.ten_tho[i], self.o[i]] for i in range(so_tho)]
        return self.kq


def chay(che_do, ts):
    if che_do == "don_dong_bo":
        return DongBo(ts, False).chay()
    if che_do == "da_dong_bo":
        return DongBo(ts, True).chay()
    if che_do == "don_bat_dong_bo":
        return BatDongBo(ts, False).chay()
    return BatDongBo(ts, True).chay()


# ======================================================================== chỉ số và biểu đồ văn bản
def chi_so(kq):
    """Hoàn tất (nhịp), độ trễ phản hồi của U (nhịp, -1 nếu không có U), số luồng, % lõi bận."""
    ban = sum(1 for _, o in kq.lan for c in o if c[0] == CPU)
    u = next((v for v in kq.viec if v.ten == "U"), None)
    return {"tong": kq.tong, "u_cho": (u.bat_dau - u.den) if u else -1, "luong": len(kq.lan),
            "ban": ban * 100 // max(1, kq.ts.so_loi * kq.tong), "chen": kq.chen_ngang, "lat": kq.het_lat}


KY_HIEU = {IO: "~", SAN_SANG: "-", CHO: ".", RANH: " "}


def ky_tu(c):
    if c[0] == CPU:
        return "U" if c[1] == "U" else c[1][1]       # T1 → "1"
    return KY_HIEU[c[0]]


def gantt(kq, rong=48):
    """Biểu đồ Gantt văn bản, 1 ô = 1 nhịp (0,1 s); dài hơn `rong` nhịp thì xuống dòng thành nhiều khối."""
    dong = []
    for dau in range(0, kq.tong, rong):
        cuoi = min(kq.tong, dau + rong)
        for ten, o in kq.lan:
            dong.append(f"  {ten:<11}|{''.join(ky_tu(c) for c in o[dau:cuoi])}|")
        if kq.io:
            s = "".join(str(min(n, 9)) if n else " " for n in kq.io[dau:cuoi])
            dong.append(f"  {'I/O chờ':<11}|{s}|")
        nhan = f"{giay(cuoi)} s"
        dong.append(f"  {'':<11} {giay(dau)}{nhan.rjust(cuoi - dau - len(giay(dau)) + 2)}")
    return dong
