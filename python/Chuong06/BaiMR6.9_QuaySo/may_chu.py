"""Bài mở rộng MR6.9 – Lõi máy chủ quay số trúng thưởng: nghiệp vụ + sổ kiểm toán (khung ở chung.py).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
Chương trình khuyến mãi "Vòng quay may mắn": 10 món quà, hàng chục nghìn lượt quay. Thao tác ghi duy nhất:
    {"loai":"quay","ma_yc":k[,"nguoi":"u17"]}
Luật: mỗi người quay ĐÚNG MỘT lần (quay lần nữa – dù bằng ma_yc khác, từ thiết bị khác – bị từ chối
"da_quay" kèm kết quả cũ: idempotency theo tên); mỗi lượt trúng với xác suất p (mặc định 1/2000) nếu
còn quà; trúng thì nhận ngẫu nhiên một món còn lại.

Tài nguyên trong KhoTranhChap:
  - ô "qua"      : {"con": (mã các món còn lại)} – ô "nóng" mà mọi lượt TRÚNG cùng tranh;
  - ô "ng:<tên>" : None (chưa quay) | {"qua": mã|None} – một ô cho mỗi người, tạo khi người đó quay lần
                   đầu. Lượt không trúng chỉ chạm ô của chính mình nên các lượt chạy song song được
                   (khóa theo tài nguyên); chỉ lượt trúng mới khóa thêm ô "qua".
Rút thăm (random() < p) làm TRƯỚC, ngoài vùng găng: kết quả rút thăm không phụ thuộc trạng thái.

Sổ kiểm toán đếm hai sai phạm:
  thua_qua    – một món quà đã trao lại được trao lần nữa (tổng số quà trao > số quà có);
  quay_2_lan  – một người có hai lượt quay được chấp nhận (hai thiết bị cùng bấm, ô của người đó bị
                ghi đè).
"""
import collections
import random
import threading
import time

from chung import MayChuCoSo, SoKiemToan

TEN_QUA = ["Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm điện",
           "Loa", "Phiếu 500k"]


def chu_p(p):
    """0.0005 -> "1/2000"; 0.3 -> "0.3"."""
    if p > 0 and abs(1 / p - round(1 / p)) < 1e-6 and round(1 / p) > 1:
        return f"1/{round(1 / p)}"
    return f"{p:g}"


class SoKiemToanQuaySo(SoKiemToan):
    TEN_SAI_PHAM = {"thua_qua": "thừa quà", "quay_2_lan": "quay 2 lần"}

    def __init__(self, so_qua):
        super().__init__()
        self.so_qua = so_qua
        self.da_quay = set()             # người đã có lượt quay được chấp nhận (theo sổ)
        self.da_trao = {}                # mã quà -> người nhận đầu tiên
        self.so_trao = 0                 # số lượt trúng được chấp nhận

    def _kiem_tra(self, gd):
        ng, q = gd["nguoi"], gd["qua"]
        if ng in self.da_quay:
            self._sai_pham("quay_2_lan", f"{ng} quay lần 2")
        self.da_quay.add(ng)
        if q is not None:
            self.so_trao += 1
            if q in self.da_trao:
                self._sai_pham("thua_qua", f"{TEN_QUA[q]}: {self.da_trao[q]} rồi {ng}")
            else:
                self.da_trao[q] = ng


class MayChuQuaySo(MayChuCoSo):
    TEN = "Quay số MR6.9"
    LOAI_YEU_CAU = ("quay",)
    THAM_SO_MAC_DINH = {"so_qua": 10, "p": 0.0005}

    # ------------------------------------------------------------ tạo / đặt lại chương trình
    def _kiem_tham_so(self, ts):
        super()._kiem_tham_so(ts)
        if not 1 <= ts.get("so_qua", 1) <= len(TEN_QUA):
            raise ValueError(f"số quà 1–{len(TEN_QUA)}")
        if not 0 <= ts.get("p", 0) <= 1:
            raise ValueError("xác suất p trong [0, 1]")

    def _khoi_tao(self):
        so_qua = self.tham_so["so_qua"]
        self.kho.dat_o("qua", {"con": tuple(range(so_qua))})
        self.so = SoKiemToanQuaySo(so_qua)
        self._khoa_tao = threading.Lock()                  # tạo ô "ng:<tên>" đúng một lần
        self._da_tao = set()
        self.nguoi_trung = collections.deque(maxlen=50)    # mới nhất trước (để hiển thị)
        self._bao_het = False

    def _o_nguoi(self, nguoi):
        k = "ng:" + nguoi
        with self._khoa_tao:
            if k not in self._da_tao:
                self._da_tao.add(k)
                self.kho.dat_o(k, None)
        return k

    # ------------------------------------------------------------ quay
    def _xu_ly_yeu_cau(self, khach, nguoi, tin, tra_loi):
        k_ng = self._o_nguoi(nguoi)
        rut_trung = random.random() < self.tham_so["p"]    # kim dừng ở ô quà với xác suất p
        cac_khoa = [k_ng]
        if rut_trung and self.kho.doc("qua")["con"]:       # hết quà thì khỏi chạm ô "qua" (quà không tăng lại)
            cac_khoa.append("qua")

        def xu_ly(doc):                                    # hàm THUẦN: đọc doc, trả về (kq, ghi)
            cu = doc[k_ng]
            if cu is not None:
                truoc = "không trúng" if cu["qua"] is None else f"trúng {TEN_QUA[cu['qua']]}"
                return {"ok": False, "ma_loi": "da_quay", "ly_do": f"đã quay rồi ({truoc})",
                        "qua": cu["qua"]}, None
            q = doc.get("qua")
            if q is not None and q["con"]:
                chon = random.choice(q["con"])
                con = tuple(x for x in q["con"] if x != chon)
                return ({"ok": True, "trung": True, "qua": chon, "ten_qua": TEN_QUA[chon], "nguoi": nguoi,
                         "con_lai": len(con)},
                        {k_ng: {"qua": chon}, "qua": {"con": con}})
            return ({"ok": True, "trung": False, "qua": None, "nguoi": nguoi},
                    {k_ng: {"qua": None}})

        gop = khach.so_nguoi is not None                 # người ảo của bộ mô phỏng: không báo từng người
        self.kho.thuc_hien(cac_khoa, xu_ly, tra_loi, khi_ghi=lambda kq: self._da_quay(kq, gop))

    def _da_quay(self, kq, gop):
        """Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi."""
        self.so.ghi(nguoi=kq["nguoi"], qua=kq["qua"])
        if kq["trung"]:
            self.nguoi_trung.appendleft({"nguoi": kq["nguoi"], "qua": kq["qua"], "gio": time.strftime("%H:%M:%S")})
            if not gop:
                self.thong_bao(f"{kq['nguoi']} trúng {kq['ten_qua']}")
            if kq["con_lai"] == 0 and not self._bao_het:
                self._bao_het = True
                self.thong_bao(f"Đã trao hết quà sau {len(self.so.giao_dich)} lượt")
        self.bao_doi()

    # ------------------------------------------------------------ trạng thái gửi client
    def _trang_thai(self):
        con = set(self.kho.doc("qua")["con"])
        trung = list(self.nguoi_trung)
        nhan = {}
        for x in reversed(trung):                           # người nhận gần nhất của mỗi món
            nhan[x["qua"]] = x["nguoi"]
        so_qua = self.tham_so["so_qua"]
        return {"so_qua": so_qua, "p": self.tham_so["p"], "con_lai": len(con),
                "qua": [{"ma": i, "ten": TEN_QUA[i], "con": i in con, "nguoi": None if i in con else nhan.get(i)}
                        for i in range(so_qua)],
                "da_quay": len(self.so.giao_dich), "so_trung": self.so.so_trao,
                "nguoi_trung": trung[:20]}

    def _tom_tat(self):
        return (f"Trao {self.so.so_trao}/{self.tham_so['so_qua']} quà, {len(self.so.giao_dich)} lượt,"
                f" p {chu_p(self.tham_so['p'])}")


_khoa_in = threading.Lock()


def in_dong(tien_to, text):
    """Nhiều luồng cùng in: mỗi dòng in trọn vẹn."""
    with _khoa_in:
        print(f"[{tien_to}] {text}", flush=True)
