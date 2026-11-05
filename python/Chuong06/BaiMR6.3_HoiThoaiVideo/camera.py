"""Bài mở rộng MR6.3 – Nguồn camera cho bản giao diện: camera thật qua ffmpeg hoặc camera giả lập.

Không dùng thư viện ngoài cho Python: chương trình chạy tiến trình `ffmpeg` (công cụ dòng lệnh) và
đọc khung hình thô từ ống (pipe) stdout của nó:
    ffmpeg -f <dshow|v4l2|avfoundation> -i <camera> -vf scale=160:120,fps=<fps>
           -pix_fmt rgb24 -f rawvideo pipe:1
Mỗi khung đúng 160 × 120 × 3 = 57 600 byte RGB, đưa thẳng vào đường gửi sẵn có (thu nhỏ, nén zlib,
chia mảnh). Một luồng nền đọc ống liên tục và chỉ giữ khung MỚI NHẤT – video trực tiếp không cần
khung cũ; vòng gửi lấy khung mới nhất theo tốc độ server cho phép.
Liệt kê camera: Windows `ffmpeg -list_devices true -f dshow -i dummy` (đọc stderr), Linux /dev/video*,
macOS `ffmpeg -f avfoundation -list_devices true -i ""`.
Không có ffmpeg hoặc không có camera -> chỉ còn "Camera giả lập" (CameraTongHop của chung.py).
"""
import glob
import os
import re
import shutil
import subprocess
import sys
import threading

from chung import CAO, RONG, CameraTongHop

KICH_THUOC = RONG * CAO * 3
GIA_LAP, MAU_THU = "gia_lap", "testsrc"           # hai "camera" không phải thiết bị thật
AN_CUA_SO = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}


def co_ffmpeg():
    return shutil.which("ffmpeg") is not None


def liet_ke_camera():
    """Danh sách (mã thiết bị, tên hiển thị) camera thật tìm được qua ffmpeg."""
    if not co_ffmpeg():
        return []
    try:
        if sys.platform.startswith("win"):
            r = subprocess.run(["ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"],
                               capture_output=True, text=True, encoding="utf-8", errors="replace",
                               timeout=10, **AN_CUA_SO)
            ds, trong_muc_video = [], False
            for dong in r.stderr.splitlines():
                if "DirectShow video devices" in dong:          # định dạng ffmpeg cũ (theo mục)
                    trong_muc_video = True
                elif "DirectShow audio devices" in dong:
                    trong_muc_video = False
                m = re.search(r'"([^"]+)"\s*\((video|none)\)', dong)   # định dạng mới: "Tên" (video)
                if m and m.group(2) == "video":
                    ds.append((m.group(1), m.group(1)))
                elif trong_muc_video and "Alternative name" not in dong:
                    m = re.search(r'"([^"]+)"', dong)
                    if m:
                        ds.append((m.group(1), m.group(1)))
            return list(dict.fromkeys(ds))
        if sys.platform == "darwin":
            r = subprocess.run(["ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""],
                               capture_output=True, text=True, errors="replace", timeout=10)
            ds, trong_muc_video = [], False
            for dong in r.stderr.splitlines():
                if "video devices" in dong:
                    trong_muc_video = True
                elif "audio devices" in dong:
                    trong_muc_video = False
                elif trong_muc_video:
                    m = re.search(r"\[(\d+)\]\s+(.+)$", dong)
                    if m:
                        ds.append((m.group(1), m.group(2).strip()))
            return ds
        return [(p, p) for p in sorted(glob.glob("/dev/video*"))]           # Linux
    except (OSError, subprocess.SubprocessError):
        return []


def lenh_ffmpeg(ma, fps):
    """Dòng lệnh ffmpeg thu camera `ma` (hoặc ảnh mẫu thử testsrc) ra khung RGB thô 160×120."""
    if ma == MAU_THU:
        vao = ["-re", "-f", "lavfi", "-i", f"testsrc=size={RONG}x{CAO}:rate={fps}"]
    elif sys.platform.startswith("win"):
        vao = ["-f", "dshow", "-i", f"video={ma}"]
    elif sys.platform == "darwin":
        vao = ["-f", "avfoundation", "-framerate", "30", "-i", ma]
    else:
        vao = ["-f", "v4l2", "-i", ma]
    return (["ffmpeg", "-hide_banner", "-loglevel", "error"] + vao
            + ["-vf", f"scale={RONG}:{CAO},fps={fps}", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"])


class CameraGiaLap:
    """Bọc CameraTongHop (khung tổng hợp của bản dòng lệnh) theo cùng giao diện với CameraFfmpeg."""
    that = False

    def __init__(self, ten, ma):
        self.nguon, self.mo_ta, self.loi = CameraTongHop(ten, ma), "GIẢ LẬP – khung tổng hợp", ""

    def lay_khung(self, so):
        return self.nguon.lay_khung(so)

    def dong(self):
        pass


class CameraFfmpeg:
    """Camera qua tiến trình ffmpeg; luồng nền đọc từng khung 57 600 byte, giữ khung mới nhất."""

    def __init__(self, ma, ten, fps=15):
        self.that = ma != MAU_THU
        self.mo_ta = f"THẬT – {ten} (ffmpeg)" if self.that else "MẪU THỬ ffmpeg testsrc (không phải camera)"
        self.khoa = threading.Lock()
        self.moi_nhat, self.so_khung, self.loi = None, 0, ""
        self.tien_trinh = subprocess.Popen(lenh_ffmpeg(ma, fps), stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, **AN_CUA_SO)
        threading.Thread(target=self._doc, daemon=True).start()

    def _doc(self):
        ong = self.tien_trinh.stdout
        while True:
            khung = ong.read(KICH_THUOC)           # đọc đủ một khung (read chặn tới khi đủ hoặc hết)
            if len(khung) < KICH_THUOC:
                break                              # ffmpeg kết thúc / lỗi mở camera
            with self.khoa:
                self.moi_nhat = khung
                self.so_khung += 1
        err = self.tien_trinh.stderr.read().decode("utf-8", "replace").strip()
        self.loi = err.splitlines()[-1] if err else "ffmpeg đã dừng"

    def lay_khung(self, so):
        with self.khoa:
            k = self.moi_nhat
        return bytearray(k) if k is not None else None   # None: chưa có khung nào

    def dong(self):
        try:
            self.tien_trinh.kill()
            self.tien_trinh.wait(timeout=2)
        except (OSError, subprocess.SubprocessError):
            pass


def mo_camera(ma_thiet_bi, ten_hien_thi, ten_client, ma_client):
    if ma_thiet_bi == GIA_LAP or not co_ffmpeg():
        return CameraGiaLap(ten_client, ma_client)
    return CameraFfmpeg(ma_thiet_bi, ten_hien_thi)
