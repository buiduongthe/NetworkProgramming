"""Chụp cửa sổ Tk và ghép ảnh cho kịch bản minh họa giao diện (demo) – chạy được cả Linux và Windows.

Linux (kể cả Xvfb): dùng ImageMagick như trước – `import -window <id>` chụp một cửa sổ (cả khung viền),
`convert ... +append/-append` ghép ảnh.
Windows: không có ImageMagick (và `convert.exe` của Windows là công cụ đổi hệ thống tệp – không được gọi), nên:
  - chụp: đưa cửa sổ lên trên cùng, lấy khung cửa sổ theo tọa độ vật lý (DWM) rồi chụp vùng màn hình đó bằng
    Pillow (ImageGrab);
  - ghép: làm lại đúng phần lệnh convert mà các kịch bản dùng (tệp ảnh, ( ), -background, -bordercolor,
    -border, -gravity north/west/center, +append, -append) bằng Pillow.
Cần Pillow trên Windows:  py -3 -m pip install --user pillow
"""
import os
import subprocess
import sys
import time

WINDOWS = sys.platform == "win32"


def chup(cua_so, duong):
    """Chụp một cửa sổ Tk (cả khung viền) vào tệp PNG."""
    cua_so.update()
    if not WINDOWS:
        subprocess.run(["import", "-window", cua_so.wm_frame(), duong], check=True)
        return
    import ctypes
    from ctypes import wintypes
    from PIL import ImageGrab
    vi_tri = cua_so.winfo_x(), cua_so.winfo_y()
    cua_so.geometry("+0+0")                       # cửa sổ có thể nằm một phần ngoài màn hình (bố cục cho Xvfb)
    cua_so.deiconify()
    cua_so.lift()
    cua_so.attributes("-topmost", True)
    cua_so.update()
    time.sleep(0.4)                               # chờ Windows vẽ xong sau khi đưa lên trên
    cua_so.update()
    hwnd = int(cua_so.wm_frame(), 16)
    r = wintypes.RECT()
    DWMWA_EXTENDED_FRAME_BOUNDS = 9               # khung thật của cửa sổ, tọa độ vật lý (không phụ thuộc DPI)
    ctypes.windll.dwmapi.DwmGetWindowAttribute(wintypes.HWND(hwnd), DWMWA_EXTENDED_FRAME_BOUNDS,
                                               ctypes.byref(r), ctypes.sizeof(r))
    ImageGrab.grab(bbox=(r.left, r.top, r.right, r.bottom), all_screens=True).save(duong)
    cua_so.attributes("-topmost", False)
    cua_so.geometry(f"+{vi_tri[0]}+{vi_tri[1]}")
    cua_so.update()


def convert(doi_so):
    """Như `convert <doi_so...>` của ImageMagick (phần con mà các kịch bản demo dùng); tệp cuối là tệp ra."""
    if not WINDOWS:
        subprocess.run(["convert", *doi_so], check=True)
        return
    from PIL import Image, ImageOps
    *lenh, ra = doi_so
    nen, vien_mau, can = "#ffffff", "#dfdfdf", "northwest"
    ngan = [[]]                                   # ngăn xếp danh sách ảnh; "(" mở danh sách mới

    def noi(ds, ngang):
        if not ds:
            return []
        rong = sum(a.width for a in ds) if ngang else max(a.width for a in ds)
        cao = max(a.height for a in ds) if ngang else sum(a.height for a in ds)
        kq = Image.new("RGB", (rong, cao), nen)
        vt = 0
        for a in ds:
            if ngang:                             # căn dọc: north* trên, south* dưới, west/center/east giữa
                y = 0 if can.startswith("north") else cao - a.height if can.startswith("south") else (cao - a.height) // 2
                kq.paste(a, (vt, y)); vt += a.width
            else:                                 # căn ngang: *west trái, *east phải, north/center/south giữa
                x = 0 if can.endswith("west") else rong - a.width if can.endswith("east") else (rong - a.width) // 2
                kq.paste(a, (x, vt)); vt += a.height
        return [kq]

    i = 0
    while i < len(lenh):
        t = lenh[i]
        if t == "(":
            ngan.append([])
        elif t == ")":
            ds = ngan.pop(); ngan[-1].extend(ds)
        elif t == "-background":
            nen = lenh[i + 1]; i += 1
        elif t == "-bordercolor":
            vien_mau = lenh[i + 1]; i += 1
        elif t == "-gravity":
            can = lenh[i + 1].lower(); i += 1
        elif t == "-border":
            n = int(lenh[i + 1]); i += 1
            ngan[-1] = [ImageOps.expand(a, border=n, fill=vien_mau) for a in ngan[-1]]
        elif t in ("+append", "-append"):
            ngan[-1] = noi(ngan[-1], t == "+append")
        else:
            ngan[-1].append(Image.open(t).convert("RGB"))
        i += 1
    ds = [a for nhom in ngan for a in nhom]
    (ds[0] if len(ds) == 1 else noi(ds, False)[0]).save(ra)


def xoa(*tep):
    """Như `rm -f tệp...`."""
    for t in tep:
        try:
            os.remove(t)
        except FileNotFoundError:
            pass
