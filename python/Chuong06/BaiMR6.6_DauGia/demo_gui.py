"""Bài mở rộng MR6.6 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba phòng đấu giá An, Bình, Chi
(Toplevel). Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ, `yield hàm` = chờ tới khi
điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in `[demo] …`:
  1–3. chế độ khoa: Bình và Chi cùng bấm "+1 bước" -> một người bị từ chối "giá đã thay đổi";
  4.   chế độ khong_khoa: ba người cùng bấm "+1 bước" -> cả ba cùng "được chấp nhận" -> SAI PHẠM;
       chụp ket_qua_gui_tranh_chap.png;
  5.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
  6–7. phiên 8 s, gia hạn 4 s: trả giá phút cuối -> gia hạn hai lần -> kết thúc, công bố người thắng;
       chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import time
import tkinter as tk

from chung import bang_so_sanh, tien
from dau_gia_gui import CuaSoDauGia
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
    log("1. Máy chủ: START SERVER (chế độ khoa, phiên 120 s)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    yield 1.0
    log("2. An bấm '+1 bước'")
    an.nut_buoc[0][1].invoke()
    yield 0.8
    log("3. Bình và Chi CÙNG bấm '+1 bước' (cùng giá)")
    binh.nut_buoc[0][1].invoke()
    chi.nut_buoc[0][1].invoke()
    yield 0.8
    in_cua_so(ds)
    in_server(sv)
    log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "20")
    sv.nut_tre.invoke()
    sv.chon_che_do("khong_khoa")
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" for cs in ds)
    yield 0.5
    log("   An, Bình, Chi CÙNG bấm '+1 bước'")
    for cs in ds:
        cs.nut_buoc[0][1].invoke()
    yield 0.8
    in_cua_so(ds)
    in_server(sv)
    log("   Lịch sử trả giá (mới nhất trước):")
    for x in sv.lop.trang_thai()["lich_su"][:4]:
        log(f"     {x['gio']} {x['nguoi']:<5} {tien(x['gia'])}")
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(binh.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("5. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "2")
    sv.nut_tre.invoke()
    sv.chon_che_do("khoa")
    sv.o_n.delete(0, "end")
    sv.o_n.insert(0, "500")
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
    log("6. Đặt lại: phiên 8 s, gia hạn 4 s; An '+1 bước'")
    sv.dat_tham_so(thoi_gian_s=8, gia_han_s=4)
    sv.nut_dat_lai.invoke()
    yield 1.0
    an.nut_buoc[0][1].invoke()
    yield 4.2
    log(f"   Bình '+2 bước' khi còn {an.nhan_dong_ho['text']}")
    binh.nut_buoc[1][1].invoke()
    yield 0.5
    log(f"   -> đồng hồ An: {an.nhan_dong_ho['text']} {an.nhan_gia_han['text']}")
    yield lambda: an.het_han is not None and an.het_han - time.monotonic() < 0.8
    log(f"7. An '+1 bước' khi còn {an.nhan_dong_ho['text']}")
    an.nut_buoc[0][1].invoke()
    yield 0.5
    log(f"   -> đồng hồ Bình: {binh.nhan_dong_ho['text']} {binh.nhan_gia_han['text']}")
    yield lambda: an.tt["ket_thuc"]
    yield 0.8
    in_cua_so(ds)
    log(f"   máy chủ: {sv.lop.thong_ke_day_du()['tom_tat']}")
    log("8. Chụp 4 cửa sổ")
    tam = [os.path.join(thu_muc, f".gui_{i}.png") for i in range(4)]
    for t, w in zip(tam, [sv.root, an.root, binh.root, chi.root]):
        chup(w, t)
    ra = os.path.join(thu_muc, "ket_qua_gui.png")
    ghep_anh([tam[:2], tam[2:]], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("9. Máy chủ: STOP SERVER")
    sv.nut_stop.invoke()
    yield lambda: sv.nut_start["state"] == "normal"
    yield 0.5
    log(f"   Bình: {binh.thanh_tt['text']}")
    for cs in ds:
        cs.dong()
    sv.dong()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41702
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoDauGia(tk.Toplevel(root), ten, host, cong)
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
