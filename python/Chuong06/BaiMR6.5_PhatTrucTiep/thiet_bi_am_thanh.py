"""Bài mở rộng MR6.5 – Thiết bị âm thanh: micro/loa thật hoặc nguồn giả lập. Chép từ MR6.2, sửa:
micro giả lập là NguonAmClapper (440 Hz + bíp 1000 Hz mỗi giây tròn).

- Thiết bị thật dùng thư viện ngoài `sounddevice` (gói pip, bọc thư viện C PortAudio). Nếu không nhập
  được (chưa cài, hoặc máy thiếu PortAudio) thì chỉ còn nguồn giả lập; phát ra loa được bỏ qua nhưng
  chương trình vẫn đo mức âm như bình thường.
- PortAudio gọi hàm callback trên LUỒNG RIÊNG của nó mỗi khi thu đủ / cần phát một khối 320 mẫu.
  Callback không được chờ lâu, nên chỉ đẩy/lấy khung qua queue.Queue; vòng lặp 20 ms của chương
  trình lấy/đẩy khung ở đầu kia của hàng đợi (mô hình nhà sản xuất – người tiêu dùng).
- Mọi nguồn âm (thật hay giả lập) có cùng giao diện: lay_khung() -> list 320 mẫu, dong(); thuộc tính
  `that` (True = thiết bị thật) và `mo_ta` để cửa sổ hiện chỉ báo rõ ràng.
"""
import array
import queue

from chung import MAU_MOI_KHUNG, TAN_SO_MAU, NguonAmClapper

GIA_LAP = None                                # "chỉ số thiết bị" của nguồn giả lập


def nap_sounddevice():
    """Trả về (mô-đun sounddevice hoặc None, lý do không dùng được)."""
    try:
        import sounddevice as sd             # thư viện ngoài, tùy chọn
        return sd, ""
    except Exception as e:                   # ImportError (chưa cài) hoặc OSError (thiếu PortAudio)
        return None, str(e) or type(e).__name__


def liet_ke(sd, thu=True):
    """Danh sách (chỉ số, tên) thiết bị thu (thu=True) hoặc phát của host API mặc định.

    Trên Windows mỗi thiết bị xuất hiện nhiều lần (MME, DirectSound, WASAPI…); chỉ lấy host API
    mặc định để danh sách gọn."""
    if sd is None:
        return []
    try:
        try:
            api = sd.default.hostapi
        except Exception:
            api = None
        kenh = "max_input_channels" if thu else "max_output_channels"
        return [(i, d["name"]) for i, d in enumerate(sd.query_devices())
                if d[kenh] > 0 and (api is None or api < 0 or d["hostapi"] == api)]
    except Exception:                         # máy không có thiết bị âm thanh nào
        return []


def chuan_hoa(mau):
    """Đảm bảo đúng 320 mẫu (thiết bị có thể trả khối ngắn hơn khi vừa mở/đóng)."""
    if len(mau) >= MAU_MOI_KHUNG:
        return list(mau[:MAU_MOI_KHUNG])
    return list(mau) + [0] * (MAU_MOI_KHUNG - len(mau))


class MicGiaLap(NguonAmClapper):
    """Micro giả lập: 440 Hz biên độ thấp + bíp 1000 Hz 100 ms ở mỗi giây tròn."""
    that = False

    def __init__(self):
        super().__init__()
        self.mo_ta = "GIẢ LẬP – 440 Hz + bíp 1000 Hz mỗi giây"

    def dong(self):
        pass


class MicThat:
    """Micro thật: PortAudio gọi _thu() mỗi 20 ms (320 mẫu, 16 kHz, mono, 16 bit) trên luồng của nó."""
    that = True

    def __init__(self, sd, chi_so, ten):
        self.mo_ta = f"THẬT – {ten}"
        self.hang = queue.Queue(maxsize=10)          # tối đa 200 ms chờ; đầy thì bỏ khung mới
        self.luong = sd.RawInputStream(samplerate=TAN_SO_MAU, channels=1, dtype="int16",
                                       blocksize=MAU_MOI_KHUNG, device=chi_so, callback=self._thu)
        self.luong.start()

    def _thu(self, indata, frames, t, status):        # chạy trên luồng PortAudio
        try:
            self.hang.put_nowait(bytes(indata))
        except queue.Full:
            pass

    def lay_khung(self):                              # gọi từ vòng 20 ms
        try:
            b = self.hang.get_nowait()
        except queue.Empty:
            return [0] * MAU_MOI_KHUNG                # chưa có dữ liệu: coi như im lặng
        a = array.array("h")
        a.frombytes(b)
        return chuan_hoa(a)

    def dong(self):
        try:
            self.luong.stop()
            self.luong.close()
        except Exception:
            pass


class LoaThat:
    """Loa thật (thiết bị phát mặc định): vòng 20 ms đẩy khung vào hàng đợi, PortAudio lấy ra phát."""

    def __init__(self, sd, chi_so=None):
        self.hang = queue.Queue(maxsize=10)
        self.luong = sd.RawOutputStream(samplerate=TAN_SO_MAU, channels=1, dtype="int16",
                                        blocksize=MAU_MOI_KHUNG, device=chi_so, callback=self._phat)
        self.luong.start()

    def _phat(self, outdata, frames, t, status):      # chạy trên luồng PortAudio
        try:
            b = self.hang.get_nowait()
        except queue.Empty:
            b = b""
        n = len(outdata)
        outdata[:] = (b + bytes(n))[:n]               # thiếu dữ liệu: phát im lặng

    def phat(self, mau):
        try:
            self.hang.put_nowait(array.array("h", mau).tobytes())
        except queue.Full:
            pass

    def dong(self):
        try:
            self.luong.stop()
            self.luong.close()
        except Exception:
            pass


def mo_mic(sd, chi_so, ten=""):
    """Mở micro đã chọn; chi_so = GIA_LAP hoặc lỗi mở thiết bị thật -> nguồn giả lập.
    Trả về (nguồn, thông báo lỗi hoặc "")."""
    if chi_so is not GIA_LAP and sd is not None:
        try:
            return MicThat(sd, chi_so, ten), ""
        except Exception as e:
            return MicGiaLap(), f"không mở được micro '{ten}' ({e}); chuyển sang giả lập"
    return MicGiaLap(), ""


def mo_loa(sd):
    """Mở loa mặc định; trả về (LoaThat hoặc None, lý do nếu None)."""
    if sd is None:
        return None, "không có sounddevice – chỉ đo mức"
    if not liet_ke(sd, thu=False):
        return None, "không có thiết bị phát – chỉ đo mức"
    try:
        return LoaThat(sd), ""
    except Exception as e:
        return None, f"không mở được loa ({e}) – chỉ đo mức"
