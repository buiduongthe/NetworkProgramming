"""Bài mở rộng MR6.14 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba ứng dụng đặt vé An, Bình, Chi
(Toplevel). Kịch bản chọn đoạn, bấm chỗ, bấm nút như người dùng (generator: `yield giây` = chờ,
`yield hàm` = chờ tới khi điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in `[demo] …`:
  1–3. xe giường nằm, chế độ khoa: An và Bình chọn D01 Sài Gòn → Đà Lạt, Chi chọn D01 Đà Lạt → Nha Trang,
       cả ba CÙNG bấm "GIỮ CHỖ" -> một trong An/Bình giữ được, Chi CŨNG giữ được (đoạn không chồng nhau),
       người còn lại bị từ chối; hai người giữ được thanh toán -> cùng một giường bán hai vé;
  4.   chế độ khong_khoa, trễ 0–20 ms: cả ba chọn D02 Sài Gòn → Nha Trang, CÙNG bấm -> cả ba "Đã giữ",
       rồi hai người thấy "Chỗ D02 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
  5.   So sánh 4 chế độ (mô phỏng 500 người cùng giữ ghế nóng D01, đoạn ngẫu nhiên, 200 kết nối) -> bảng;
  6.   đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s: An giữ 05, 06 Sài Gòn → Bảo Lộc, không thanh toán; Bình
       bấm 06 cho Sài Gòn → Đà Lạt: đang giữ; hết hạn -> Bình giữ và mua 05, 06 Sài Gòn → Đà Lạt; Chi mua
       05 Đà Lạt → Nha Trang (cùng ghế, đoạn sau); An giữ 10, 11 (đồng hồ chạy); chụp 4 cửa sổ ghép
       thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from dat_ve_xe_gui import CuaSoDatVeXe
from giao_dien_chung import chay_kich_ban, chup, ghep_anh
from server_gui import tao_cua_so


def log(text):
    print(f"[demo] {text}", flush=True)


def in_cua_so(ds):
    for cs in ds:
        log(f"   {cs.ten}: {cs.thong_bao['text']}")


def in_server(sv):
    tk_ = sv.lop.thong_ke_day_du()
    log(f"   máy chủ: {tk_['so_yc']} yc, tc {tk_['thanh_cong']}, từ chối {tk_['tu_choi']},"
        f" sai phạm {tk_['sai_pham_tong']}")
    for v in tk_["vi_du_sai_pham"][:2]:
        log(f"     ví dụ: {v}")


def kich_ban(root, sv, ds, thu_muc):
    an, binh, chi = ds
    yield 0.5
    log("1. Máy chủ: START SERVER (chế độ khoa, xe giường nằm, giữ chỗ 30 s)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    yield 1.0
    log("2. An, Bình: D01 Sài Gòn → Đà Lạt; Chi: D01 Đà Lạt → Nha Trang; CÙNG bấm 'GIỮ CHỖ'")
    an.dat_doan(0, 2)
    binh.dat_doan(0, 2)
    chi.dat_doan(2, 3)
    for cs in ds:
        cs.bam("D01")
    for cs in ds:
        cs.nut_giu.invoke()
    yield lambda: all("Đang giữ chỗ" not in cs.thong_bao["text"] for cs in ds)
    yield 0.4
    in_cua_so(ds)
    in_server(sv)
    duoc = [cs for cs in ds if cs.giu]
    log(f"3. {' và '.join(cs.ten for cs in duoc)} bấm 'THANH TOÁN'")
    for cs in duoc:
        cs.nut_tt.invoke()
    yield lambda: all(cs.ve for cs in duoc)
    yield 0.4
    for cs in duoc:
        log(f"   {cs.ten}: {cs.thong_bao['text']}")
    log(f"   máy chủ: {sv.lop.thong_ke_day_du()['tom_tat']}")
    log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "20")
    sv.nut_tre.invoke()
    sv.chon_che_do("khong_khoa")
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" for cs in ds)
    yield 0.5
    log("   Cả ba chọn D02 Sài Gòn → Nha Trang rồi CÙNG bấm 'GIỮ CHỖ'")
    truoc = {cs.ten: cs.so_giu for cs in ds}
    for cs in ds:
        cs.dat_doan(0, 3)
        cs.bam("D02")
    for cs in ds:
        cs.nut_giu.invoke()
    yield lambda: all("Đang giữ chỗ" not in cs.thong_bao["text"] for cs in ds)
    for cs in ds:
        log(f"   {cs.ten}: máy chủ trả lời " + ("ĐÃ GIỮ D02" if cs.so_giu > truoc[cs.ten] else "từ chối"))
    yield 0.6
    log("   …sau cap_nhat của máy chủ:")
    in_cua_so(ds)
    in_server(sv)
    tt = sv.lop.trang_thai()
    log("   D02 thuộc: " + ", ".join(str(tt["chu"].get(f"D02#{k}")) for k in range(3)))
    nan_nhan = next((cs for cs in ds if "lấy mất" in cs.thong_bao["text"]), binh)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(nan_nhan.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng D01, đoạn ngẫu nhiên)")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "2")
    sv.nut_tre.invoke()
    sv.chon_che_do("khoa")
    yield lambda: sv.lop.kho.che_do == "khoa"
    yield 0.3
    sv.nut_so_sanh.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    for d in bang_so_sanh(sv.hang_so_sanh):
        log(d)
    for cau in sv.nhan_ket_luan["text"].split(". "):
        log(f"   {cau.rstrip('.')}")
    yield 0.5
    log("6. Đặt lại: xe ghế ngồi 50 chỗ, giữ chỗ 6 s; An giữ 05, 06 Sài Gòn → Bảo Lộc (không thanh toán)")
    sv.dat_tham_so(ttl_s=6, loai_xe="ghe")
    sv.nut_dat_lai.invoke()
    yield lambda: all(cs.tt["ttl_s"] == 6 and cs.tt["loai_xe"] == "ghe" and not cs.tt["dem"]["ban"] for cs in ds)
    yield 0.8
    an.dat_doan(0, 1)
    an.bam("05")
    an.bam("06")
    an.nut_giu.invoke()
    yield lambda: an.giu
    yield 0.5
    binh.dat_doan(0, 2)
    binh.bam("06")
    log(f"   Bình bấm 06 (Sài Gòn → Đà Lạt): {binh.thong_bao['text']}")
    yield lambda: not an.giu
    log(f"   hết hạn – An: {an.thong_bao['text']}")
    yield 0.3
    log("   Bình giữ 05, 06 Sài Gòn → Đà Lạt rồi thanh toán; Chi mua 05 Đà Lạt → Nha Trang")
    for g in ("05", "06"):
        binh.bam(g)
    chi.dat_doan(2, 3)
    chi.bam("05")
    binh.nut_giu.invoke()
    chi.nut_giu.invoke()
    yield lambda: binh.giu and chi.giu
    yield 0.4
    binh.nut_tt.invoke()
    chi.nut_tt.invoke()
    yield lambda: binh.ve and chi.ve
    yield 0.4
    log("   An giữ 10, 11 Sài Gòn → Nha Trang; Chi chọn 21")
    an.dat_doan(0, 3)
    an.bam("10")
    an.bam("11")
    an.nut_giu.invoke()
    chi.dat_doan(0, 3)
    chi.bam("21")
    yield lambda: an.giu
    yield 1.5
    in_cua_so(ds)
    log(f"   An: đồng hồ giữ chỗ {an.nhan_dong_ho['text']}")
    log(f"   máy chủ: {sv.lop.thong_ke_day_du()['tom_tat']}")
    in_server(sv)
    log("7. Chụp 4 cửa sổ")
    tam = [os.path.join(thu_muc, f".gui_{i}.png") for i in range(4)]
    for t, w in zip(tam, [sv.root, an.root, binh.root, chi.root]):
        chup(w, t)
    ra = os.path.join(thu_muc, "ket_qua_gui.png")
    ghep_anh([tam[:2], tam[2:]], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("8. Máy chủ: STOP SERVER")
    sv.nut_stop.invoke()
    yield lambda: sv.nut_start["state"] == "normal"
    yield 0.5
    log(f"   Bình: {binh.thanh_tt['text']}")
    for cs in ds:
        cs.dong()
    sv.dong()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41782
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoDatVeXe(tk.Toplevel(root), ten, host, cong)
        cs.root.update_idletasks()
        if i == 1:                                      # Bình, Chi: hàng dưới
            x, y = 0, max(root.winfo_reqheight(), ds[0].root.winfo_reqheight()) + 40
        cs.root.geometry(f"+{x}+{y}")
        x += cs.root.winfo_reqwidth() + 20
        ds.append(cs)
    chay_kich_ban(root, kich_ban(root, sv, ds, thu_muc), log, het_gio=90)
    root.mainloop()
    log("Kết thúc")


if __name__ == "__main__":
    main()
