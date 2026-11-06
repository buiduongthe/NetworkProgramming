"""Bài mở rộng MR6.5 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: cửa sổ máy chủ (Tk), máy phát P, người xem A và B (Toplevel).
P dùng camera giả lập "bảng clapper" và micro giả lập; A vào ngay, đồng bộ tiếng-hình BẬT; B bỏ chọn
đồng bộ rồi vào muộn ~3 s. Kịch bản bấm nút như người dùng (generator: `yield giây` = chờ,
`yield hàm` = chờ tới khi điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in số liệu `[demo] …`,
chụp 4 cửa sổ ghép thành ket_qua_gui.png. Có ffmpeg: thêm một lượt "Mẫu thử ffmpeg (testsrc)" và một
lượt "Tệp video" (tạo tệp mẫu mp4 có chớp + bíp mỗi giây, phát cả hình lẫn tiếng của tệp; chụp máy phát
và người xem A thành ket_qua_gui_tep.png).

Chạy:  xvfb-run -a -s "-screen 0 2000x1400x24" python3 demo_gui.py [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
"""
import os
import sys
import tkinter as tk

import chup_anh
from camera import MAU_THU, co_ffmpeg
from tep_video import tao_tep_mau
from phat_gui import CuaSoPhat
from server_gui import CuaSoServer
from xem_gui import CuaSoXem


def log(text):
    print(f"[demo] {text}", flush=True)


def ms(x, dau=False):
    return "–" if x is None else (f"{x:+.0f}" if dau else f"{x:.0f}")


def in_so_lieu(sv, p, ds):
    t = sv.lop.trang_thai()
    log(f"   server: vào {t['kbps_vao']:.0f}, ra {t['kbps_ra']:.0f} kbit/s,"
        f" hệ số {t['he_so']:.2f}")
    for x in t["nguoi_xem"]:
        log(f"     -> {x['ten']}: {x['nhan']}, {x['kbps']:.0f} kbit/s, I đầu {ms(x['khung_dau_ms'])} ms"
            f" ({x['nguon_dau'] or '–'})")
    tp = p.lop.trang_thai()
    log(f"   P: I {tp['so_i']} P {tp['so_p']}, TB I {tp['tb_i']:.0f} B, P {tp['tb_p']:.0f} B,"
        f" PLI {tp['pli_nhan']}")
    for ten, cs in ds.items():
        tt = cs.lop.trang_thai()
        g = tt["giai_doan"][-1]
        lech = g["tong"] / g["n"] if g["n"] else None
        log(f"   {ten}: đồng bộ {'BẬT' if tt['dong_bo'] else 'TẮT'}, khung đầu {ms(tt['khung_dau_ms'])} ms"
            f" ({tt['nguon_dau']})")
        log(f"      lệch TB {ms(lech, True)} ms, trễ {ms(tt['tre_tb'])} ms, mất tiếng {tt['mat_am']},"
            f" đứng {tt['dung_hinh']}")


def chup(cua_so, duong):
    cua_so.update()
    chup_anh.chup(cua_so, duong)


def kich_ban(root, sv, p, ds, out):
    a, b = ds["A"], ds["B"]
    yield 0.5
    log("1. Server: bấm START SERVER")
    sv.nut_start.invoke()
    log("2. P: Kết nối; camera giả lập (clapper), micro giả lập")
    p.chon_mic.current(len(p.ds_mic) - 1)          # kịch bản luôn dùng micro giả lập (mặc định là micro thật)
    p.nut_ket_noi.invoke()
    yield lambda: p.nut_phat["state"] == "normal"
    log("   P bấm PHÁT (10 fps, GOP 20)")
    p.nut_phat.invoke()
    yield 1.0
    log("3. A: Kết nối ngay (đồng bộ tiếng-hình BẬT)")
    a.nut_ket_noi.invoke()
    yield 3.5
    log("4. B: bỏ chọn 'Đồng bộ', Kết nối (vào muộn 3,5 s)")
    b.bien_dong_bo.set(False)
    b.nut_ket_noi.invoke()
    yield lambda: b.lop.khung_dau_ms is not None
    yield 3.0
    log("5. Số liệu (bộ đệm GOP bật):")
    in_so_lieu(sv, p, ds)
    log("6. P bấm 'Khung khóa ngay'; A bấm 'Chụp'")
    p.nut_i.invoke()
    yield 0.6
    duong = a.chup()
    log(f"   A đã chụp {duong}")
    yield 0.5
    log("7. Chụp 4 cửa sổ")
    tam = []
    for i, w in enumerate([root, p.root, a.root, b.root]):
        tam.append(f".gui_{i}.png")
        chup(w, tam[-1])
    chup_anh.convert(["(", tam[0], tam[1], "-background", "#d0d0d0", "-gravity", "north",
                      "+append", ")", "(", tam[2], tam[3], "+append", ")", "-background", "#d0d0d0",
                      "-gravity", "west", "-append", "-bordercolor", "#d0d0d0", "-border", "4", out])
    for f in tam:
        os.remove(f)
    log(f"   Đã ghi {out} ({os.path.getsize(out) // 1024} KB)")
    log("8. Server: chọn B, bấm 'Ngắt người chọn'")
    sv.chon_nguoi_xem("B")
    sv.nut_ngat.invoke()
    yield 0.8
    log(f"   B: {b.thanh_tt['text']}")
    if co_ffmpeg():
        log("9. P: DỪNG PHÁT, chọn 'Mẫu thử ffmpeg (testsrc)', PHÁT")
        p.nut_dung.invoke()
        yield 0.5
        p.chon_cam.current([m for m, _ in p.ds_cam].index(MAU_THU))
        p.nut_phat.invoke()
        yield 4.0
        tp = p.lop.trang_thai()
        log(f"   P testsrc: I {tp['so_i']} P {tp['so_p']}, TB I {tp['tb_i']:.0f} B,"
            f" P {tp['tb_p']:.0f} B")
        log(f"   P: {tp['camera']}")
        ta = a.lop.trang_thai()
        log(f"   A: khung I {ta['khung_i']}, P {ta['khung_p']}, lệch {ms(ta['lech_hien'], True)} ms")
        log("10. P: DỪNG PHÁT, tạo tệp mẫu mp4, nguồn Tệp video, PHÁT")
        p.nut_dung.invoke()
        os.makedirs("out", exist_ok=True)
        tep = tao_tep_mau(os.path.join("out", "tep_mau.mp4"), 10)
        p.dat_tep(tep)
        log(f"   {p.chon_cam.get()} | {p.chon_mic.get()}")
        yield 0.3
        p.nut_phat.invoke()
        yield 4.4
        tp = p.lop.trang_thai()
        log(f"   P tệp: I {tp['so_i']} P {tp['so_p']}, TB I {tp['tb_i']:.0f} B, P {tp['tb_p']:.0f} B")
        log(f"   P: {tp['camera'][:52]}")
        ta = a.lop.trang_thai()
        log(f"   A: khung I {ta['khung_i']}, P {ta['khung_p']}, lệch {ms(ta['lech_hien'], True)} ms,"
            f" mức tiếng {ta['muc_am']:.1f} dBFS")
        tam = [".gui_tep_0.png", ".gui_tep_1.png"]
        chup(p.root, tam[0])
        chup(a.root, tam[1])
        ra = os.path.join(os.path.dirname(out) or ".", "ket_qua_gui_tep.png")
        chup_anh.convert([tam[0], tam[1], "-background", "#d0d0d0", "-gravity", "north", "+append",
                          "-bordercolor", "#d0d0d0", "-border", "4", ra])
        for f in tam:
            os.remove(f)
        log(f"   Đã ghi {ra} ({os.path.getsize(ra) // 1024} KB)")
    log("11. Server bấm STOP SERVER")
    sv.nut_stop.invoke()
    yield lambda: sv.nut_start["state"] == "normal"
    yield 0.6
    log(f"   P: {p.thanh_tt['text'][:46]}")
    log(f"   A: {a.thanh_tt['text'][:46]}")
    for cs in (p, a, b):
        cs.dong()
    sv.dong()


def chay_kich_ban(root, gen, het_gio=15.0):
    """Bộ điều phối: lấy bước tiếp theo của generator và hẹn giờ bằng root.after (luồng giao diện)."""
    def buoc():
        try:
            v = next(gen)
        except StopIteration:
            return
        if callable(v):
            cho(v, het_gio)
        else:
            root.after(int(v * 1000), buoc)

    def cho(dk, con_lai):
        if dk():
            buoc()
        elif con_lai <= 0:
            log("Kịch bản chờ quá lâu – dừng")
            root.destroy()
        else:
            root.after(50, cho, dk, con_lai - 0.05)
    root.after(0, buoc)


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41652
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else cong_tcp + 1
    out = sys.argv[4] if len(sys.argv) > 4 else "ket_qua_gui.png"
    root = tk.Tk()
    sv = CuaSoServer(root, host, cong_tcp, cong_udp)
    root.geometry("+0+0")
    root.update_idletasks()
    p = CuaSoPhat(tk.Toplevel(root), "P", host, cong_tcp)
    p.root.geometry(f"+{root.winfo_reqwidth() + 20}+0")
    p.root.update_idletasks()
    y = max(root.winfo_reqheight(), p.root.winfo_reqheight()) + 40
    ds = {}
    x = 0
    for ten in "AB":
        ds[ten] = CuaSoXem(tk.Toplevel(root), ten, host, cong_tcp)
        ds[ten].root.geometry(f"+{x}+{y}")
        ds[ten].root.update_idletasks()
        x += ds[ten].root.winfo_reqwidth() + 20
    chay_kich_ban(root, kich_ban(root, sv, p, ds, out))
    root.mainloop()
    log("Kết thúc")


if __name__ == "__main__":
    main()
