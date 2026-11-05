"""Bài mở rộng MR6.8 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ (Tk) và ba ứng dụng đặt vé An, Bình, Chi
(Toplevel). Kịch bản bấm ghế, bấm nút như người dùng (generator: `yield giây` = chờ, `yield hàm` = chờ
tới khi điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in `[demo] …`:
  1–3. chế độ khoa: cả ba chọn E7 rồi CÙNG bấm "GIỮ CHỖ" -> một người giữ được, hai người bị từ chối;
       người giữ được thanh toán -> vé;
  4.   chế độ khong_khoa, trễ 0–20 ms: cả ba chọn E8, CÙNG bấm -> cả ba "Đã giữ", rồi hai người thấy
       "Ghế E8 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
  5.   So sánh 4 chế độ (mô phỏng 500 người cùng giữ ghế nóng E7, 200 kết nối) -> bảng;
  6.   đặt lại, giữ chỗ 6 s: An giữ F5, F6 không thanh toán; Bình thấy ghế đang giữ; hết hạn -> ghế
       trống lại, Bình giữ và mua; Chi mua G7, G8; An giữ E5, E6 (đồng hồ chạy), Chi chọn H6;
       chụp 4 cửa sổ ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 2600x1700x24" python3 demo_gui.py [host] [cổng]
"""
import os
import sys
import tkinter as tk

from chung import bang_so_sanh
from dat_ve_gui import CuaSoDatVe
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
    log("1. Máy chủ: START SERVER (chế độ khoa, giữ chỗ 30 s)")
    sv.nut_start.invoke()
    yield 0.3
    log("   An, Bình, Chi: Kết nối")
    for cs in ds:
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.tt for cs in ds)
    yield 1.0
    log("2. Cả ba chọn ghế E7 rồi CÙNG bấm 'GIỮ CHỖ'")
    for cs in ds:
        cs.bam("E7")
    for cs in ds:
        cs.nut_giu.invoke()
    yield lambda: all("Đang giữ chỗ" not in cs.thong_bao["text"] for cs in ds)
    yield 0.4
    in_cua_so(ds)
    in_server(sv)
    duoc = next(cs for cs in ds if cs.giu)
    log(f"3. {duoc.ten} bấm 'THANH TOÁN'")
    duoc.nut_tt.invoke()
    yield lambda: duoc.ve
    yield 0.4
    log(f"   {duoc.ten}: {duoc.thong_bao['text']}")
    log(f"   máy chủ: {sv.lop.thong_ke_day_du()['tom_tat']}")
    log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)")
    sv.o_tre.delete(0, "end")
    sv.o_tre.insert(0, "20")
    sv.nut_tre.invoke()
    sv.chon_che_do("khong_khoa")
    yield lambda: all(cs.tt["che_do"] == "khong_khoa" for cs in ds)
    yield 0.5
    log("   Cả ba chọn E8 rồi CÙNG bấm 'GIỮ CHỖ'")
    truoc = {cs.ten: cs.so_giu for cs in ds}
    for cs in ds:
        cs.bam("E8")
    for cs in ds:
        cs.nut_giu.invoke()
    yield lambda: all("Đang giữ chỗ" not in cs.thong_bao["text"] for cs in ds)
    for cs in ds:
        log(f"   {cs.ten}: máy chủ trả lời "
            + ("ĐÃ GIỮ E8" if cs.so_giu > truoc[cs.ten] else "từ chối"))
    yield 0.6
    log("   …sau cap_nhat của máy chủ:")
    in_cua_so(ds)
    in_server(sv)
    log(f"   E8 thuộc: {sv.lop.trang_thai()['chu'].get('E8')}")
    nan_nhan = next((cs for cs in ds if "lấy mất" in cs.thong_bao["text"]), binh)
    tam = [os.path.join(thu_muc, ".tc_0.png"), os.path.join(thu_muc, ".tc_1.png")]
    chup(sv.root, tam[0])
    chup(nan_nhan.root, tam[1])
    ra = os.path.join(thu_muc, "ket_qua_gui_tranh_chap.png")
    ghep_anh([tam], ra)
    log(f"   Đã ghi {os.path.basename(ra)} ({os.path.getsize(ra) // 1024} KB)")
    log("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng E7)")
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
    log("6. Đặt lại: giữ chỗ 6 s; An giữ F5, F6 (không thanh toán)")
    sv.dat_tham_so(ttl_s=6)
    sv.nut_dat_lai.invoke()
    yield lambda: all(cs.tt["ttl_s"] == 6 and not cs.tt["dem"]["ban"] for cs in ds)
    yield 0.8
    an.bam("F5")
    an.bam("F6")
    an.nut_giu.invoke()
    yield lambda: an.giu
    yield 0.5
    binh.bam("F6")
    log(f"   Bình bấm F6: {binh.thong_bao['text']}")
    yield lambda: not an.giu
    log(f"   hết hạn – An: {an.thong_bao['text']}")
    yield 0.3
    log("   Bình giữ F5, F6 rồi thanh toán; Chi mua G7, G8")
    for g in ("F5", "F6"):
        binh.bam(g)
    for g in ("G7", "G8"):
        chi.bam(g)
    binh.nut_giu.invoke()
    chi.nut_giu.invoke()
    yield lambda: binh.giu and chi.giu
    yield 0.4
    binh.nut_tt.invoke()
    chi.nut_tt.invoke()
    yield lambda: binh.ve and chi.ve
    yield 0.4
    log("   An giữ E5, E6; Chi chọn H6")
    an.bam("E5")
    an.bam("E6")
    an.nut_giu.invoke()
    chi.bam("H6")
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
    cong = int(sys.argv[2]) if len(sys.argv) > 2 else 41722
    thu_muc = os.path.dirname(os.path.abspath(__file__))
    root = tk.Tk()
    sv = tao_cua_so(root, host, cong)
    root.geometry("+0+0")
    root.update_idletasks()
    x, y = root.winfo_reqwidth() + 20, 0
    ds = []
    for i, ten in enumerate(("An", "Bình", "Chi")):
        cs = CuaSoDatVe(tk.Toplevel(root), ten, host, cong)
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
