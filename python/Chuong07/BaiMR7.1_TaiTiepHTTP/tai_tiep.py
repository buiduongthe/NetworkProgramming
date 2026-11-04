"""Bài mở rộng MR7.1 – Client tải tệp HTTP có tải tiếp (resume) khi kết nối bị đứt.

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 7.1 (client HTTP), Bài 5.6 (thử lại có
lùi thời gian) và Bài 3.4 (kiểm tra SHA-256). Quy trình:
  1. HEAD lấy kích thước, ETag và mã băm SHA-256 của tệp.
  2. GET với "Range: bytes=<đã_có>-" và "If-Range: <ETag>"; ghi nối vào tệp cục bộ.
     206 -> nối tiếp; 200 -> server gửi lại từ đầu (tệp đã đổi hoặc không hỗ trợ Range).
  3. Nếu nhận ít hơn Content-Length (kết nối đứt) -> chờ lùi thời gian rồi thử lại từ chỗ dừng.
  4. Kiểm tra kích thước và SHA-256.
Lưu ý: http.client KHÔNG báo lỗi khi kết nối đóng sớm; phải tự so số byte với Content-Length.

Chạy:  python3 tai_tiep.py <url> <tệp_lưu> [--so-lan-thu N]
"""
import argparse
import hashlib
import http.client
import os
import sys
import time
from urllib.parse import urlsplit


def head(u):
    conn = http.client.HTTPConnection(u.hostname, u.port or 80, timeout=5)
    conn.request("HEAD", u.path)
    r = conn.getresponse()
    r.read()
    conn.close()
    if r.status != 200:
        raise RuntimeError(f"HEAD trả {r.status}")
    return int(r.getheader("Content-Length")), r.getheader("ETag"), r.getheader("X-Checksum-SHA256")


def get_range(u, path, have, etag):
    """Một lần GET; trả về số byte có trong tệp cục bộ sau lần này."""
    headers = {}
    if have > 0:
        headers = {"Range": f"bytes={have}-", "If-Range": etag}
    conn = http.client.HTTPConnection(u.hostname, u.port or 80, timeout=5)
    try:
        conn.request("GET", u.path, headers=headers)
        r = conn.getresponse()
        print(f"[tai] -> {r.status} {r.reason}"
              + (f", {r.getheader('Content-Range')}" if r.status == 206 else ""), flush=True)
        if r.status == 416:                     # dải không thỏa mãn được: giữ nguyên phần đã có
            r.read()
            return have
        if r.status == 200:
            mode, have = "wb", 0                # server gửi toàn bộ: ghi đè từ đầu
        elif r.status == 206:
            mode = "ab"
        else:
            raise RuntimeError(f"Mã trạng thái không mong đợi {r.status}")
        expected = int(r.getheader("Content-Length"))
        got = 0
        with open(path, mode) as f:
            while got < expected:
                chunk = r.read(min(16384, expected - got))
                if not chunk:                   # kết nối đóng sớm: http.client không báo lỗi
                    break
                f.write(chunk)
                got += len(chunk)
        if got < expected:
            print(f"[tai] Kết nối đứt: nhận {got}/{expected} byte của lần này", flush=True)
        return have + got
    finally:
        conn.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("url")
    ap.add_argument("out")
    ap.add_argument("--so-lan-thu", type=int, default=5)
    a = ap.parse_args()
    u = urlsplit(a.url)
    size, etag, sha = head(u)
    print(f"[tai] HEAD: {size} byte, ETag {etag}", flush=True)
    have = os.path.getsize(a.out) if os.path.exists(a.out) else 0
    for attempt in range(1, a.so_lan_thu + 1):
        if have >= size:
            break
        print(f"[tai] Lần {attempt}: đã có {have} byte, xin "
              + (f"Range: bytes={have}-" if have else "toàn bộ"), flush=True)
        try:
            have = get_range(u, a.out, have, etag)
        except (OSError, http.client.HTTPException) as e:
            print(f"[tai] Lỗi: {e}", flush=True)
            have = os.path.getsize(a.out) if os.path.exists(a.out) else 0
        if have < size:
            delay = 0.2 * 2 ** (attempt - 1)    # lùi thời gian theo cấp số nhân (Bài 5.6)
            print(f"[tai] Chờ {delay:.1f} s rồi tải tiếp", flush=True)
            time.sleep(delay)
    if have < size:
        print(f"[tai] THẤT BẠI sau {a.so_lan_thu} lần thử: có {have}/{size} byte", flush=True)
        sys.exit(1)
    digest = hashlib.sha256(open(a.out, "rb").read()).hexdigest()
    print(f"[tai] Đủ {have}/{size} byte; SHA-256 {'KHỚP' if digest == sha else 'SAI'} ({digest[:16]}...)",
          flush=True)


if __name__ == "__main__":
    main()
