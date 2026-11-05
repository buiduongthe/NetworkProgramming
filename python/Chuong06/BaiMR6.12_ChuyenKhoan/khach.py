"""Bài mở rộng MR6.12 – Lõi client dùng chung (giao diện người dùng, bộ mô phỏng, quản trị) + dòng lệnh quản trị.

Tệp này GIỐNG NHAU ở cả 8 bài tranh chấp (MR6.6–MR6.13). Không có gì về nghiệp vụ.

KetNoiKhach – một kết nối TCP:
  ket_noi(...)      chào, đọc trả lời chào và cap_nhat đầu tiên (đồng bộ) -> self.trang_thai
  gui_yc(loai, khi_kq, **truong)   gửi một yêu cầu (tự đánh ma_yc), KHÔNG chờ: khi_kq(tin, rtt_ms) được
                    gọi khi có trả lời cùng ma_yc (pipelining: gửi tiếp được ngay)
  gui_lo([...])     gửi nhiều yêu cầu trong MỘT lần sendall (bộ mô phỏng "bắn đồng loạt")
  hoi(loai, **truong)  gửi rồi CHỜ trả lời (cần luồng đọc chạy nền) – dùng cho quản trị
  bat_dau_doc()     luồng đọc chạy nền (giao diện);  vong_doc(xong)  đọc ngay trên luồng gọi (mô phỏng)
Gọi lại (từ luồng đọc): khi_cap_nhat(tin), khi_tin(tin) (su_kien, loi, trả lời lạ), khi_dong().
QuanTri – kết nối vai_tro "quan_tri": che_do(), tre(), dat_lai(**ts), thong_ke().

Dòng lệnh quản trị:  python3 khach.py <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
"""
import itertools
import socket
import sys
import threading
import time

from chung import doc_dong, doc_tham_so, dong_thong_ke, gui_json, ma_hoa


class KetNoiKhach:
    def __init__(self, khi_cap_nhat=None, khi_tin=None, khi_dong=None):
        self.khi_cap_nhat = khi_cap_nhat or (lambda tin: None)
        self.khi_tin = khi_tin or (lambda tin: None)
        self.khi_dong = khi_dong or (lambda: None)
        self.sock = self._tep = None
        self.mo = False
        self.chao = self.trang_thai = None
        self._ma = itertools.count(1)
        self._cho = {}                       # ma_yc -> (khi_kq, thời điểm gửi)
        self._khoa = threading.Lock()        # bảo vệ _cho và việc gửi

    # ------------------------------------------------------------ kết nối
    def ket_noi(self, host, cong, ten, vai_tro="nguoi_dung", so_nguoi=None, cho_s=5.0):
        sock = socket.create_connection((host, cong), timeout=cho_s)
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        chao = {"loai": "chao", "ten": ten, "vai_tro": vai_tro}
        if so_nguoi is not None:
            chao["so_nguoi"] = so_nguoi
        try:
            gui_json(sock, chao)
            tep = sock.makefile("rb")
            tin = doc_dong(tep)
            if tin is None or tin.get("loai") != "chao":
                raise ConnectionError((tin or {}).get("ly_do", "máy chủ đóng kết nối"))
            self.chao = tin
            self.trang_thai = doc_dong(tep)        # máy chủ gửi ngay một cap_nhat sau lời chào
            sock.settimeout(None)
        except BaseException:
            sock.close()
            raise
        self.sock, self._tep, self.mo = sock, tep, True
        return tin

    def dong(self):
        self.mo = False
        if self.sock:
            try:
                self.sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.sock.close()

    # ------------------------------------------------------------ gửi
    def _dang_ky(self, loai, khi_kq, truong):
        ma = next(self._ma)
        self._cho[ma] = (khi_kq, time.perf_counter())
        return ma, {"loai": loai, "ma_yc": ma, **truong}

    def gui_yc(self, loai, khi_kq=None, **truong):
        with self._khoa:
            ma, tin = self._dang_ky(loai, khi_kq, truong)
            gui_json(self.sock, tin)
        return ma

    def gui_lo(self, ds):
        """ds: danh sách (loai, truong_dict, khi_kq). Một lần sendall cho cả lô."""
        with self._khoa:
            du_lieu = b"".join(ma_hoa(self._dang_ky(loai, khi_kq, truong)[1]) for loai, truong, khi_kq in ds)
            self.sock.sendall(du_lieu)

    def hoi(self, loai, cho_s=30.0, **truong):
        xong, kq = threading.Event(), {}

        def nhan(tin, rtt):
            kq.update(tin)
            xong.set()
        self.gui_yc(loai, nhan, **truong)
        if not xong.wait(cho_s):
            raise TimeoutError(f"không có trả lời cho {loai}")
        return kq

    # ------------------------------------------------------------ đọc
    def bat_dau_doc(self):
        threading.Thread(target=self.vong_doc, daemon=True).start()

    def vong_doc(self, xong=None):
        """Đọc và phân phát tới khi kết nối đóng, hoặc (nếu có) xong() đúng sau một thông điệp."""
        try:
            while True:
                if xong and xong():
                    return                         # đủ việc: kết nối vẫn mở, người gọi tự đóng
                tin = doc_dong(self._tep)
                if tin is None:
                    break
                self._phan_phat(tin)
        except (OSError, ValueError):
            pass
        self.mo = False
        self.khi_dong()

    def _phan_phat(self, tin):
        ma = tin.get("ma_yc")
        with self._khoa:
            cho = self._cho.pop(ma, None) if ma is not None and not tin.get("lap_lai") else None
        if cho is not None:
            khi_kq, t0 = cho
            if khi_kq:
                khi_kq(tin, (time.perf_counter() - t0) * 1000)
        elif tin.get("loai") == "cap_nhat":
            self.trang_thai = tin
            self.khi_cap_nhat(tin)
        else:
            self.khi_tin(tin)


class QuanTri(KetNoiKhach):
    """Kết nối quản trị (đồng bộ): mỗi lệnh chờ trả lời; lỗi -> ValueError."""

    def __init__(self, host, cong, ten="quan_tri"):
        super().__init__()
        self.ket_noi(host, cong, ten, "quan_tri")
        self.bat_dau_doc()

    def _lenh(self, loai, **truong):
        kq = self.hoi(loai, **truong)
        if not kq.get("ok", True):
            raise ValueError(kq.get("ly_do", "bị từ chối"))
        return kq

    def che_do(self, gia_tri):
        return self._lenh("che_do", gia_tri=gia_tri)

    def tre(self, ms):
        return self._lenh("tre", ms=ms)

    def dat_lai(self, **tham_so):
        return self._lenh("dat_lai", **tham_so)

    def thong_ke(self):
        return self._lenh("thong_ke")


def main():
    if len(sys.argv) < 4:
        print("Dùng: khach.py <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke")
        return
    host, cong, lenh, ts = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4:]

    def bao(text):
        print(f"[quan_tri] {text}", flush=True)
    qt = QuanTri(host, cong)
    try:
        bao(f"> {' '.join([lenh] + ts)}")
        if lenh == "che_do" and len(ts) == 1:
            qt.che_do(ts[0])
        elif lenh == "tre" and len(ts) == 1:
            qt.tre(float(ts[0]))
        elif lenh == "dat_lai":
            qt.dat_lai(**doc_tham_so(ts))
        elif lenh != "thong_ke":
            bao("lệnh không hợp lệ")
            return
        bao("OK")
        if lenh == "thong_ke":
            for d in dong_thong_ke(qt.thong_ke()):
                bao(d)
    except ValueError as e:
        bao(f"Bị từ chối: {e}")
    finally:
        qt.dong()


if __name__ == "__main__":
    main()
