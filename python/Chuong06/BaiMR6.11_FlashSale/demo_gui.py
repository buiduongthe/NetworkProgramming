"""Bài mở rộng MR6.11 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba trang flash sale của An, Bình,
Chi (Toplevel). Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ, `yield hàm` = chờ tới
khi điều kiện đúng), in `[demo] …`:
  1–2. chế độ khoa, kho 3 sản phẩm, mở bán sau 3 s: nút MUA NGAY khóa trong lúc đếm ngược; vừa mở,
       An mua 2, Bình 1, Chi 2 cùng lúc -> tổng bán đúng 3, người đến sau bị từ chối;
  3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "đặt hàng thành công" 5 sản phẩm
       từ kho 3 -> SAI PHẠM (tồn âm, lệch sổ); chụp ket_qua_gui_tranh_chap.png;
  4.   So sánh 4 chế độ (10 000 người, kho 100, 200 kết nối, trễ 0–1 ms) -> bảng, dòng khong_khoa đỏ;
  5–6. đợt mới: kho 100, mở sau 4 s (chụp trang của Chi lúc đếm ngược); mở bán: An mua 2,
       50 người ảo cùng mua, Bình mua 2 -> thanh "SẮP CHÁY HÀNG"; chụp, ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from flash_sale_gui import CuaSoFlashSale
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
    log(f"   {tk_['tom_tat']}")


def dat_lai(sv, **ts):
    sv.dat_tham_so(**ts)
    sv.nut_dat_lai.invoke()


def dat_tre(sv, ms):
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, str(ms))
    sv.nut_tre.invoke()


def kich_ban(root, sv, ds, thu_muc):
    an, binh, chi = ds
    yield 0.5
    log("1. Máy chủ: START SERVER (chế độ khoa)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    log("2. Đặt lại: kho 3 sản phẩm, mở bán sau 3 s")
    dat_lai(sv, ton_dau=3, mo_sau_s=3)
    yield lambda: all(cs.tt["ton_dau"] == 3 for cs in ds)
    yield 0.5
    log(f"   An thấy: {an.nhan_dong_ho['text']}, nút {an.nut_mua['text']!r}"
        f" ({an.nut_mua['state']})")
    yield lambda: all(cs.nut_mua["state"] == "normal" for cs in ds)
    log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY")
    for cs, sl in zip(ds, (2, 1, 2)):
        cs.mua(sl)
    yield lambda: all(not cs.dang_cho for cs in ds)
    yield 0.5
    in_cua_so(ds)
    in_server(sv)
    log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; đặt lại")
    dat_tre(sv, 20)
    dat_lai(sv, ton_dau=3, mo_sau_s=2)
    sv.chon_che_do("khong_khoa")
    yield lambda: sv.lop.kho.che_do == "khong_khoa"
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" and cs.nut_mua["state"] == "normal" for cs in ds)
    log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY")
    for cs, sl in zip(ds, (2, 1, 2)):
        cs.mua(sl)
    yield lambda: all(not cs.dang_cho for cs in ds)
    yield 0.6
    in_cua_so(ds)
    in_server(sv)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(chi.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("4. Máy chủ: khoa, trễ 0–1 ms, kho 100; So sánh 4 chế độ")
    dat_tre(sv, 1)
    dat_lai(sv, ton_dau=100, mo_sau_s=30)
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
    log("5. Đặt lại: kho 100, mở bán sau 4 s")
    dat_lai(sv, ton_dau=100, mo_sau_s=4)
    yield lambda: all(cs.tt["ton"] == 100 and not cs.tt["mo_ban"] for cs in ds)
    yield 1.5
    tam = [os.path.join(thu_muc, f".gui_{i}.png") for i in range(4)]
    log(f"   Chi thấy: {chi.nhan_trang_thai['text']} {chi.nhan_dong_ho['text']},"
        f" nút {chi.nut_mua['text']!r}")
    chup(chi.root, tam[3])
    yield lambda: an.nut_mua["state"] == "normal"
    log("6. Mở bán: An mua 2; 50 người ảo cùng mua; Bình mua 2")
    an.mua(2)
    yield lambda: not an.dang_cho
    sv.o_n.delete(0, "end")
    sv.o_n.insert(0, "50")
    sv.nut_ban.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    sv._xong_mo_phong(sv.hang_so_sanh)                 # bảng: hiện lại kết quả so sánh 4 chế độ
    binh.mua(2)
    yield lambda: not binh.dang_cho
    yield 0.6
    in_cua_so([an, binh])
    in_server(sv)
    log("7. Chụp cửa sổ (Chi: lúc đếm ngược)")
    for t, w in zip(tam[:3], [sv.root, an.root, binh.root]):
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
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41752
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoFlashSale(tk.Toplevel(root), ten, host, cong)
        cs.root.update_idletasks()
        if i == 1:                                      # Bình, Chi: hàng dưới
            x, y = 0, max(root.winfo_reqheight(), ds[0].root.winfo_reqheight()) + 40
        cs.root.geometry(f"+{x}+{y}")
        x += cs.root.winfo_reqwidth() + 20
        ds.append(cs)
    chay_kich_ban(root, kich_ban(root, sv, ds, thu_muc), log, het_gio=120)
    root.mainloop()
    log("Kết thúc")


if __name__ == "__main__":
    main()
