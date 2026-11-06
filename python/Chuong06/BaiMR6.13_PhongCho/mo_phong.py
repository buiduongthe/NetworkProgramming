"""Bài mở rộng MR6.13 – Bộ mô phỏng tải: n người dùng ảo trên k kết nối, rào xuất phát, pipelining.

Phần KHUNG (từ "KHUNG" trở xuống) giống nhau ở 9 bài tranh chấp; mỗi bài chỉ sửa phần NGHIỆP VỤ:
  - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: phòng chờ bật hay tắt), cập nhật từ
    cap_nhat và từ trả lời kq;
  - NguoiAo: kịch bản của MỘT người dùng ảo: yeu_cau_dau() -> yêu cầu đầu tiên; sau_kq(tin) -> yêu cầu
    kế tiếp hoặc None (xong). Yêu cầu là dict {"loai": ..., các trường...} (khung tự thêm ma_yc).
  - THAM_SO_DAT_LAI: tham số gửi kèm dat_lai trước mỗi chế độ trong so_sanh.

Người ảo ở bài này (mỗi người mua 1–4 vé, `lan` không dùng):
  - phòng chờ bật:  xep_hang -> cho_luot (máy chủ giữ yêu cầu tới khi tới lượt) -> mua;
  - phòng chờ tắt:  mua ngay; bị "qua_tai" thì bấm lại ngay (tối đa 3 lần) – như người dùng nhấn F5;
  - 1/10 người là "TAB THỨ HAI" của người liền trước (u10 đóng vai u9…): cùng xếp hàng, cùng được lượt,
    cùng bấm mua -> chế độ an toàn chỉ nhận một đơn, khong_khoa nhận cả hai (sai phạm "sai lượt").
DO_MUA ghi thời gian từ lần bấm "mua" đầu tiên tới kết quả cuối của từng người (gồm cả các lần bấm lại)
– so_sanh_phong_cho() dùng để so sánh bật/tắt phòng chờ.

Một lượt: kết nối k socket (người i đi trên kết nối i mod k; kết nối chào kèm "so_nguoi"), mọi luồng
đứng chờ ở RÀO XUẤT PHÁT (threading.Barrier) -> cùng lúc mỗi kết nối gửi yêu cầu đầu của mọi người
mình chở trong MỘT lần sendall; trả lời tới đâu gửi yêu cầu kế tiếp của người đó tới đó. Đo thời gian
tổng, yc/s, khứ hồi p50/p95/p99, rồi hỏi thong_ke của máy chủ (sai phạm theo sổ kiểm toán).
Khứ hồi gồm cả cho_luot – tức là cả thời gian đứng trong phòng chờ.

Chạy:  python3 mo_phong.py <host> <cổng> [--n 200] [--k 200] [--lan 1] [--che-do khoa] [--dat-lai]
                          [--so-sanh] [--tre-ms 2]
       (--dat-lai, --so-sanh: phòng chờ bật, cấp lượt 200 người/s, thùng 20 token)
       so sánh bật/tắt phòng chờ: python3 so_sanh_phong_cho.py <host> <cổng> [--n 1000]
"""
import argparse
import collections
import random
import threading
import time

from chung import CHE_DO, bang_so_sanh, dong_sai_pham, phan_vi, so_us
from khach import KetNoiKhach, QuanTri

# ===================================================================== NGHIỆP VỤ (MR6.13 – phòng chờ)
SO_LAN_MAC_DINH = 1                       # mỗi người một lần mua (1–4 vé)
THAM_SO_DAT_LAI = {"phong_cho": 1, "toc_do": 200.0, "dung_luong": 20}
BAM_LAI = 3                               # bị quá tải / xung đột: bấm lại tối đa 3 lần


class DoMua:
    """Kết quả cuối của từng người, thời gian mua (ms), số lần bị từ chối vì quá tải – an toàn đa luồng."""

    def __init__(self):
        self.khoa = threading.Lock()
        self.dat_lai()

    def dat_lai(self):
        with self.khoa:
            self.tre, self.kq, self.qua_tai = [], collections.Counter(), 0

    def ket_thuc(self, ma, tre_ms=None):
        with self.khoa:
            self.kq[ma] += 1
            if tre_ms is not None:
                self.tre.append(tre_ms)

    def dem_qua_tai(self):
        with self.khoa:
            self.qua_tai += 1


DO_MUA = DoMua()


class NguCanh:
    """Phòng chờ bật hay tắt (đọc từ cap_nhat đầu tiên)."""

    def __init__(self, trang_thai):
        self.phong_cho = trang_thai["phong_cho"]

    def cap_nhat(self, tin):
        pass

    def nhan_kq(self, tin):
        pass


class NguoiAo:
    def __init__(self, ten, nc, lan):
        i = int(ten[1:])
        self.nc = nc
        self.la_tab2 = i % 10 == 0                       # tab thứ hai của người u(i-1)
        self.ai = f"u{i - 1}" if self.la_tab2 else ten
        self.so_ve = random.randint(1, 4)
        self.dang, self.bam, self.t_mua = None, 0, None

    def _gui(self, loai, **kw):
        self.dang = loai
        return {"loai": loai, "nguoi": self.ai, **kw}

    def _mua(self):
        if self.t_mua is None:
            self.t_mua = time.perf_counter()
        return self._gui("mua", so_ve=self.so_ve)

    def yeu_cau_dau(self):
        if not self.nc.phong_cho:
            return self._mua()
        return self._gui("xep_hang")

    def sau_kq(self, tin):
        ma = tin.get("ma_loi")
        if self.dang == "xep_hang":
            if tin.get("ok"):
                return self._gui("cho_luot")
            if ma == "xung_dot" and self.bam < BAM_LAI:
                self.bam += 1
                return self._gui("xep_hang")
            DO_MUA.ket_thuc(ma)
            return None
        if self.dang == "cho_luot":
            if tin.get("ok"):
                return self._mua()
            DO_MUA.ket_thuc(ma)                         # hết vé khi đang đứng trong hàng
            return None
        if ma == "qua_tai":
            DO_MUA.dem_qua_tai()
        if ma in ("qua_tai", "xung_dot") and self.bam < BAM_LAI:
            self.bam += 1
            return self._mua()
        DO_MUA.ket_thuc("ok" if tin.get("ok") else ma, (time.perf_counter() - self.t_mua) * 1000)
        return None


TIEU_DE_PHONG_CHO = "  Phòng chờ Mua được Hết vé Quá tải Bỏ cuộc p50 ms p99 ms"


def bang_phong_cho(hang):
    """Các dòng bảng so sánh bật/tắt phòng chờ (≤ 64 ký tự). Mua được, Hết vé, Bỏ cuộc: số NGƯỜI
    (kết quả cuối); Quá tải: số LẦN bị từ chối "qua_tai"; p50/p99: thời gian mua của từng người."""
    ds = [TIEU_DE_PHONG_CHO]
    for h in hang:
        p50 = "–" if h["p50_ms"] is None else f"{h['p50_ms']:.1f}"
        p99 = "–" if h["p99_ms"] is None else f"{h['p99_ms']:.1f}"
        ds.append(f"  {h['phong_cho']:<9}{h['mua_duoc']:>9} {h['het_ve']:>6} {h['qua_tai']:>7}"
                  f" {h['bo_cuoc']:>7} {p50:>6} {p99:>6}")
    if hang:
        ds.append("  Thời gian cả lượt: " + ", ".join(f"{h['phong_cho']} {h['giay']:.2f} s" for h in hang))
    return ds


def so_sanh_phong_cho(host, cong, n, k, bao, che_do="khoa"):
    """Hai lượt ở cùng chế độ (mặc định khoa): phòng chờ TẮT rồi BẬT; đo thời gian mua của từng người."""
    global THAM_SO_DAT_LAI
    goc = THAM_SO_DAT_LAI
    hang = []
    try:
        for bat in (0, 1):
            THAM_SO_DAT_LAI = dict(goc, phong_cho=bat)
            DO_MUA.dat_lai()
            t0 = time.perf_counter()
            h = chay_luot(host, cong, n, k, 1, bao, che_do=che_do, dat_lai=True)
            if h is None:
                break
            with DO_MUA.khoa:
                tre = sorted(DO_MUA.tre)
                kq, qua_tai = dict(DO_MUA.kq), DO_MUA.qua_tai
            hang.append({"phong_cho": "bật" if bat else "tắt", "mua_duoc": kq.get("ok", 0), "qua_tai": qua_tai,
                         "het_ve": kq.get("het_ve", 0), "bo_cuoc": kq.get("qua_tai", 0),
                         "p50_ms": phan_vi(tre, 50), "p99_ms": phan_vi(tre, 99),
                         "giay": time.perf_counter() - t0, "sai_pham": h["sai_pham"]})
    finally:
        THAM_SO_DAT_LAI = goc
    return hang


# ===================================================================== KHUNG (giống nhau ở 9 bài)
class KetQuaKetNoi:
    def __init__(self):
        self.rtt, self.ok, self.tu_choi, self.ly_do = [], 0, 0, collections.Counter()
        self.loi, self.t_xong = None, None


def _chay_ket_noi(host, cong, i, ds_ten, lan, rao, kq):
    """Luồng của một kết nối: chào -> chờ rào -> gửi lô yêu cầu đầu -> đọc, gửi tiếp tới khi xong."""
    kn = KetNoiKhach()
    try:
        kn.ket_noi(host, cong, f"mp{i + 1}", so_nguoi=len(ds_ten))
        nc = NguCanh(kn.trang_thai)
        kn.khi_cap_nhat = nc.cap_nhat
        con = [len(ds_ten)]                     # số người chưa xong

        def gui(na, yc):
            if yc is None:
                con[0] -= 1
                return None
            loai = yc.pop("loai")
            return loai, yc, lambda tin, rtt: nhan(na, tin, rtt)

        def nhan(na, tin, rtt):                 # chạy trên chính luồng này (trong vong_doc)
            kq.rtt.append(rtt)
            if tin.get("ok"):
                kq.ok += 1
            else:
                kq.tu_choi += 1
                kq.ly_do[tin.get("ma_loi", "khac")] += 1
            nc.nhan_kq(tin)
            tiep = gui(na, na.sau_kq(tin))
            if tiep:
                kn.gui_yc(tiep[0], tiep[2], **tiep[1])

        ds_nguoi = [NguoiAo(ten, nc, lan) for ten in ds_ten]
        rao.wait()                              # RÀO XUẤT PHÁT: mọi kết nối sẵn sàng mới bắn
        lo = [x for x in (gui(na, na.yeu_cau_dau()) for na in ds_nguoi) if x]
        if lo:
            kn.gui_lo(lo)
        kn.vong_doc(xong=lambda: con[0] == 0)
        if con[0]:
            kq.loi = "máy chủ đóng kết nối giữa chừng"
    except Exception as e:                      # lỗi trước rào: phá rào để luồng chính biết
        kq.loi = str(e) or type(e).__name__
        rao.abort()
    finally:
        kq.t_xong = time.perf_counter()
        kn.dong()


def chay_luot(host, cong, n, k, lan, bao, che_do=None, dat_lai=False, tre_ms=None):
    """Một lượt mô phỏng; trả về một hàng của bảng so sánh (dict) hoặc None nếu lỗi."""
    k = max(1, min(k, n))
    qt = QuanTri(host, cong, "mo_phong")
    try:
        if tre_ms is not None:
            qt.tre(tre_ms)
        if dat_lai:                             # đặt lại trước (máy chủ in tổng kết lượt trước đúng chế độ)
            qt.dat_lai(**THAM_SO_DAT_LAI)
        if che_do:
            qt.che_do(che_do)
        che_do = qt.thong_ke()["che_do"]
        bao(f"=== {che_do}: {n} người × {lan} lần, {k} kết nối ===")
        nhom = [[] for _ in range(k)]
        for i in range(n):
            nhom[i % k].append(f"u{i + 1}")
        rao = threading.Barrier(k + 1)
        ket_qua = [KetQuaKetNoi() for _ in range(k)]
        luong = [threading.Thread(target=_chay_ket_noi, args=(host, cong, i, nhom[i], lan, rao, ket_qua[i]),
                                  daemon=True) for i in range(k)]
        for t in luong:
            t.start()
        try:
            rao.wait(timeout=60)
        except threading.BrokenBarrierError:
            loi = next((r.loi for r in ket_qua if r.loi), "hết thời gian chờ")
            bao(f"Không mở được đủ {k} kết nối: {loi}")
            return None
        t0 = time.perf_counter()
        bao(f"Rào xuất phát: {k}/{k} kết nối sẵn sàng -> bắn")
        for t in luong:
            t.join()
        t1 = max(r.t_xong for r in ket_qua)
        loi = [r.loi for r in ket_qua if r.loi]
        if loi:
            bao(f"{len(loi)} kết nối lỗi: {loi[0]}")
        rtt = sorted(x for r in ket_qua for x in r.rtt)
        ok = sum(r.ok for r in ket_qua)
        tu_choi = sum(r.tu_choi for r in ket_qua)
        ly_do = sum((r.ly_do for r in ket_qua), collections.Counter())
        tong = ok + tu_choi
        giay = max(t1 - t0, 1e-6)
        yc_s = round(tong / giay)
        p = {x: phan_vi(rtt, x) for x in (50, 95, 99)}
        bao(f"{tong} yc xong trong {giay:.2f} s = {yc_s} yc/s")
        bao("Khứ hồi ms: " + " ".join(f"p{x} {'–' if v is None else f'{v:.1f}'}" for x, v in p.items()))
        bao(f"Thành công {ok}, từ chối {tu_choi}")
        if ly_do:
            bao("  lý do: " + ", ".join(f"{m} {s}" for m, s in ly_do.most_common()))
        tk = qt.thong_ke()
        bao(f"Máy chủ: xung đột {tk['xung_dot']}, thử lại {tk['thu_lai']},"
            f" hàng đợi max {tk['hang_doi_max']}")
        bao(f"Máy chủ xử lý µs: p50 {so_us(tk['p50_us'])} p95 {so_us(tk['p95_us'])}"
            f" p99 {so_us(tk['p99_us'])}")
        bao(dong_sai_pham(tk))
        for v in tk["vi_du_sai_pham"][:2]:
            bao(f"  ví dụ: {v}")
        bao(tk["tom_tat"])
        return {"che_do": che_do, "thanh_cong": ok, "sai_pham": tk["sai_pham_tong"],
                "xung_dot": tk["xung_dot"], "yc_s": yc_s, "p99_ms": p[99], "tu_choi": tu_choi,
                "tom_tat": tk["tom_tat"]}
    finally:
        qt.dong()


def so_sanh(host, cong, n, k, lan, bao, tre_ms=None):
    """Chạy lần lượt 4 chế độ (đặt lại trạng thái trước mỗi lần); trả về các hàng; trả lại chế độ cũ."""
    qt = QuanTri(host, cong, "so_sanh")
    try:
        goc = qt.thong_ke()["che_do"]
        hang = []
        for cd in CHE_DO:
            h = chay_luot(host, cong, n, k, lan, bao, che_do=cd, dat_lai=True, tre_ms=tre_ms)
            if h is None:
                break
            hang.append(h)
        qt.che_do(goc)
        return hang
    finally:
        qt.dong()


def main():
    ap = argparse.ArgumentParser(description="Bộ mô phỏng tải – tranh chấp tài nguyên")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=41770)
    ap.add_argument("--n", type=int, default=200, help="số người dùng ảo")
    ap.add_argument("--k", type=int, default=None, help="số kết nối (mặc định min(n, 200))")
    ap.add_argument("--lan", type=int, default=SO_LAN_MAC_DINH, help="số yêu cầu mỗi người")
    ap.add_argument("--che-do", choices=CHE_DO, default=None, help="đổi chế độ máy chủ trước khi chạy")
    ap.add_argument("--dat-lai", action="store_true", help="đặt lại trạng thái trước khi chạy")
    ap.add_argument("--so-sanh", action="store_true", help="chạy cả 4 chế độ và in bảng so sánh")
    ap.add_argument("--tre-ms", type=float, default=None, help="đặt độ trễ xử lý tối đa của máy chủ")
    a = ap.parse_args()
    k = a.k if a.k else min(a.n, 200)

    def bao(text):
        print(f"[mo_phong] {text}", flush=True)
    if a.so_sanh:
        hang = so_sanh(a.host, a.cong, a.n, k, a.lan, bao, a.tre_ms)
        bao(f"Bảng so sánh ({a.n} người × {a.lan} lần, {min(k, a.n)} kết nối):")
        for d in bang_so_sanh(hang):
            print(d, flush=True)
    else:
        chay_luot(a.host, a.cong, a.n, k, a.lan, bao, a.che_do, a.dat_lai, a.tre_ms)


if __name__ == "__main__":
    main()
