"""Bài mở rộng MR5.4 (b) – Server TCP đa luồng: mỗi client một luồng và nhóm luồng cố định.

Vai trò server mở hai cổng:
  cổng P   – máy chủ A: luồng "nhận-A" accept(), mỗi kết nối tạo MỘT LUỒNG MỚI "kết-nối-k";
  cổng P+1 – máy chủ B: nhóm 2 luồng cố định "nhóm-1", "nhóm-2" lấy việc từ hàng đợi giới hạn 2 phần tử;
             luồng "nhận-B" đọc dòng yêu cầu rồi put_nowait(); hàng đợi đầy → trả "503 BAN <mã>" ngay.
Yêu cầu:  "VIEC <mã> <ms>"  → server chờ <ms> (giả lập xử lý) rồi trả "XONG <mã> <tên luồng xử lý>".

Vai trò tai (máy tạo tải): 6 luồng "khách-k" lần lượt (cách nhau 20 ms) gửi VIEC k 300 tới từng máy chủ,
in kết quả và bảng so sánh.
"""
import queue
import socket
import threading
import time

from nhat_ky import log, say

N_KHACH, MS_VIEC, CACH_MS = 6, 300, 20
NHOM, HANG_DOI = 2, 2


def xu_ly(f, parts):
    """Phần việc giống nhau ở hai máy chủ: chờ <ms> rồi trả lời kèm tên luồng đang chạy."""
    log(f"xử lý VIEC {parts[1]} {parts[2]}")
    time.sleep(int(parts[2]) / 1000)
    f.write(f"XONG {parts[1]} {threading.current_thread().name}\n")
    f.flush()


# ---------------- Máy chủ A: mỗi client một luồng ----------------
def may_chu_a(srv):
    dem = 0
    while True:
        conn, _ = srv.accept()
        dem += 1
        log(f"kết nối mới → tạo luồng kết-nối-{dem}")
        threading.Thread(target=phuc_vu_a, args=(conn,), name=f"kết-nối-{dem}", daemon=True).start()


def phuc_vu_a(conn):
    with conn, conn.makefile("rw", encoding="utf-8", newline="\n") as f:
        parts = f.readline().split()
        if len(parts) == 3 and parts[0] == "VIEC":
            xu_ly(f, parts)


# ---------------- Máy chủ B: nhóm luồng cố định + hàng đợi giới hạn ----------------
def may_chu_b(srv):
    viec = queue.Queue(maxsize=HANG_DOI)          # hàng đợi có giới hạn: không phình vô hạn khi quá tải
    for i in range(1, NHOM + 1):
        threading.Thread(target=tho_b, args=(viec,), name=f"nhóm-{i}", daemon=True).start()
    while True:
        conn, _ = srv.accept()
        f = conn.makefile("rw", encoding="utf-8", newline="\n")
        parts = f.readline().split()
        if len(parts) != 3 or parts[0] != "VIEC":
            conn.close()
            continue
        try:
            viec.put_nowait((conn, f, parts))       # không chờ: đầy thì từ chối ngay
            log(f"VIEC {parts[1]} vào hàng đợi")
        except queue.Full:
            log(f"hàng đợi đầy ({HANG_DOI}/{HANG_DOI}) → trả 503 cho VIEC {parts[1]}")
            f.write(f"503 BAN {parts[1]}\n")
            f.flush()
            f.close()
            conn.close()


def tho_b(viec):
    """Luồng thợ của nhóm: lặp mãi, mỗi lần lấy một việc (chặn khi hàng đợi rỗng)."""
    while True:
        conn, f, parts = viec.get()
        with conn, f:
            xu_ly(f, parts)


def run_server(host, port):
    pa, pb = int(port), int(port) + 1
    srv_a = socket.create_server((host, pa), backlog=64)
    srv_b = socket.create_server((host, pb), backlog=64)
    say(f"[server] cổng {pa}: máy chủ A – mỗi client một luồng")
    say(f"[server] cổng {pb}: máy chủ B – nhóm {NHOM} luồng cố định, hàng đợi tối đa {HANG_DOI} (đầy → 503)")
    threading.Thread(target=may_chu_a, args=(srv_a,), name="nhận-A", daemon=True).start()
    threading.Thread(target=may_chu_b, args=(srv_b,), name="nhận-B", daemon=True).start()
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass


# ---------------- Máy tạo tải ----------------
def khach(host, port, k, kq):
    time.sleep((k - 1) * CACH_MS / 1000)          # khách k xuất phát muộn hơn khách 1 là (k-1)×20 ms
    t = time.perf_counter()
    with socket.create_connection((host, port), timeout=10) as s:
        s.sendall(f"VIEC {k} {MS_VIEC}\n".encode())
        line = s.makefile(encoding="utf-8").readline().strip()
    dt = time.perf_counter() - t
    kq[k] = line
    parts = line.split()
    if parts and parts[0] == "XONG":
        log(f"XONG {k} bởi {parts[2]} sau {dt:.2f} s")
    else:
        log(f"{line} (bị từ chối) sau {dt:.2f} s")


def mot_dot(host, port):
    kq = {}
    t = time.perf_counter()
    ths = [threading.Thread(target=khach, args=(host, port, k, kq), name=f"khách-{k}")
           for k in range(1, N_KHACH + 1)]
    for th in ths:
        th.start()
    for th in ths:
        th.join()
    ok = sum(1 for v in kq.values() if v.startswith("XONG"))
    return ok, N_KHACH - ok, time.perf_counter() - t


def run_tai(host, port):
    pa, pb = int(port), int(port) + 1
    say(f"(b) Máy chủ đa luồng: {N_KHACH} client đồng thời (cách nhau {CACH_MS} ms), "
        f"mỗi yêu cầu xử lý {MS_VIEC} ms")
    say(f"Máy chủ A – mỗi client một luồng (cổng {pa}):")
    a = mot_dot(host, pa)
    say(f"Máy chủ B – nhóm {NHOM} luồng, hàng đợi {HANG_DOI} (cổng {pb}):")
    b = mot_dot(host, pb)
    say()
    say(f"  {'Máy chủ':<29}{'Thành công':>11}{'Từ chối':>9}{'Thời gian':>10}")
    for ten, (ok, tu_choi, dt) in [("A. mỗi client một luồng", a),
                                   (f"B. nhóm {NHOM} luồng + hàng đợi {HANG_DOI}", b)]:
        say(f"  {ten:<29}{ok:>11}{tu_choi:>9}{dt:>8.2f} s")
    say("Nhận xét: A tạo luồng không giới hạn – nhanh khi ít client nhưng quá tải sẽ cạn bộ nhớ;")
    say(f"  B giữ số luồng cố định, chỉ nhận {NHOM + HANG_DOI} việc cùng lúc, phần vượt bị từ chối ngay (503)")
    say("  thay vì xếp hàng vô hạn – client biết để thử lại sau.")
