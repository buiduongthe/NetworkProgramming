"""Bài mở rộng MR6.12 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba ứng dụng ngân hàng An (TK01),
Bình (TK02), Chi (TK03) (Toplevel). Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ,
`yield hàm` = chờ tới khi điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in `[demo] …`:
  1–2. chế độ khoa (khóa theo thứ tự mã): An→Bình và Bình→An CÙNG LÚC -> cả hai thành công;
  3.   bật "khóa ngây thơ" + Đặt lại, trễ 0–50 ms: An→Bình và Bình→An cùng lúc -> KHÓA CHẾT, sau 2 s
       một giao dịch bị hủy ("khóa chết: …"), giao dịch kia đi tiếp; chụp ket_qua_gui_khoa_chet.png;
  4.   tắt "khóa ngây thơ" + Đặt lại: lặp lại -> cả hai thành công ngay;
  5.   chế độ khong_khoa, trễ 0–20 ms: An→Bình, Bình→Chi, Chi→An cùng lúc -> TỔNG TIỀN LỆCH (đỏ);
       chụp ket_qua_gui_tranh_chap.png;
  6.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
  7.   Đặt lại, An chuyển cho Chi; chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from giao_dien_chung import chay_kich_ban, chup, ghep_anh
from ngan_hang_gui import CuaSoNganHang
from server_gui import tao_cua_so


def log(text):
    print(f"[demo] {text}", flush=True)


def in_cua_so(ds):
    for cs in ds:
        log(f"   {cs.ten}: " + cs.thong_bao["text"].split("\n")[0])


def in_server(sv):
    tk_ = sv.lop.thong_ke_day_du()
    log(f"   máy chủ: {tk_['so_yc']} yc, tc {tk_['thanh_cong']}, từ chối {tk_['tu_choi']},"
        f" sai phạm {tk_['sai_pham_tong']}")
    log(f"   {tk_['tom_tat']}")
    for v in tk_["vi_du_sai_pham"][:2]:
        log(f"     ví dụ: {v}")


def chuyen(cs, den, so_tien):
    cs.chon_nguoi_nhan(den)
    cs._dat_tien(so_tien)
    cs.nut_chuyen.invoke()


def tre(sv, ms):
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, str(ms))
    sv.nut_tre.invoke()


def cheo(an, binh):
    """An→Bình 500.000 đ và Bình→An 300.000 đ trong cùng một nhịp giao diện (cùng lúc)."""
    dem = an.so_kq + binh.so_kq
    chuyen(an, "TK02", 500_000)
    chuyen(binh, "TK01", 300_000)
    return lambda: an.so_kq + binh.so_kq == dem + 2


def kich_ban(root, sv, ds, thu_muc):
    an, binh, chi = ds
    yield 0.5
    log("1. Máy chủ: START SERVER (khoa, khóa theo thứ tự mã)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối (TK01, TK02, TK03)")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    yield 1.0
    log("2. An→Bình 500.000 đ và Bình→An 300.000 đ CÙNG LÚC")
    yield cheo(an, binh)
    yield 0.4
    in_cua_so(ds[:2])
    log("3. Bật 'khóa ngây thơ', Đặt lại; trễ 0–50 ms")
    sv.dat_khoa_ngay_tho(True)
    sv.nut_dat_lai.invoke()
    tre(sv, 50)
    yield lambda: an.tt["khoa_ngay_tho"] and an.tt["so_gd"] == 0
    yield 0.5
    log("   An→Bình và Bình→An cùng lúc…")
    yield cheo(an, binh)
    yield 0.4
    in_cua_so(ds[:2])
    log(f"   máy chủ: khóa chết phát hiện {sv.lop.trang_thai()['khoa_chet']}")
    tam = [os.path.join(thu_muc, f".kc_{i}.png") for i in range(3)]
    for t, w in zip(tam, [sv.root, an.root, binh.root]):
        chup(w, t)
    ra = os.path.join(thu_muc, "ket_qua_gui_khoa_chet.png")
    ghep_anh([tam[:1], tam[1:]], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("4. Tắt 'khóa ngây thơ', Đặt lại; lặp lại chuyển chéo")
    sv.dat_khoa_ngay_tho(False)
    sv.nut_dat_lai.invoke()
    yield lambda: not an.tt["khoa_ngay_tho"] and an.tt["so_gd"] == 0
    yield 0.5
    yield cheo(an, binh)
    yield 0.4
    in_cua_so(ds[:2])
    log("5. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại")
    tre(sv, 20)
    sv.chon_che_do("khong_khoa")
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" for cs in ds)
    sv.nut_dat_lai.invoke()
    yield lambda: an.tt["so_gd"] == 0
    yield 0.5
    log("   An→Bình, Bình→Chi, Chi→An 1.000.000 đ cùng lúc")
    dem = sum(cs.so_kq for cs in ds)
    for cs, den in zip(ds, ("TK02", "TK03", "TK01")):
        chuyen(cs, den, 1_000_000)
    yield lambda: sum(cs.so_kq for cs in ds) == dem + 3
    yield 0.6
    in_cua_so(ds)
    in_server(sv)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(chi.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("6. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ")
    tre(sv, 2)
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
    log("7. Đặt lại; An chuyển 200.000 đ cho Chi")
    sv.nut_dat_lai.invoke()
    yield lambda: an.tt["so_gd"] == 0
    yield 0.5
    dem = an.so_kq
    chuyen(an, "TK03", 200_000)
    yield lambda: an.so_kq == dem + 1
    yield 0.8
    in_cua_so([an])
    log(f"   Chi: {chi.nhan_den['text']}")
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
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41762
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoNganHang(tk.Toplevel(root), ten, host, cong)
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
