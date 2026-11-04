"""Bài mở rộng MR8.3 – Phòng thủ tấn công từ chối dịch vụ mức ứng dụng kiểu Slowloris.

Bài mở rộng, KHÔNG có trong giáo trình; nối tiếp Chương 6 (server nhiều client, Bài 6.1/6.2)
và Chương 8 (vận hành an toàn). Mục đích DUY NHẤT là dạy cách *phòng thủ*: so sánh một server
HTTP/1.1 nhỏ ở hai chế độ và đo xem một yêu cầu bình thường còn được phục vụ hay không.

  server vulnerable <host> <port>   – nhóm luồng cố định, KHÔNG đặt hạn đọc tiêu đề (dễ bị đánh gục)
  server protected  <host> <port>   – có hạn đọc tiêu đề, cỡ tiêu đề tối đa, hạn chót rỗi,
                                       giới hạn kết nối theo IP và trần kết nối toàn cục (trả 503)
  slow   <host> <port> [N] [nguồn]  – CLIENT THÍ NGHIỆM: mở N kết nối tới 127.0.0.1 và nhỏ giọt
                                       từng byte tiêu đề (chỉ cho loopback – từ chối mọi đích khác).
                                       Mặc định dùng địa chỉ nguồn 127.0.0.2 để tách IP "kẻ tấn công"
                                       khỏi IP của probe (127.0.0.1), nhờ đó thấy rõ giới hạn theo IP.
  probe  <host> <port>              – gửi MỘT yêu cầu HTTP bình thường, đo thành công/độ trễ

CẢNH BÁO ĐẠO ĐỨC: vai "slow" chỉ dùng trong phòng thí nghiệm cô lập, chỉ nhắm 127.0.0.1 của
chính máy bạn, để kiểm chứng biện pháp phòng thủ. Tuyệt đối không nhắm máy/dịch vụ của người khác.
"""
import ipaddress
import socket
import sys
import threading
import time

# ------------------------------- Tham số phòng thủ (chế độ protected) -------------------------------
HAN_DOC_TIEU_DE = 3.0        # giây: toàn bộ phần đầu (request head) phải xong trong hạn này
HAN_CHOT_ROI = 2.0           # giây: không nhận thêm byte nào trong khoảng này -> coi là rỗi, đóng
CO_TIEU_DE_TOI_DA = 8 * 1024 # byte: tổng kích thước phần đầu tối đa
GIOI_HAN_IP = 10             # số kết nối đồng thời tối đa cho mỗi địa chỉ IP
TRAN_TOAN_CUC = 64           # số kết nối đồng thời tối đa toàn server (vượt -> trả 503 rồi đóng)

SO_LUONG_WORKER = 40         # nhóm luồng cố định (giống MaxRequestWorkers của Apache prefork)
TRA_LOI = (b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n"
           b"Connection: close\r\nContent-Length: 20\r\n\r\nXin chao tu server!\n")


def loopback_hay_khong(host):
    """Kiểm tra host có phải địa chỉ loopback (127.0.0.0/8, ::1) hay không – dùng cho vai 'slow'."""
    try:
        infos = socket.getaddrinfo(host, None)
    except socket.gaierror:
        return False
    for _, _, _, _, sa in infos:
        if not ipaddress.ip_address(sa[0]).is_loopback:
            return False
    return True


# =================================== SERVER ===================================
class ThongKe:
    """Bộ đếm có khóa để các luồng worker cùng ghi."""

    def __init__(self):
        self.khoa = threading.Lock()
        self.chap_nhan = 0        # số kết nối đã chấp nhận
        self.xong = 0             # số yêu cầu phục vụ xong
        self.dong_cham = 0        # số kết nối bị đóng vì gửi tiêu đề quá chậm/quá hạn
        self.tu_choi_503 = 0      # số kết nối bị từ chối do vượt trần/giới hạn IP

    def tang(self, ten, n=1):
        with self.khoa:
            setattr(self, ten, getattr(self, ten) + n)


def doc_tieu_de_co_han(conn, stat):
    """Chế độ PROTECTED: đọc phần đầu HTTP với ba lớp chặn Slowloris.

    (1) hạn chót tuyệt đối HAN_DOC_TIEU_DE cho toàn bộ phần đầu;
    (2) hạn chót rỗi HAN_CHOT_ROI giữa hai lần nhận byte (nhỏ giọt -> timeout);
    (3) tổng cỡ tiêu đề không vượt CO_TIEU_DE_TOI_DA.
    Trả về True nếu nhận đủ phần đầu (kết thúc bằng CRLFCRLF), False nếu phải đóng sớm.
    """
    du_lieu = b""
    han_tuyet_doi = time.monotonic() + HAN_DOC_TIEU_DE
    while b"\r\n\r\n" not in du_lieu:
        con_lai = han_tuyet_doi - time.monotonic()
        if con_lai <= 0:
            return False                                  # (1) quá hạn tổng
        conn.settimeout(min(HAN_CHOT_ROI, con_lai))       # (2) hạn chót rỗi
        try:
            chunk = conn.recv(1024)
        except socket.timeout:
            return False
        if not chunk:
            return False                                  # client đóng giữa chừng
        du_lieu += chunk
        if len(du_lieu) > CO_TIEU_DE_TOI_DA:              # (3) tiêu đề quá to
            return False
    return True


def doc_tieu_de_khong_han(conn):
    """Chế độ VULNERABLE: đọc phần đầu HTTP, KHÔNG đặt hạn -> một client nhỏ giọt giữ worker mãi."""
    conn.settimeout(None)
    du_lieu = b""
    while b"\r\n\r\n" not in du_lieu:
        chunk = conn.recv(1024)
        if not chunk:
            return False
        du_lieu += chunk
    return True


def worker(sock, mode, stat, dang_chay, dem_ip):
    """Một luồng trong nhóm: lặp lấy kết nối từ accept và phục vụ."""
    while dang_chay[0]:
        try:
            conn, addr = sock.accept()
        except OSError:
            return
        stat.tang("chap_nhan")
        ip = addr[0]
        if mode == "protected":
            # Trần toàn cục và giới hạn theo IP: vượt -> 503 rồi đóng ngay (giải phóng worker).
            with stat.khoa:
                tong = sum(dem_ip.values())
                rieng = dem_ip.get(ip, 0)
                qua_tai = tong >= TRAN_TOAN_CUC or rieng >= GIOI_HAN_IP
                if not qua_tai:
                    dem_ip[ip] = rieng + 1
            if qua_tai:
                stat.tang("tu_choi_503")
                try:
                    conn.sendall(b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n"
                                 b"Content-Length: 0\r\n\r\n")
                finally:
                    conn.close()
                continue
            try:
                du = doc_tieu_de_co_han(conn, stat)
                if du:
                    conn.sendall(TRA_LOI)
                    stat.tang("xong")
                else:
                    stat.tang("dong_cham")               # đóng kết nối gửi tiêu đề quá chậm
            except OSError:
                pass
            finally:
                with stat.khoa:
                    dem_ip[ip] = dem_ip.get(ip, 1) - 1
                    if dem_ip[ip] <= 0:
                        dem_ip.pop(ip, None)
                conn.close()
        else:  # vulnerable
            try:
                du = doc_tieu_de_khong_han(conn)         # KẸT Ở ĐÂY nếu client nhỏ giọt
                if du:
                    conn.sendall(TRA_LOI)
                    stat.tang("xong")
            except OSError:
                pass
            finally:
                conn.close()


def chay_server(mode, host, port):
    sock = socket.create_server((host, port), reuse_port=False)
    sock.listen(256)
    stat = ThongKe()
    dang_chay = [True]
    dem_ip = {}
    print(f"[server:{mode}] Nghe tại {host}:{port}, nhóm {SO_LUONG_WORKER} worker. "
          f"Ctrl+C để dừng.", flush=True)
    if mode == "protected":
        print(f"[server:protected] Phòng thủ: hạn đọc tiêu đề {HAN_DOC_TIEU_DE}s, "
              f"hạn rỗi {HAN_CHOT_ROI}s, tiêu đề <= {CO_TIEU_DE_TOI_DA}B, "
              f"<= {GIOI_HAN_IP} kết nối/IP, trần {TRAN_TOAN_CUC} kết nối.", flush=True)
    else:
        print("[server:vulnerable] KHÔNG có hạn đọc tiêu đề: mỗi kết nối chậm giữ một worker.",
              flush=True)
    threads = [threading.Thread(target=worker, args=(sock, mode, stat, dang_chay, dem_ip),
                                daemon=True) for _ in range(SO_LUONG_WORKER)]
    for t in threads:
        t.start()
    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    finally:
        dang_chay[0] = False
        sock.close()
        print(f"[server:{mode}] Dừng. Tổng kết: chấp nhận {stat.chap_nhan}, phục vụ xong "
              f"{stat.xong}, đóng vì chậm {stat.dong_cham}, từ chối 503 {stat.tu_choi_503}.",
              flush=True)


# =================================== CLIENT THÍ NGHIỆM (chỉ loopback) ===================================
def chay_slow(host, port, n, nguon="127.0.0.2"):
    """Mở n kết nối và nhỏ giọt từng byte tiêu đề, KHÔNG bao giờ kết thúc phần đầu.

    Chỉ chạy được khi host là loopback của chính máy bạn. Giữ kết nối khoảng 9 giây rồi thoát.
    Dùng địa chỉ nguồn loopback riêng (mặc định 127.0.0.2) để server phân biệt với probe.
    """
    if not loopback_hay_khong(host):
        sys.exit(f"[slow] TỪ CHỐI: đích {host} không phải loopback. "
                 f"Vai này chỉ dùng cho 127.0.0.1 trong phòng thí nghiệm của chính bạn.")
    if not ipaddress.ip_address(nguon).is_loopback:
        sys.exit(f"[slow] TỪ CHỐI: địa chỉ nguồn {nguon} không phải loopback.")
    print(f"[slow] (CHỈ THÍ NGHIỆM) mở {n} kết nối nhỏ giọt tới {host}:{port} "
          f"từ nguồn {nguon} trong ~9s", flush=True)
    socks = []
    for i in range(n):
        try:
            s = socket.create_connection((host, port), timeout=2, source_address=(nguon, 0))
            s.sendall(b"GET /?slow HTTP/1.1\r\nHost: localhost\r\n")  # phần đầu CHƯA kết thúc
            socks.append(s)
        except OSError:
            pass
    print(f"[slow] đã mở được {len(socks)} kết nối; bắt đầu nhỏ giọt mỗi ~7s một byte", flush=True)
    het_han = time.monotonic() + 9.0
    try:
        while time.monotonic() < het_han:
            time.sleep(7)
            song = 0
            for s in list(socks):
                try:
                    s.sendall(b"X")                       # một byte rác, giữ kết nối "còn sống"
                    song += 1
                except OSError:
                    socks.remove(s)
            print(f"[slow] còn {song} kết nối đang giữ", flush=True)
    except KeyboardInterrupt:
        pass
    for s in socks:
        s.close()
    print("[slow] đóng toàn bộ kết nối thí nghiệm", flush=True)


# =================================== PROBE (yêu cầu bình thường) ===================================
def chay_probe(host, port):
    """Gửi một yêu cầu HTTP/1.1 bình thường, đo xem có được phục vụ không (hạn 4s)."""
    t0 = time.perf_counter()
    try:
        s = socket.create_connection((host, port), timeout=4)
        s.settimeout(4)
        s.sendall(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = s.recv(1024)
            if not chunk:
                break
            data += chunk
        s.close()
        dt = (time.perf_counter() - t0) * 1000
        dong_dau = data.split(b"\r\n", 1)[0].decode("latin1") if data else "(rỗng)"
        if data.startswith(b"HTTP/1.1 200"):
            print(f"[probe] THÀNH CÔNG sau {dt:.0f} ms: {dong_dau}", flush=True)
        else:
            print(f"[probe] phản hồi khác: {dong_dau} ({dt:.0f} ms)", flush=True)
    except (socket.timeout, TimeoutError):
        dt = (time.perf_counter() - t0) * 1000
        print(f"[probe] THẤT BẠI: hết thời gian chờ sau {dt:.0f} ms – server không phục vụ kịp "
              f"(bị tấn công làm nghẽn).", flush=True)
    except OSError as e:
        print(f"[probe] THẤT BẠI: {e}", flush=True)


def main():
    a = sys.argv
    if len(a) >= 5 and a[1] == "server":
        chay_server(a[2], a[3], int(a[4]))
    elif len(a) >= 4 and a[1] == "slow":
        chay_slow(a[2], int(a[3]), int(a[4]) if len(a) > 4 else 150,
                  a[5] if len(a) > 5 else "127.0.0.2")
    elif len(a) >= 4 and a[1] == "probe":
        chay_probe(a[2], int(a[3]))
    else:
        print("Cách dùng:")
        print("  python3 chong_slowloris.py server vulnerable|protected <host> <port>")
        print("  python3 chong_slowloris.py slow  <host> <port> [N]   (chỉ 127.0.0.1)")
        print("  python3 chong_slowloris.py probe <host> <port>")
        sys.exit(1)


if __name__ == "__main__":
    main()
