"""Bài mở rộng MR6.3 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: cửa sổ "Camera Server" (Tk) và 3 cửa sổ "Camera Client"
(Toplevel) A, B, C. A và B dùng "Camera giả lập"; C dùng "Mẫu thử ffmpeg (testsrc)" – đi đúng đường
camera thật (tiến trình ffmpeg -> ống stdout -> khung RGB 160×120) nhưng nguồn là ảnh mẫu của
ffmpeg vì máy chạy demo không có webcam. Kịch bản bấm nút như người dùng (generator: `yield giây`
= chờ, `yield hàm` = chờ tới khi điều kiện đúng; mainloop vẫn chạy trong lúc chờ), in số liệu fps,
kbit/s, rồi chụp các cửa sổ thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 1400x1100x24" python3 demo_gui.py [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
"""
import os
import sys
import tkinter as tk

import chup_anh
from camera import GIA_LAP, MAU_THU
from khach_gui import CuaSoKhach
from server_gui import CuaSoServer


def log(text):
    print(f"[demo] {text}", flush=True)


def chon_camera(cs, ma):
    for i, (m, _) in enumerate(cs.ds_cam):
        if m == ma:
            cs.chon_cam.current(i)
            return True
    return False


def in_so_lieu(sv, ds):
    for k in sv.phong.anh_chup():
        log(f"    server nhận #{k['ma']} {k['ten']}: camera {k['ly_do']:<12} giới hạn {k['fps_toi_da']:2d} fps"
            f" 1/{k['chia']} | vào {k['fps_vao']:2.0f} fps {k['kbps_vao']:4.0f} kbit/s | lưới gửi đi"
            f" {k['fps_ra']:2.0f} fps {k['kbps_ra']:4.0f} kbit/s, mảnh mất {k['manh_mat']}")
    for t, cs in ds.items():
        l = cs.lop
        kt = getattr(cs, "kich_thuoc_luoi", (0, 0))
        log(f"    client {t}: gửi {l.fps_gui:2.0f} fps {l.kbps_gui:4.0f} kbit/s | nhận lưới {kt[0]}x{kt[1]}"
            f" {l.fps_nhan:2.0f} fps {l.kbps_nhan:4.0f} kbit/s | {cs.chi_bao_cam['text']}")


def chup(cua_so, duong):
    cua_so.update()
    chup_anh.chup(cua_so, duong)


def kich_ban(root, sv, ds, out):
    yield 0.5
    log("1. Server: bấm START SERVER")
    sv.nut_start.invoke()
    log(f"   -> {sv.thanh_tt['text']}")
    log("2. Client A, B, C: bấm Kết nối")
    for cs in ds.values():
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.lop and cs.lop.da_ket_noi for cs in ds.values())
    yield 0.3
    log("3. A, B chọn 'Camera giả lập'; C chọn 'Mẫu thử ffmpeg (testsrc)'; bấm START")
    for t, cs in ds.items():
        if not (t == "C" and chon_camera(cs, MAU_THU)):
            chon_camera(cs, GIA_LAP)
        cs.nut_start.invoke()
    yield 3.0
    log("4. Số liệu khi cả 3 camera bật (mặc định 10 fps, 1/1):")
    in_so_lieu(sv, ds)
    log("5. Server chọn B, bấm 'Tắt camera'; chọn C, đặt FPS gửi tối đa 5, độ phân giải 1/2")
    sv.chon_client(ds["B"].lop.ma)
    sv.nut_tat_cam.invoke()
    sv.chon_client(ds["C"].lop.ma)
    sv.spin_fps.delete(0, "end")
    sv.spin_fps.insert(0, "5")
    sv.ap_fps()
    sv.chon_chia.current(1)
    sv.ap_chia()
    log("   A chọn nhận lưới độ phân giải 1/2, 5 fps")
    ds["A"].chon_chia.current(1)
    ds["A"].ap_chia()
    ds["A"].spin_fps.delete(0, "end")
    ds["A"].spin_fps.insert(0, "5")
    ds["A"].ap_fps()
    yield 0.5
    log("6. B bấm CANCEL rồi START (xin bật lại khi server đang tắt camera của B)")
    ds["B"].nut_cancel.invoke()
    yield 0.3
    ds["B"].nut_start.invoke()
    yield 2.7
    log("7. Số liệu sau các lệnh:")
    in_so_lieu(sv, ds)
    for t, cs in ds.items():
        log(f"    {t}: {cs.nhan_cam_server['text']} | {cs.thanh_tt['text']}")
    log("8. Chụp các cửa sổ")
    tam = []
    for i, w in enumerate([root] + [cs.root for cs in ds.values()]):
        tam.append(f".gui_{i}.png")
        chup(w, tam[-1])
    chup_anh.convert([tam[0], "(", *tam[1:], "-bordercolor", "#d0d0d0", "-border", "4",
                      "+append", ")", "-background", "#d0d0d0", "-gravity", "center", "-append",
                      "-bordercolor", "#d0d0d0", "-border", "4", out])
    for p in tam:
        os.remove(p)
    log(f"   Đã ghi {out} ({os.path.getsize(out) // 1024} KB)")
    log("9. Server bấm STOP SERVER")
    sv.nut_stop.invoke()
    yield lambda: sv.nut_start["state"] == "normal"
    yield 0.5
    for t, cs in ds.items():
        log(f"   {t}: {cs.thanh_tt['text']}")
    for cs in ds.values():
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
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41635
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else 41636
    out = sys.argv[4] if len(sys.argv) > 4 else "ket_qua_gui.png"
    root = tk.Tk()
    sv = CuaSoServer(root, host, cong_tcp, cong_udp, in_nhat_ky=True)
    root.geometry("+0+0")
    root.update_idletasks()
    y = root.winfo_reqheight() + 30
    ds = {}
    for i, ten in enumerate("ABC"):
        ds[ten] = CuaSoKhach(tk.Toplevel(root), ten, host, cong_tcp)
        ds[ten].root.geometry(f"+{i * 700}+{y}")
    chay_kich_ban(root, kich_ban(root, sv, ds, out))
    root.mainloop()
    log("Kết thúc")


if __name__ == "__main__":
    main()
