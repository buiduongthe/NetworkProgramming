"""Bài mở rộng MR6.2 – Kịch bản minh họa tự động cho bản giao diện (chạy dưới xvfb-run trên Linux).

Một tiến trình, một luồng giao diện: cửa sổ server (Tk) và 3 cửa sổ client (Toplevel) A, B, C với
micro giả lập 440, 620, 880 Hz; micro giả lập của server 330 Hz. Kịch bản bấm nút như người dùng
(widget.invoke(), thanh trượt .set()), viết dạng generator: mỗi `yield số_giây` là "chờ chừng ấy
giây", `yield hàm_điều_kiện` là "chờ tới khi điều kiện đúng" – trong lúc chờ, mainloop vẫn chạy
nên cửa sổ vẫn vẽ lại, các thanh đo mức vẫn nhảy. Ở hai thời điểm, kịch bản đo mức từng âm
(Goertzel, chung.muc_am_dbfs) trong 1 s âm thanh gần nhất mà mỗi bên nghe được để chứng minh
mix-minus, khóa mic, độ lợi, âm lượng nghe. Cuối cùng chụp các cửa sổ (ImageMagick `import`) và
ghép thành ket_qua_gui.png.

Chạy:  xvfb-run -a -s "-screen 0 1600x1250x24" python3 demo_gui.py [host] [cổng_tcp] [cổng_udp] [tệp_ảnh]
"""
import math
import os
import sys
import tkinter as tk

import chup_anh
from chung import muc_am_dbfs
from khach_gui import CuaSoKhach
from server_gui import TAN_SO_SERVER, CuaSoServer

TAN_SO = {"A": 440, "B": 620, "C": 880}


def log(text):
    print(f"[demo] {text}", flush=True)


def chu_muc(mau, f):
    db = muc_am_dbfs(mau, f) if len(mau) >= 1600 else -math.inf
    return " vắng" if db < -60 else f"{db:5.1f}"


def do_muc(sv, ds):
    """In mức (dBFS) của từng âm trong 1 s gần nhất mà mỗi client và server nghe được."""
    ten_am = [(TAN_SO_SERVER, "server")] + [(f, t) for t, f in TAN_SO.items()]
    log("    người nghe  " + "".join(f"{t}({f})".rjust(12) for f, t in ten_am) + "   tổng")
    for t, cs in ds.items():
        with cs.lop.khoa:
            mau = list(cs.lop.nhan_gan_day)
        log(f"    client {t}    " + "".join(chu_muc(mau, f).rjust(12) for f, _ in ten_am)
            + f"  {cs.lop.muc_nhan:5.1f}")
    anh = sv.phong.anh_chup()
    log("    server      " + "".join(chu_muc(anh['nghe_gan_day'], f).rjust(12) for f, _ in ten_am)
        + f"  {anh['muc_nghe']:5.1f}")
    for k in anh["khach"]:
        log(f"    server thấy #{k['ma']} {k['ten']}: mic {k['ly_do']:<12} mức vào {k['muc']:6.1f} dBFS,"
            f" độ lợi {k['do_loi']:.2f}, nghe {k['am_luong_nghe']:.2f}, mất {k['mat']}")


def chup(cua_so, duong):
    cua_so.update()
    chup_anh.chup(cua_so, duong)


def kich_ban(root, sv, ds, out):
    yield 0.5
    log("1. Server: chọn micro 'Giả lập', Mic Volume 50 %, bật 'Phát ra loa server', bấm Start Server")
    sv.chon_mic.current(len(sv.ds_mic) - 1)
    sv.thanh_amv.set(50)
    sv.bien_nghe.set(True)
    sv.doi_nghe()
    sv.nut_start.invoke()
    sv.nut_mic_on.invoke()
    log(f"   -> '{sv.nhan_tt['text']}'; {sv.chi_bao_mic['text']}; {sv.chi_bao_loa['text']}")
    log("2. Client A, B, C: bấm Connect")
    for cs in ds.values():
        cs.nut_ket_noi.invoke()
    yield lambda: all(cs.lop and cs.lop.da_ket_noi for cs in ds.values())
    yield 0.3
    for t, cs in ds.items():
        log(f"   {t}: '{cs.nhan_tt['text']}', {cs.nhan_server_vol['text']}")
    log("3. Mỗi client: chọn micro 'Giả lập', bấm Start Audio, Start Mic")
    for cs in ds.values():
        cs.chon_mic.current(len(cs.ds_mic) - 1)
        cs.nut_audio_on.invoke()
        cs.nut_mic_on.invoke()
    yield 2.5
    log(f"   {sv.nhan_so['text']}; {ds['A'].chi_bao_mic['text']}; {ds['A'].chi_bao_loa['text']}")
    log("4. Đo lần 1 (mọi mic mở) – mức từng âm, dBFS:")
    do_muc(sv, ds)
    yield 0.2
    log("5. Server bấm 'Khóa' ở hàng B, kéo độ lợi C lên 200 %; A kéo âm lượng nghe xuống 50 %")
    ma_b = ds["B"].lop.ma
    ma_c = ds["C"].lop.ma
    sv.hang[ma_b]["khoa"].invoke()
    sv.hang[ma_c]["loi"].set(200)
    ds["A"].thanh_aln.set(50)
    yield 0.6
    log("6. B bấm Stop Mic rồi Start Mic (xin bật lại khi đang bị khóa)")
    ds["B"].nut_mic_off.invoke()
    yield 0.3
    ds["B"].nut_mic_on.invoke()
    yield 0.5
    for t, cs in ds.items():
        log(f"   {t}: '{cs.nhan_tt['text']}', {cs.nhan_server_vol['text']}, {cs.nhan_mic_server['text']}")
    yield 2.0
    log("7. Đo lần 2 (B bị khóa, độ lợi C = 2, A nghe 50 %) – mức từng âm, dBFS:")
    do_muc(sv, ds)
    yield 0.3
    log("8. Chụp các cửa sổ")
    tam = []
    for i, w in enumerate([root] + [cs.root for cs in ds.values()]):
        tam.append(f".gui_{i}.png")
        chup(w, tam[-1])
    chup_anh.convert([tam[0], "(", *tam[1:], "-bordercolor", "#e8e8e8", "-border", "4",
                      "+append", ")", "-background", "#e8e8e8", "-gravity", "center", "-append",
                      "-bordercolor", "#e8e8e8", "-border", "4", out])
    for p in tam:
        os.remove(p)
    log(f"   Đã ghi {out} ({os.path.getsize(out) // 1024} KB)")
    log("9. Server bấm Stop Server")
    sv.nut_stop.invoke()
    yield lambda: sv.nut_start["state"] == "normal"
    yield 0.5
    for t, cs in ds.items():
        log(f"   {t}: '{cs.nhan_tt['text']}'")
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
    cong_tcp = int(sys.argv[2]) if len(sys.argv) > 2 else 41625
    cong_udp = int(sys.argv[3]) if len(sys.argv) > 3 else 41626
    out = sys.argv[4] if len(sys.argv) > 4 else "ket_qua_gui.png"
    root = tk.Tk()
    sv = CuaSoServer(root, host, cong_tcp, cong_udp, in_nhat_ky=True)
    root.geometry("1180x600+0+0")
    ds = {}
    for i, (ten, f) in enumerate(TAN_SO.items()):
        ds[ten] = CuaSoKhach(tk.Toplevel(root), ten, host, cong_tcp, f)
        ds[ten].root.geometry(f"+{i * 520}+640")
    chay_kich_ban(root, kich_ban(root, sv, ds, out))
    root.mainloop()
    log("Kết thúc")


if __name__ == "__main__":
    main()
