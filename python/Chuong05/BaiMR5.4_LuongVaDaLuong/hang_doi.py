"""Bài mở rộng MR5.4 (d) – Nhà sản xuất – người tiêu dùng trên một kết nối TCP.

  luồng "nhận"  : gửi "PHAT 9" tới dịch vụ thử, đọc từng dòng "GOI k dữ-liệu" từ socket,
                  put() vào hàng đợi việc (giới hạn 4 phần tử – đầy thì luồng nhận phải CHỜ: áp lực ngược,
                  không đọc thêm từ socket → bộ đệm TCP đầy → bên gửi cũng chậm lại);
  3 luồng "xử-lý-k": get() một gói, xử lý (80 ms + tính tổng byte), put() kết quả vào hàng đợi kết quả;
  luồng "gửi"   : lấy kết quả, ghi "KQ k tổng" vào CÙNG socket (một luồng đọc, một luồng ghi: an toàn).
Kết thúc êm (graceful shutdown) bằng "viên thuốc độc" (poison pill): hết dữ liệu, luồng nhận đặt 3 giá trị
None vào hàng đợi việc – mỗi luồng xử lý gặp None thì báo "đã dừng" cho luồng gửi rồi thoát; luồng gửi đếm
đủ 3 báo dừng thì thoát. Không luồng nào bị giết giữa chừng, không gói nào bị bỏ sót.
(Cách khác: cờ hủy threading.Event mà mọi luồng kiểm tra định kỳ – dùng khi cần dừng NGAY, bỏ việc dở.)
"""
import queue
import socket
import threading
import time

from dich_vu import tong_byte
from nhat_ky import dat_ngu_canh, log, say

N_GOI, N_THO, SUC_CHUA, MS_XU_LY = 9, 3, 4, 80
DUNG = None                           # viên thuốc độc


def run(host, port):
    say(f"(d) Nhà sản xuất – người tiêu dùng: luồng nhận đọc socket → hàng đợi (tối đa {SUC_CHUA}) → "
        f"{N_THO} luồng xử lý → luồng gửi")
    viec = queue.Queue(maxsize=SUC_CHUA)
    ket_qua = queue.Queue()
    dem = {}
    so_lan_cho = [0]
    s = socket.create_connection((host, int(port)), timeout=10)
    f_doc = s.makefile("r", encoding="utf-8")
    f_ghi = s.makefile("w", encoding="utf-8", newline="\n")

    def nhan():
        log(f"kết nối dịch vụ, gửi PHAT {N_GOI}")
        f_ghi.write(f"PHAT {N_GOI}\n")
        f_ghi.flush()
        for _ in range(N_GOI):
            _, k, text = f_doc.readline().split()
            goi = (int(k), text)
            try:
                viec.put_nowait(goi)
            except queue.Full:
                so_lan_cho[0] += 1
                log(f"hàng đợi đầy ({SUC_CHUA}/{SUC_CHUA}) → chờ chỗ trống (áp lực ngược)")
                viec.put(goi)                    # chặn tới khi một luồng xử lý lấy bớt
            log(f"GOI {k} → hàng đợi")
        log(f"hết dữ liệu → đặt {N_THO} viên thuốc độc (poison pill) vào hàng đợi")
        for _ in range(N_THO):
            viec.put(DUNG)

    def xu_ly():
        ten = threading.current_thread().name
        dem[ten] = 0
        while True:
            goi = viec.get()
            if goi is DUNG:
                dat_ngu_canh("")
                log("gặp viên thuốc độc → dừng")
                ket_qua.put(DUNG)
                return
            k, text = goi
            dat_ngu_canh(f"yc={k}")              # ngữ cảnh cục bộ luồng: log() tự ghi "yc=k"
            time.sleep(MS_XU_LY / 1000)          # giả lập xử lý tốn thời gian
            v = tong_byte(text)
            log(f"xử lý GOI {k}: tổng byte = {v}")
            dem[ten] += 1
            ket_qua.put((k, v))

    def gui():
        con_lai = N_THO
        while con_lai:
            kq = ket_qua.get()
            if kq is DUNG:
                con_lai -= 1
                continue
            f_ghi.write(f"KQ {kq[0]} {kq[1]}\n")
            f_ghi.flush()
            log(f"gửi KQ {kq[0]} {kq[1]}")
        log(f"cả {N_THO} luồng xử lý đã dừng → hết việc gửi")

    t = time.perf_counter()
    ths = [threading.Thread(target=nhan, name="nhận")]
    ths += [threading.Thread(target=xu_ly, name=f"xử-lý-{i}") for i in range(1, N_THO + 1)]
    ths += [threading.Thread(target=gui, name="gửi")]
    for th in ths:
        th.start()
    for th in ths:
        th.join()
    tra_loi = f_doc.readline().strip()
    dt = time.perf_counter() - t
    s.close()
    log(f"dịch vụ trả lời: {tra_loi}")
    say()
    say(f"  {'Luồng':<12}{'Số gói đã xử lý':>16}")
    for i in range(1, N_THO + 1):
        say(f"  {'xử-lý-' + str(i):<12}{dem.get(f'xử-lý-{i}', 0):>16}")
    say(f"  Luồng nhận phải chờ vì hàng đợi đầy: {so_lan_cho[0]} lần; tổng {dt:.2f} s "
        f"(1 luồng xử lý ≈ {N_GOI * MS_XU_LY / 1000:.2f} s)")
