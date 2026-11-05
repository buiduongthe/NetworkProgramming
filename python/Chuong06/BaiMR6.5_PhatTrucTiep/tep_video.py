"""Bài mở rộng MR6.5 – Nguồn "tệp video" cho máy phát: phát lại hình + tiếng của một tệp (mp4, mkv,
avi, webm…) như một buổi phát trực tiếp (giống OBS "Media Source" hay `ffmpeg -re … rtmp://…`).

Hai tiến trình ffmpeg cùng đọc tệp với `-re` (đọc theo tốc độ thật, không nhanh hơn thời gian phát)
và `-stream_loop -1` (hết tệp thì phát lại từ đầu):
    hình:  ffmpeg -re -stream_loop -1 -i <tệp> -an -vf scale=160:120:force_original_aspect_ratio=decrease,
                  pad=160:120:(ow-iw)/2:(oh-ih)/2,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
    tiếng: ffmpeg -re -stream_loop -1 -i <tệp> -vn -ac 1 -ar 16000 -f s16le pipe:1
Bộ lọc `fps=<fps>` cho đúng <fps> khung mỗi giây THỜI GIAN CỦA TỆP, nên khung thứ i có thời điểm trong
tệp i × 1000 / fps ms; tiếng có 16 000 mẫu mỗi giây của tệp.

Đồng bộ tiếng-hình ở phía phát. Hai tiến trình khởi động lệch nhau vài chục – vài trăm ms; nếu cứ gửi
"khung mới nhất" thì hình và tiếng của TỆP lệch nhau ngay từ máy phát, người xem đồng bộ theo nhãn
thời gian cũng không sửa được. Cách làm ở đây (giống bộ ghép kênh – muxer – của bộ mã hóa thật):
  - Tiếng là đồng hồ chủ: vòng 20 ms của máy phát lấy lần lượt 320 mẫu của tệp. Chưa đủ mẫu (ffmpeg
    chưa chạy kịp) -> gửi im lặng và `lech_ms` += 20; dồn quá 300 ms -> bỏ bớt 20 ms và `lech_ms` −= 20.
    Vậy ở thời điểm phương tiện T máy phát đang gửi tiếng của giây T − lech_ms trong tệp.
  - Hình: luồng đọc giữ ~3 s khung gần nhất kèm thời điểm trong tệp; khi máy phát chụp khung ở thời
    điểm phương tiện T, chọn khung có thời điểm trong tệp ≤ T − lech_ms gần nhất. Nhờ vậy nhãn thời gian
    gửi đi khớp với nội dung của tệp, và cơ chế đồng bộ ở người xem (MR6.5) hoạt động như với camera.
Tệp không có tiếng (hoặc chọn micro khác) -> hình lấy khung mới nhất.
"""
import array
import collections
import os
import subprocess
import sys
import threading

from camera import AN_CUA_SO, KICH_THUOC, co_ffmpeg
from chung import CAO, FPS_MAC_DINH, KHUNG_MS, MAU_MOI_KHUNG, RONG, TAN_SO_MAU

TEP = "tep"                                   # mã nguồn "tệp video" trong danh sách camera/micro
BYTE_MOI_KHUNG_AM = MAU_MOI_KHUNG * 2         # 320 mẫu × 2 byte (s16le)
DON_TOI_DA_MS = 300                           # tiếng của tệp dồn quá mức này thì bỏ bớt
GIU_HINH_MS = 3000                            # giữ khung hình của 3 s gần nhất


def lenh_hinh(duong_dan, fps):
    loc = (f"scale={RONG}:{CAO}:force_original_aspect_ratio=decrease,"
           f"pad={RONG}:{CAO}:(ow-iw)/2:(oh-ih)/2,fps={fps}")
    return ["ffmpeg", "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duong_dan,
            "-an", "-vf", loc, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"]


def lenh_tieng(duong_dan):
    return ["ffmpeg", "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duong_dan,
            "-vn", "-ac", "1", "-ar", str(TAN_SO_MAU), "-f", "s16le", "pipe:1"]


class TepVideo:
    """Một tệp đang phát: hai tiến trình ffmpeg + bộ đệm tiếng + vòng khung hình có thời điểm trong tệp."""

    def __init__(self, duong_dan, fps=FPS_MAC_DINH, co_tieng=True):
        if not co_ffmpeg():
            raise OSError("không có ffmpeg trong PATH")
        if not os.path.isfile(duong_dan):
            raise OSError(f"không thấy tệp {duong_dan}")
        self.duong_dan, self.fps = duong_dan, fps
        self.ten = os.path.basename(duong_dan)
        self.khoa = threading.Lock()
        self.hinh = collections.deque()               # (ms trong tệp, khung rgb)
        self.tieng = bytearray()
        self.so_khung_doc = 0
        self.lech_ms = 0                              # thời điểm phương tiện − thời điểm trong tệp (tiếng)
        self.dung_tieng = False                       # micro của máy phát đang là tiếng của tệp này
        self.loi_hinh = self.loi_tieng = ""
        self.het_tieng = not co_tieng
        self.ffmpeg_hinh = subprocess.Popen(lenh_hinh(duong_dan, fps), stdout=subprocess.PIPE,
                                            stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, **AN_CUA_SO)
        threading.Thread(target=self._doc_hinh, daemon=True).start()
        self.ffmpeg_tieng = None
        if co_tieng:
            self.ffmpeg_tieng = subprocess.Popen(lenh_tieng(duong_dan), stdout=subprocess.PIPE,
                                                 stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, **AN_CUA_SO)
            threading.Thread(target=self._doc_tieng, daemon=True).start()

    @staticmethod
    def _loi(tien_trinh, mac_dinh):
        err = tien_trinh.stderr.read().decode("utf-8", "replace").strip()
        return err.splitlines()[-1] if err else mac_dinh

    def _doc_hinh(self):
        ong = self.ffmpeg_hinh.stdout
        while True:
            khung = ong.read(KICH_THUOC)
            if len(khung) < KICH_THUOC:
                break
            with self.khoa:
                ms = self.so_khung_doc * 1000 // self.fps
                self.so_khung_doc += 1
                self.hinh.append((ms, khung))
                while self.hinh and self.hinh[0][0] < ms - GIU_HINH_MS:
                    self.hinh.popleft()
        self.loi_hinh = self._loi(self.ffmpeg_hinh, "ffmpeg (hình) đã dừng")

    def _doc_tieng(self):
        ong = self.ffmpeg_tieng.stdout
        while True:
            b = ong.read(BYTE_MOI_KHUNG_AM)
            if not b:
                break
            with self.khoa:
                self.tieng += b
        loi = self._loi(self.ffmpeg_tieng, "tệp không có tiếng")
        with self.khoa:
            self.het_tieng = True
            self.loi_tieng = loi

    # ------------------------------------------------------------ tiếng (vòng 20 ms của máy phát)
    def lay_tieng(self):
        """320 mẫu tiếp theo của tệp; chưa đủ -> im lặng (lech_ms += 20)."""
        with self.khoa:
            don = BYTE_MOI_KHUNG_AM * DON_TOI_DA_MS // KHUNG_MS
            while len(self.tieng) > don:              # dồn quá nhiều: bỏ 20 ms cũ nhất
                del self.tieng[:BYTE_MOI_KHUNG_AM]
                self.lech_ms -= KHUNG_MS
            if len(self.tieng) < BYTE_MOI_KHUNG_AM:
                self.lech_ms += KHUNG_MS
                return [0] * MAU_MOI_KHUNG
            b = bytes(self.tieng[:BYTE_MOI_KHUNG_AM])
            del self.tieng[:BYTE_MOI_KHUNG_AM]
        a = array.array("h")
        a.frombytes(b)
        if sys.byteorder == "big":                    # s16le: byte thấp trước
            a.byteswap()
        return a.tolist()

    # ------------------------------------------------------------ hình (vòng fps của máy phát)
    def lay_hinh(self, ts_ms):
        with self.khoa:
            if not self.hinh:
                return None
            if not self.dung_tieng or self.het_tieng:
                return bytearray(self.hinh[-1][1])    # không có tiếng của tệp: khung mới nhất
            can = ts_ms - self.lech_ms               # thời điểm trong tệp ứng với tiếng đang gửi
            chon = None
            for ms, khung in self.hinh:
                if ms <= can:
                    chon = khung
                else:
                    break
            if chon is None:
                chon = self.hinh[0][1]                # hình chưa tới kịp: dùng khung sớm nhất đang có
            return bytearray(chon)

    def dong(self):
        for p in (self.ffmpeg_hinh, self.ffmpeg_tieng):
            if p is None:
                continue
            try:
                p.kill()
                p.wait(timeout=2)
            except (OSError, subprocess.SubprocessError):
                pass


class CameraTep:
    """Phần hình của TepVideo theo giao diện camera của máy phát."""
    that = False

    def __init__(self, tep):
        self.tep = tep
        self.mo_ta = f"TỆP VIDEO – {tep.ten}"

    @property
    def loi(self):
        return self.tep.loi_hinh

    def dat_fps(self, fps):
        pass                                          # ffmpeg chạy theo fps lúc mở

    def lay_khung(self, so, ts_ms):
        return self.tep.lay_hinh(ts_ms)

    def dong(self):
        self.tep.dong()


class MicTep:
    """Phần tiếng của TepVideo theo giao diện micro của máy phát."""
    that = False

    def __init__(self, tep):
        self.tep = tep
        tep.dung_tieng = True
        self.mo_ta = f"TIẾNG CỦA TỆP – {tep.ten}"

    def lay_khung(self):
        return self.tep.lay_tieng()

    def dong(self):
        self.tep.dong()


def mo_tep(duong_dan, fps=FPS_MAC_DINH, lay_tieng=True):
    """Mở tệp video; trả về (CameraTep, MicTep hoặc None). Lỗi -> OSError."""
    tep = TepVideo(duong_dan, fps, co_tieng=lay_tieng)
    return CameraTep(tep), (MicTep(tep) if lay_tieng else None)


def tao_tep_mau(duong_dan, giay=6):
    """Tạo tệp mp4 mẫu bằng ffmpeg (để thử khi chưa có tệp): ảnh testsrc2 chớp trắng 0,1 s đầu mỗi
    giây, tiếng 440 Hz nhỏ + bíp 1000 Hz đúng lúc chớp – nhìn/nghe là thấy hình và tiếng có khớp."""
    hinh = (f"testsrc2=size=320x240:rate=25:duration={giay},"
            "drawbox=x=0:y=0:w=iw:h=ih:color=white@0.85:t=fill:enable='lt(mod(t\\,1)\\,0.1)'")
    tieng = (f"aevalsrc='0.08*sin(2*PI*440*t)+0.4*sin(2*PI*1000*t)*lt(mod(t\\,1)\\,0.1)':"
             f"s={TAN_SO_MAU}:d={giay}")
    lenh = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", hinh,
            "-f", "lavfi", "-i", tieng, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "25",
            "-c:a", "aac", "-shortest", duong_dan]
    r = subprocess.run(lenh, capture_output=True, text=True, errors="replace", **AN_CUA_SO)
    if r.returncode != 0:                          # ffmpeg thiếu libx264: dùng mpeg4
        lenh[lenh.index("libx264")] = "mpeg4"
        r = subprocess.run(lenh, capture_output=True, text=True, errors="replace", **AN_CUA_SO)
    if r.returncode != 0:
        raise OSError(r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "ffmpeg lỗi")
    return duong_dan
