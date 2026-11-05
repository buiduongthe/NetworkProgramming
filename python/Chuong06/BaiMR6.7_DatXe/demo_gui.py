"""Bài mở rộng MR6.7 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba ứng dụng gọi xe An, Bình, Chi
(Toplevel). Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ, `yield hàm` = chờ tới khi
điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in `[demo] …`:
  1–3. chế độ khoa, 1 xe: An, Bình, Chi CÙNG bấm "ĐẶT XE" -> một người được xe, hai người "Hết chỗ";
       người được xe hủy chuyến;
  4.   chế độ khong_khoa, trễ 0–20 ms: cả ba CÙNG bấm -> cả ba "Đặt thành công – tài xế Hùng", rồi hai
       người thấy "Tài xế Hùng đã nhận khách khác!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
  5.   đặt lại 3 xe, So sánh 4 chế độ (mô phỏng 1000 người, 200 kết nối) -> bảng, khong_khoa tô đỏ;
  6.   đặt lại (3 xe): An, Bình, Chi cùng đặt -> mỗi người một xe, xe chạy tới điểm đón;
       chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from dat_xe_gui import CuaSoGoiXe
from giao_dien_chung import chay_kich_ban, chup, ghep_anh
from server_gui import tao_cua_so


def log(text):
    print(f"[demo] {text}", flush=True)


def in_cua_so(ds):
    for cs in ds:
        log(f"   {cs.ten}: {cs.nhan_tt['text']}")


def in_server(sv):
    tk_ = sv.lop.thong_ke_day_du()
    log(f"   máy chủ: {tk_['so_yc']} yc, tc {tk_['thanh_cong']}, từ chối {tk_['tu_choi']},"
        f" sai phạm {tk_['sai_pham_tong']}")
    for v in tk_["vi_du_sai_pham"][:2]:
        log(f"     ví dụ: {v}")


def cung_bam(ds):
    for cs in ds:                                       # ba lần bấm liền nhau trên luồng giao diện
        cs.nut_dat.invoke()


def kich_ban(root, sv, ds, thu_muc):
    an, binh, chi = ds
    yield 0.5
    log("1. Máy chủ: START SERVER (chế độ khoa, 1 xe)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    yield 1.0
    log("2. An, Bình, Chi CÙNG bấm 'ĐẶT XE'")
    cung_bam(ds)
    yield lambda: all(cs.trang != "dang_tim" for cs in ds)
    yield 0.5
    in_cua_so(ds)
    in_server(sv)
    duoc = next(cs for cs in ds if cs.trang == "da_dat")
    yield 1.0
    log(f"3. {duoc.ten} bấm 'Hủy chuyến'")
    duoc.nut_huy.invoke()
    yield lambda: duoc.trang == "xong"
    yield 0.5
    log(f"   {duoc.ten}: {duoc.nhan_tt['text']}")
    log(f"   máy chủ: {sv.lop.thong_ke_day_du()['tom_tat']}")
    log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "20")
    sv.nut_tre.invoke()
    sv.chon_che_do("khong_khoa")
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" for cs in ds)
    yield 0.5
    log("   An, Bình, Chi CÙNG bấm 'ĐẶT XE'")
    cung_bam(ds)
    yield lambda: all(cs.trang != "dang_tim" for cs in ds)
    for cs in ds:
        log(f"   {cs.ten} nhận: {cs.nhan_tt['text']}")
    yield 0.6
    log("   …sau cap_nhat của máy chủ:")
    in_cua_so(ds)
    in_server(sv)
    log(f"   xe 1 chở: {sv.lop.trang_thai()['xe'][0]['khach']}")
    nan_nhan = next((cs for cs in ds if cs.trang == "mat_xe"), binh)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(nan_nhan.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("5. Máy chủ: khoa, trễ 0–2 ms, đặt lại 3 xe")
    log("   So sánh 4 chế độ (1000 người)")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "2")
    sv.nut_tre.invoke()
    sv.chon_che_do("khoa")
    sv.dat_tham_so(so_xe=3)
    sv.nut_dat_lai.invoke()
    sv.o_n.delete(0, "end")
    sv.o_n.insert(0, "1000")
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
    log("6. Đặt lại (3 xe); An, Bình, Chi CÙNG bấm 'ĐẶT XE'")
    sv.nut_dat_lai.invoke()
    yield lambda: all(cs.tt["trong"] == 3 for cs in ds)
    yield 0.8
    cung_bam(ds)
    yield lambda: all(cs.trang != "dang_tim" for cs in ds)
    yield 1.6
    in_cua_so(ds)
    for cs in ds:
        if cs.chuyen:
            log(f"   {cs.ten}: {cs.nhan_tx['text'].split()[0]} {cs.chuyen['bien_so']},"
                f" đến sau {cs.chuyen['phut']} phút")
    log(f"   máy chủ: {sv.lop.thong_ke_day_du()['tom_tat']}")
    log("7. Chụp 4 cửa sổ")
    tam = [os.path.join(thu_muc, f".gui_{i}.png") for i in range(4)]
    for t, w in zip(tam, [sv.root, an.root, binh.root, chi.root]):
        chup(w, t)
    ra = os.path.join(thu_muc, "ket_qua_gui.png")
    ghep_anh([tam[:2], tam[2:]], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    yield lambda: all(cs.hoat_hinh is None or cs._da_toi() for cs in ds)
    yield 0.3
    in_cua_so(ds)
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
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41712
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoGoiXe(tk.Toplevel(root), ten, host, cong)
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
