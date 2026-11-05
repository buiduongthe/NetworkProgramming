"""Bài mở rộng MR6.10 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba cổng đăng ký của An, Bình, Chi
(Toplevel). Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ, `yield hàm` = chờ tới khi
điều kiện đúng), in `[demo] …`:
  1–2. chế độ khoa, sĩ số 1: cả ba cùng bấm "Đăng ký" lớp 123013-01 -> một người được, hai người
       "Lớp đã đủ";
  3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba "thành công" -> SAI PHẠM vượt sĩ số;
       danh sách lớp chỉ còn một tên, hai cửa sổ kia báo "đăng ký đã bị MẤT";
       chụp ket_qua_gui_tranh_chap.png;
  4.   So sánh 4 chế độ (2000 sinh viên, 3 lớp × 50, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
  5–6. đợt mới (3 lớp × 50): An đăng ký 2 lớp, Bình 1 lớp; 500 sinh viên ảo đăng ký -> lớp đủ;
       Chi đăng ký lớp đủ -> "Lớp đã đủ"; An hủy một lớp -> Chi đăng ký được chỗ vừa nhả;
       chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from dang_ky_gui import CuaSoDangKy
from giao_dien_chung import chay_kich_ban, chup, ghep_anh
from server_gui import tao_cua_so

LOP1, LOP2, LOP3 = "123013-01", "123005-01", "006124-01"


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


def in_lop(sv):
    for x in sv.lop.trang_thai()["lop"]:
        log(f"   {x['ma']}: {x['da_dk']}/{x['si_so']} {' '.join(x['sv'][:3])}".rstrip())


def dat_lai(sv, **ts):
    sv.dat_tham_so(**ts)
    sv.nut_dat_lai.invoke()


def xong(ds):
    return lambda: all(not cs.dang_cho for cs in ds)


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
    log("2. Đặt lại: 3 lớp, sĩ số 1 (còn đúng 1 chỗ mỗi lớp)")
    dat_lai(sv, so_lop=3, si_so=1)
    yield lambda: all(cs.tt["si_so"] == 1 for cs in ds)
    yield 0.5
    log(f"   An, Bình, Chi CÙNG bấm Đăng ký {LOP1}")
    for cs in ds:
        cs.dang_ky(LOP1)
    yield xong(ds)
    yield 0.8
    in_cua_so(ds)
    in_server(sv)
    log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "20")
    sv.nut_tre.invoke()
    dat_lai(sv, so_lop=3, si_so=1)
    sv.chon_che_do("khong_khoa")
    yield lambda: sv.lop.kho.che_do == "khong_khoa"
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" and not cs.cua_toi for cs in ds)
    yield 0.5
    log(f"   An, Bình, Chi CÙNG bấm Đăng ký {LOP1}")
    for cs in ds:
        cs.dang_ky(LOP1)
    yield xong(ds)
    log("   ngay khi có trả lời:")
    in_cua_so(ds)
    yield 1.2
    log("   1 giây sau (đối chiếu danh sách lớp):")
    in_cua_so(ds)
    in_server(sv)
    in_lop(sv)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    mat = next((cs for cs in ds if "MẤT" in cs.thong_bao["text"]), binh)
    chup(mat.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB, cửa sổ {mat.ten})")
    log("4. Máy chủ: khoa, trễ 0–2 ms, 3 lớp × 50; So sánh")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "2")
    sv.nut_tre.invoke()
    dat_lai(sv, so_lop=3, si_so=50)
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
    log("5. Đặt lại; An đăng ký 2 lớp, Bình 1 lớp")
    dat_lai(sv, so_lop=3, si_so=50)
    yield lambda: all(not cs.cua_toi and cs.hang[LOP1]["nut"]["text"] == "Đăng ký" for cs in ds)
    yield 0.5
    an.dang_ky(LOP1)
    an.dang_ky(LOP2)
    binh.dang_ky(LOP1)
    yield xong(ds)
    yield 0.5
    log("   Bắn đồng loạt 500 sinh viên ảo (mỗi người 1 lớp)")
    sv.o_n.delete(0, "end")
    sv.o_n.insert(0, "500")
    sv.nut_ban.invoke()
    yield 0.3
    yield lambda: not sv.dang_mo_phong
    yield 0.8
    sv._xong_mo_phong(sv.hang_so_sanh)                 # bảng: hiện lại kết quả so sánh 4 chế độ
    in_lop(sv)
    log(f"   Chi bấm {chi.hang[LOP2]['nut']['text']!r} ở {LOP2}")
    chi.dang_ky(LOP2)
    yield xong(ds)
    yield 0.5
    in_cua_so([chi])
    log(f"6. An bấm Hủy {LOP2}, rồi Chi bấm Đăng ký {LOP2}")
    an.dang_ky(LOP2)
    yield xong(ds)
    yield lambda: chi.hang[LOP2]["nut"]["text"] == "Đăng ký"
    chi.dang_ky(LOP2)
    yield xong(ds)
    yield 0.6
    in_cua_so(ds)
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
    log(f"   An: {an.thanh_tt['text']}")
    for cs in ds:
        cs.dong()
    sv.dong()


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41742
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoDangKy(tk.Toplevel(root), ten, host, cong)
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
