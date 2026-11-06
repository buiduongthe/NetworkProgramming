"""Bài mở rộng MR6.9 – Bộ mô phỏng tải: n người dùng ảo trên k kết nối, rào xuất phát, pipelining.

Phần KHUNG (từ "KHUNG" trở xuống) giống nhau ở 9 bài tranh chấp; mỗi bài chỉ sửa phần NGHIỆP VỤ:
  - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: số quà còn lại), cập nhật từ cap_nhat;
  - NguoiAo: kịch bản của MỘT người dùng ảo: quay `lan` lần (mặc định 1). Người thứ 100, 200, 300…
    dùng TÊN của người liền trước (một người quay trên hai thiết bị cùng lúc) – máy chủ phải từ chối
    lượt thứ hai ("da_quay"); lan > 1 thì các lượt sau cũng phải bị từ chối.
  - THAM_SO_DAT_LAI: {} – giữ số quà và xác suất p hiện tại của máy chủ (đặt bằng server.py --p,
    khach.py dat_lai p=…, hoặc ô tham số của server_gui.py).

Một lượt: kết nối k socket (người i đi trên kết nối i mod k; kết nối chào kèm "so_nguoi"), mọi luồng
đứng chờ ở RÀO XUẤT PHÁT (threading.Barrier) -> cùng lúc mỗi kết nối gửi yêu cầu đầu của mọi người
mình chở trong MỘT lần sendall; trả lời tới đâu gửi yêu cầu kế tiếp của người đó tới đó. Đo thời gian
tổng, yc/s, khứ hồi p50/p95/p99, rồi hỏi thong_ke của máy chủ (sai phạm theo sổ kiểm toán).

Chạy:  python3 mo_phong.py <host> <cổng> [--n 100000] [--k 200] [--lan 1] [--che-do khoa] [--dat-lai]
                          [--so-sanh] [--tre-ms 2]
"""
import argparse
import collections
import random
import threading
import time

from chung import CHE_DO, bang_so_sanh, dong_sai_pham, phan_vi, so_us
from khach import KetNoiKhach, QuanTri

# ===================================================================== NGHIỆP VỤ (MR6.9 – quay số)
SO_LAN_MAC_DINH = 1                       # mỗi người quay 1 lần
THAM_SO_DAT_LAI = {}                      # giữ số quà, p hiện tại của máy chủ
HAI_THIET_BI = 100                        # người thứ 100, 200, … quay bằng tên người liền trước


class NguCanh:
    """Số quà còn lại mà một kết nối biết (chỉ để tham khảo – máy chủ mới quyết định)."""

    def __init__(self, trang_thai):
        self.con_lai = trang_thai["con_lai"]

    def cap_nhat(self, tin):
        self.con_lai = tin["con_lai"]

    def nhan_kq(self, tin):
        pass


class NguoiAo:
    """Quay `lan` lần; người thứ 100, 200, … mang tên người liền trước (thiết bị thứ hai của người đó)."""

    def __init__(self, ten, nc, lan):
        i = int(ten[1:])
        self.ten = f"u{i - 1}" if i % HAI_THIET_BI == 0 else ten
        self.nc, self.con = nc, lan

    def _quay(self):
        if self.con <= 0:
            return None
        self.con -= 1
        return {"loai": "quay", "nguoi": self.ten}

    def yeu_cau_dau(self):
        return self._quay()

    def sau_kq(self, tin):
        return self._quay()


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
    ap.add_argument("cong", nargs="?", type=int, default=41730)
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
