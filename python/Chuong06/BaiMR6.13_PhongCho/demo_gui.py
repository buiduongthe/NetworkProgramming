"""Bài mở rộng MR6.13 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba trang bán vé An, Bình, Chi
(Toplevel). Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ, `yield hàm` = chờ tới khi
điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in `[demo] …`:
  1–3. chế độ khoa, phòng chờ bật (50 người/s, 2000 vé): Chi vào hàng đầu tiên -> tới lượt ngay, mua 2 vé;
       mô phỏng 300 người ùa vào; An, Bình vào sau -> trang phòng chờ "Còn khoảng … người phía trước";
       chụp ket_qua_gui_phong_cho.png; tới lượt An -> mua 3 vé;
  4.   chế độ khong_khoa, trễ 0–20 ms: Chi rời, đăng nhập lại tên An (TAB THỨ HAI của An); hai tab cùng vào
       hàng, cùng được lượt, cùng bấm THANH TOÁN -> cả hai đơn được nhận -> SAI PHẠM "sai lượt";
       chụp ket_qua_gui_tranh_chap.png;
  5.   So sánh 4 chế độ (1000 người, 200 kết nối), rồi So sánh bật/tắt phòng chờ;
  6.   phòng chờ 20 người/s: Bình tới lượt (đồng hồ đếm ngược), Chi mua 1 vé, 400 người ùa vào, An xếp sau;
       chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

import mo_phong
from chung import bang_so_sanh
from giao_dien_chung import chay_kich_ban, chup, ghep_anh
from phong_cho_gui import CuaSoPhongCho
from server_gui import tao_cua_so


def log(text):
    print(f"[demo] {text}", flush=True)


def in_server(sv):
    tk_ = sv.lop.thong_ke_day_du()
    log(f"   máy chủ: {tk_['so_yc']} yc, tc {tk_['thanh_cong']}, từ chối {tk_['tu_choi']},"
        f" sai phạm {tk_['sai_pham_tong']}")
    log(f"   {tk_['tom_tat']}")
    for v in tk_["vi_du_sai_pham"][:2]:
        log(f"     ví dụ: {v}")


def tre(sv, ms):
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, str(ms))
    sv.nut_tre.invoke()


def mua(cs, n):
    cs.bien_so_ve.set(n)
    cs._tinh_tien()
    cs.nut_mua.invoke()


def ban(sv, n, k):
    """Bấm "Bắn đồng loạt" với n người trên k kết nối."""
    for o, v in ((sv.o_n, n), (sv.o_k, k)):
        o.delete(0, "end")
        o.insert(0, str(v))
    sv.nut_ban.invoke()


def dat_lai(sv, ds, **ts):
    sv.dat_tham_so(**ts)
    sv.nut_dat_lai.invoke()
    for cs in ds:
        cs.ve_su_kien()


def kich_ban(root, sv, ds, thu_muc):
    an, binh, chi = ds
    yield 0.5
    log("1. Máy chủ: START SERVER (khoa, phòng chờ bật)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    yield 0.8
    log("2. Đặt lại: 2000 vé, cấp lượt 50 người/s, thùng 50")
    dat_lai(sv, ds, tong_ve=2000, toc_do=50, dung_luong=50, phong_cho=1)
    yield lambda: chi.tt["su_kien"]["tong_ve"] == 2000
    yield 0.4
    log("   Chi: VÀO HÀNG CHỜ (người đầu tiên)")
    chi.nut_vao.invoke()
    yield lambda: chi.trang is chi.trang_mua
    log(f"   Chi: {chi.bang_luot['text']}")
    log("   Mô phỏng: 300 người ùa vào hàng (Bắn đồng loạt)")
    ban(sv, 300, 100)
    yield lambda: sv.lop.trang_thai()["so_cap"] >= 250
    log("   An, Bình: VÀO HÀNG CHỜ")
    an.nut_vao.invoke()
    binh.nut_vao.invoke()
    yield lambda: an.trang is an.trang_cho and binh.trang is binh.trang_cho
    yield 0.5
    log("   Chi: chọn 2 vé, THANH TOÁN")
    mua(chi, 2)
    yield 1.0
    log(f"   Chi: {chi.nhan_kq['text']}")
    log(f"   An: {an.nhan_so['text']}")
    log(f"   An: {an.nhan_truoc['text']}")
    tam = [os.path.join(thu_muc, f".pc_{i}.png") for i in range(3)]
    for t, w in zip(tam, [sv.root, an.root, chi.root]):
        chup(w, t)
    ra = os.path.join(thu_muc, "ket_qua_gui_phong_cho.png")
    ghep_anh([tam[:1], tam[1:]], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    yield lambda: an.trang is an.trang_mua
    yield 0.5
    log(f"3. An tới lượt: đồng hồ {an.nhan_dong_ho['text']}; chọn 3 vé, THANH TOÁN")
    mua(an, 3)
    yield lambda: an.so_don == 1
    log(f"   An: {an.nhan_kq['text']}")
    yield lambda: not sv.dang_mo_phong
    in_server(sv)
    log("4. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại (500 vé)")
    tre(sv, 20)
    sv.chon_che_do("khong_khoa")
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" for cs in ds)
    dat_lai(sv, ds, tong_ve=500)
    yield lambda: an.tt["su_kien"]["tong_ve"] == 500 and an.tt["da_cap_luot"] == 0
    log("   Chi: Rời, đăng nhập lại tên 'An' (tab thứ hai của An)")
    chi.nut_roi.invoke()
    chi.o_ten.delete(0, "end")
    chi.o_ten.insert(0, "An")
    chi.nut_ket_noi.invoke()
    yield lambda: chi.kn is not None and chi.tt
    yield 0.5
    log("   Hai tab của An cùng VÀO HÀNG CHỜ")
    an.nut_vao.invoke()
    chi.nut_vao.invoke()
    yield lambda: an.trang is an.trang_mua and chi.trang is chi.trang_mua
    log("   Hai tab cùng bấm THANH TOÁN 2 vé (một lượt)")
    dem = an.so_kq + chi.so_kq
    mua(an, 2)
    mua(chi, 2)
    yield lambda: an.so_kq + chi.so_kq == dem + 2
    yield 0.5
    log(f"   tab 1: {an.nhan_kq['text']}")
    log(f"   tab 2: {chi.nhan_kq['text']}")
    in_server(sv)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(chi.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("5. khoa, trễ 0–2 ms; Chi vào lại; So sánh 4 chế độ")
    chi.nut_roi.invoke()
    chi.o_ten.delete(0, "end")
    chi.o_ten.insert(0, "Chi")
    chi.nut_ket_noi.invoke()
    tre(sv, 2)
    sv.chon_che_do("khoa")
    yield lambda: sv.lop.kho.che_do == "khoa" and chi.kn is not None
    for o, v in ((sv.o_n, 1000), (sv.o_k, 200)):
        o.delete(0, "end")
        o.insert(0, str(v))
    yield 0.3
    sv.nut_so_sanh.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    for d in bang_so_sanh(sv.hang_so_sanh):
        log(d)
    for cau in sv.nhan_ket_luan["text"].split(". "):
        log(f"   {cau.rstrip('.')}")
    log("   So sánh bật/tắt phòng chờ (khoa, 1000 người)")
    sv.nut_so_sanh_pc.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    for d in mo_phong.bang_phong_cho(sv.hang_pc):
        log(d)
    log("6. Đặt lại: 2000 vé, 20 người/s, thùng 2")
    dat_lai(sv, ds, tong_ve=2000, toc_do=20, dung_luong=2, phong_cho=1)
    yield lambda: an.tt["su_kien"]["tong_ve"] == 2000 and an.tt["toc_do"] == 20
    yield 0.4
    log("   Bình, Chi vào hàng; Chi mua 1 vé")
    binh.nut_vao.invoke()
    chi.nut_vao.invoke()
    yield lambda: binh.trang is binh.trang_mua and chi.trang is chi.trang_mua
    mua(chi, 1)
    log("   400 người ùa vào; An vào hàng sau họ")
    ban(sv, 400, 100)
    yield lambda: sv.lop.trang_thai()["so_cap"] >= 350
    an.nut_vao.invoke()
    yield lambda: an.trang is an.trang_cho
    yield 2.0
    log(f"   An: {an.nhan_truoc['text']}")
    log(f"   Bình: đồng hồ {binh.nhan_dong_ho['text']}")
    log(f"   Chi: {chi.nhan_kq['text']}")
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
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41772
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoPhongCho(tk.Toplevel(root), ten, host, cong)
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
