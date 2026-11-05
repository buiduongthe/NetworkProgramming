"""Bài mở rộng MR6.9 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba vòng quay: An, Bình và
"Bình (máy 2)" – cùng một người mở trên hai thiết bị (Toplevel). Kịch bản bấm nút như người dùng
(generator: `yield giây` = chờ, `yield hàm` = chờ tới khi điều kiện đúng), in `[demo] …`:
  1–2. chế độ khoa, 1 món quà, p = 1: cả ba cùng bấm QUAY -> đúng một người trúng, thiết bị thứ hai
       của Bình bị từ chối "đã quay";
  3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "trúng" một món quà -> SAI PHẠM
       (thừa quà, quay 2 lần); chụp ket_qua_gui_tranh_chap.png;
  4.   So sánh 4 chế độ (10 000 người, p = 1/20, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
  5–6. 10 quà, p = 0.8: An, Bình quay; Bình (máy 2) quay -> "đã quay"; bắn đồng loạt 500 người ->
       danh sách người trúng trực tiếp; chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from giao_dien_chung import chay_kich_ban, chup, ghep_anh
from server_gui import tao_cua_so
from vong_quay_gui import CuaSoVongQuay


def log(text):
    print(f"[demo] {text}", flush=True)


def ten_cs(cs):
    return getattr(cs, "nhan_hien", cs.ten)


def in_cua_so(ds):
    for cs in ds:
        log(f"   {ten_cs(cs)}: {cs.thong_bao['text']}")


def in_server(sv):
    tk_ = sv.lop.thong_ke_day_du()
    log(f"   máy chủ: {tk_['so_yc']} yc, tc {tk_['thanh_cong']}, từ chối {tk_['tu_choi']},"
        f" sai phạm {tk_['sai_pham_tong']}")
    for v in tk_["vi_du_sai_pham"][:2]:
        log(f"     ví dụ: {v}")
    log(f"   {tk_['tom_tat']}")


def dat_lai(sv, **ts):
    sv.dat_tham_so(**ts)
    sv.nut_dat_lai.invoke()


def cung_bam(ds):
    for cs in ds:
        cs.nut_quay.invoke()


def kich_ban(root, sv, ds, thu_muc):
    an, binh, binh2 = ds
    yield 0.5
    log("1. Máy chủ: START SERVER (chế độ khoa)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Bình (máy 2): Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    log("2. Đặt lại: 1 món quà, p = 1 (lượt đầu chắc trúng)")
    dat_lai(sv, so_qua=1, p=1)
    yield lambda: all(cs.tt["so_qua"] == 1 for cs in ds)
    yield 0.5
    log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY")
    cung_bam(ds)
    yield 0.3
    yield lambda: all(cs.pha is None for cs in ds)
    in_cua_so(ds)
    in_server(sv)
    log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "20")
    sv.nut_tre.invoke()
    dat_lai(sv, so_qua=1, p=1)
    sv.chon_che_do("khong_khoa")
    yield lambda: sv.lop.kho.che_do == "khong_khoa"
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" and cs.nut_quay["state"] == "normal" for cs in ds)
    yield 0.5
    log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY")
    cung_bam(ds)
    yield 0.3
    yield lambda: all(cs.pha is None for cs in ds)
    yield 0.5
    in_cua_so(ds)
    in_server(sv)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(binh2.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("4. Máy chủ: khoa, trễ 0–2 ms; 10 quà, p = 0.05 (1/20);")
    log("   So sánh 4 chế độ, 10000 người")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "2")
    sv.nut_tre.invoke()
    dat_lai(sv, so_qua=10, p=0.05)
    sv.chon_che_do("khoa")
    yield lambda: sv.lop.kho.che_do == "khoa"
    sv.o_n.delete(0, "end")
    sv.o_n.insert(0, "10000")
    yield 0.3
    sv.nut_so_sanh.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    for d in bang_so_sanh(sv.hang_so_sanh):
        log(d)
    for cau in sv.nhan_ket_luan["text"].split(". "):
        log(f"   {cau.rstrip('.')}")
    yield 0.5
    log("5. Đặt lại: 10 quà, p = 0.8; An quay, rồi Bình quay")
    dat_lai(sv, so_qua=10, p=0.8)
    yield lambda: all(cs.tt["p"] == 0.8 and cs.nut_quay["state"] == "normal" for cs in ds)
    yield 0.5
    an.nut_quay.invoke()
    yield 0.4
    binh.nut_quay.invoke()
    yield 0.3
    yield lambda: an.pha is None and binh.pha is None
    log("   Bình (máy 2) bấm QUAY")
    binh2.nut_quay.invoke()
    yield 0.3
    yield lambda: binh2.pha is None
    in_cua_so(ds)
    log("6. Bắn đồng loạt 500 người (chế độ khoa)")
    sv.o_n.delete(0, "end")
    sv.o_n.insert(0, "500")
    sv.nut_ban.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    yield 0.8
    sv._xong_mo_phong(sv.hang_so_sanh)                 # bảng: hiện lại kết quả so sánh 4 chế độ
    in_server(sv)
    log("   Người trúng gần nhất (cửa sổ An):")
    for x in an.ls.get(0, 3):
        log(f"     {x}")
    log("7. Chụp 4 cửa sổ")
    tam = [os.path.join(thu_muc, f".gui_{i}.png") for i in range(4)]
    for t, w in zip(tam, [sv.root, an.root, binh.root, binh2.root]):
        chup(w, t)
    ra = os.path.join(thu_muc, "ket_qua_gui.png")
    ghep_anh([tam[:2], tam[2:]], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("8. Máy chủ: STOP SERVER")
    sv.nut_stop.invoke()
    yield lambda: sv.nut_start["state"] == "normal"
    yield 0.5
    log(f"   An: {an.thanh_tt['text']}")
    for cs in ds:
        cs.dong()
    sv.dong()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41732
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Bình")):
        cs = CuaSoVongQuay(tk.Toplevel(root), ten, host, cong)
        if i == 2:                                      # cùng người, thiết bị thứ hai
            cs.nhan_hien = "Bình (máy 2)"
            cs.root.title("Vòng quay may mắn – Bình (máy 2)")
        cs.root.update_idletasks()
        if i == 1:                                      # Bình, Bình (máy 2): hàng dưới
            x, y = 0, max(root.winfo_reqheight(), ds[0].root.winfo_reqheight()) + 40
        cs.root.geometry(f"+{x}+{y}")
        x += cs.root.winfo_reqwidth() + 20
        ds.append(cs)
    chay_kich_ban(root, kich_ban(root, sv, ds, thu_muc), log, het_gio=120)
    root.mainloop()
    log("Kết thúc")


if __name__ == "__main__":
    main()
