"""Bài mở rộng MR5.5 – Phần dùng chung: hằng số, việc CPU (băm), hàm "thư viện cũ" kiểu chặn, nhật ký.

Giao thức (mỗi kết nối một yêu cầu, dòng văn bản kết thúc bằng \\n):
  BAM <mã> <chế_độ> <số_byte> <số_vòng> <hạn_ms>\\n + <số_byte> byte dữ liệu
      chế_độ: chan    – A. làm việc CPU và gọi hàm chặn NGAY TRÊN vòng lặp sự kiện (cách làm SAI)
              tach    – B. đẩy hàm chặn sang nhóm luồng chặn, việc băm sang nhóm luồng tính
              gioihan – C. như B + áp lực ngược: semaphore bất đồng bộ cho tối đa 4 yêu cầu vào nhóm
      → "OK <mã> <16 ký tự hex đầu của SHA-256>"  hoặc  "HETGIO <mã>" khi quá hạn
  CAMBIEN <mã>     → "CAMBIEN <mã> <giá trị> <tên luồng đã hoàn tất future>"
  DATLAI           → "OK" (xóa thống kê)
  THONGKE <nhãn>   → "TK <trễ nhịp tối đa ms> <số mẫu cảm biến> <trễ giao mẫu tối đa ms> <việc ở nhóm tính tối đa>"
"""
import hashlib
import threading
import time

N_YC = 16                 # số yêu cầu đồng thời mỗi đợt
SO_BYTE = 64 * 1024       # dữ liệu mỗi yêu cầu
SO_VONG = 400             # số vòng băm (mỗi vòng băm thêm toàn bộ dữ liệu)
MS_CU = 20                # hàm "thư viện cũ" chặn 20 ms
NHOM_TINH = 2             # số luồng của nhóm tính (bằng số CPU của máy kiểm thử)
GIOI_HAN = 4              # chế độ C: tối đa 4 yêu cầu ở nhóm tính (2 đang chạy + 2 chờ)
NHIP_MS = 10              # nhịp tim của vòng lặp sự kiện
CAM_BIEN_MS = 50          # chu kỳ luồng cảm biến

T0 = time.perf_counter()
_khoa_in = threading.Lock()


def du_lieu(n):
    """Dữ liệu thử: byte thứ i = i mod 251 (giống nhau ở mọi bản ngôn ngữ)."""
    return bytes(i % 251 for i in range(n))


def bam(data, so_vong, huy=None):
    """Việc nặng CPU: SHA-256 của dữ liệu lặp so_vong lần. Kiểm tra cờ hủy trước mỗi vòng.

    hashlib nhả GIL khi băm khối ≥ 2 KB, nên hai luồng tính chạy song song thật trên hai CPU.
    Trả về (hex16, tên luồng) hoặc (None, vòng đã dừng) khi bị hủy.
    """
    h = hashlib.sha256()
    for r in range(so_vong):
        if huy is not None and huy.is_set():
            return None, r
        h.update(data)
    return h.hexdigest()[:16], threading.current_thread().name


def tra_cuu_cu(ma):
    """Giả lập hàm của một thư viện CŨ chỉ có API chặn (ví dụ trình điều khiển CSDL đồng bộ,
    gọi socket chặn): luồng gọi bị giữ MS_CU mili giây. Trả về tên luồng đã chạy nó."""
    time.sleep(MS_CU / 1000)
    return threading.current_thread().name


def log(msg):
    ai = threading.current_thread().name
    with _khoa_in:
        print(f"[{time.perf_counter() - T0:7.3f} s] {ai:<16}| {msg}", flush=True)


def say(msg=""):
    with _khoa_in:
        print(msg, flush=True)
