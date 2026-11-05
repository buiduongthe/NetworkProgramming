"""Bài mở rộng MR5.4 – Nhật ký theo luồng (thread name + biến cục bộ luồng).

Mỗi dòng nhật ký có dạng
    [  0.123 s] tải-1 yc=1      | nội dung
- thời điểm tính từ lúc chương trình khởi động (time.perf_counter);
- tên luồng (threading.current_thread().name) cho biết LUỒNG NÀO làm việc đó;
- ngữ cảnh "yc=..." lấy từ biến cục bộ luồng (threading.local): mỗi luồng có bản sao riêng,
  luồng đặt ngữ cảnh một lần (dat_ngu_canh) rồi mọi lời gọi log() sau đó tự có – không cần truyền
  mã yêu cầu qua từng hàm (giống MDC của các thư viện ghi nhật ký).
Một khóa (_khoa_in) bảo đảm hai luồng không in xen lẫn nhau trên cùng một dòng.
"""
import threading
import time

T0 = time.perf_counter()
_cuc_bo = threading.local()          # biến cục bộ luồng: mỗi luồng thấy giá trị của riêng mình
_khoa_in = threading.Lock()


def dat_ngu_canh(text):
    """Đặt ngữ cảnh (ví dụ "yc=3") cho LUỒNG HIỆN TẠI; luồng khác không bị ảnh hưởng."""
    _cuc_bo.ngu_canh = text


def bay_gio():
    """Số giây kể từ lúc chương trình khởi động."""
    return time.perf_counter() - T0


def log(msg):
    ctx = getattr(_cuc_bo, "ngu_canh", "")
    ai = (threading.current_thread().name + " " + ctx).strip()
    with _khoa_in:
        print(f"[{bay_gio():7.3f} s] {ai:<16}| {msg}", flush=True)


def say(msg=""):
    """In một dòng bình thường (tiêu đề, bảng) – vẫn qua khóa để không xen với log()."""
    with _khoa_in:
        print(msg, flush=True)
