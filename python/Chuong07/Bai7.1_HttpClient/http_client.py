"""Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP.

Cài đặt Mã giả 7.1 (Mục 7.2.1, Giáo trình Lập trình mạng): HTTP_REQUEST, READ_HEADERS, READ_CHUNKED.
Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.4 (RECV_EXACT), 3.5 (READ_LINE, cấu trúc Reader).
Client gửi GET tới từng đường dẫn và in mã trạng thái, cách định khung thân và nội dung thân.

Chạy:  python3 http_client.py [host] [port] [đường_dẫn ...]   (mặc định 127.0.0.1 8080 /)
"""
import socket
import sys


def safe_connect(host, port, timeout_ms):
    """Mã giả 2.2 (rút gọn): thử lần lượt các địa chỉ, có thời gian chờ kết nối."""
    last_error = None
    for family, socktype, proto, _, addr in socket.getaddrinfo(host, port, type=socket.SOCK_STREAM):
        s = socket.socket(family, socktype, proto)
        s.settimeout(timeout_ms / 1000)
        try:
            s.connect(addr)
            return s
        except OSError as e:
            last_error = e
            s.close()
    raise ConnectionError(f"Không kết nối được {host}:{port}: {last_error}")


class ConnectionClosed(ConnectionError):
    """Đối tác đóng kết nối giữa chừng một thông điệp."""


class Reader:
    """Mã giả 3.5 – STRUCT Reader { sock, pending }: giữ phần byte nhận dư giữa các lần đọc."""

    def __init__(self, sock):
        self.sock = sock
        self.pending = b""


def read_line(r, max_len):
    """Mã giả 3.5 – READ_LINE: trả về dòng (bỏ CRLF) hoặc None khi đối tác đã đóng."""
    while True:
        i = r.pending.find(b"\n")
        if i > max_len:                           # dòng 5: có LF nhưng dòng quá dài
            raise ValueError("Dòng quá dài")
        if i >= 0:
            line, r.pending = r.pending[:i], r.pending[i + 1:]
            return line.rstrip(b"\r").decode("utf-8")
        if len(r.pending) > max_len:
            raise ValueError("Dòng quá dài")
        chunk = r.sock.recv(4096)
        if not chunk:
            return None
        r.pending += chunk


def read_exact(r, n):
    """READ_EXACT: lấy phần dư r.pending trước, thiếu thì RECV_EXACT (Mã giả 3.4)."""
    data, r.pending = r.pending[:n], r.pending[n:]
    while len(data) < n:
        chunk = r.sock.recv(n - len(data))
        if not chunk:
            raise ConnectionError(f"Kết nối đóng khi mới nhận {len(data)}/{n} byte")
        data += chunk
    return data


def read_until_close(r):
    """READ_UNTIL_CLOSE: đọc đến khi server đóng kết nối."""
    data, r.pending = r.pending, b""
    while chunk := r.sock.recv(4096):
        data += chunk
    return data


def read_headers(r):
    """Mã giả 7.1, dòng 25–34: đọc các trường tiêu đề đến dòng trống."""
    hdr = {}
    while True:
        line = read_line(r, 8192)
        if line is None:                          # dòng 29: server đóng giữa phần tiêu đề
            raise ConnectionClosed("Kết nối đóng khi chưa đọc hết phần tiêu đề")
        if line == "":                            # dòng 30: dòng trống – hết tiêu đề
            return hdr
        k, _, v = line.partition(":")             # SPLIT_FIRST tại dấu ':' đầu tiên
        hdr[k.strip().lower()] = v.strip()        # tên trường không phân biệt hoa/thường


def read_chunked(r):
    """Mã giả 7.1, dòng 35–45: giải mã thân Transfer-Encoding: chunked."""
    data = b""
    while True:
        n = int(read_line(r, 1024).split(";")[0], 16)   # bỏ phần mở rộng sau ';'
        if n == 0:
            break
        data += read_exact(r, n)
        read_line(r, 2)                                   # CRLF sau mỗi khối
    read_headers(r)                                       # bỏ phần trailer
    return data


def http_request(host, port, method, path, headers, body=None):
    """Mã giả 7.1, dòng 1–23: gửi một yêu cầu HTTP/1.1 và đọc phản hồi."""
    s = safe_connect(host, port, 3000)
    s.settimeout(5.0)
    r = Reader(s)
    try:
        h = f"{method} {path} HTTP/1.1\r\n"
        h += f"Host: {host}:{port}\r\n"                   # kèm cổng
        for k, v in headers.items():
            h += f"{k}: {v}\r\n"
        if body is not None:
            h += f"Content-Length: {len(body)}\r\n"
        h += "Connection: close\r\n\r\n"                  # dòng trống kết thúc tiêu đề
        s.sendall(h.encode("ascii") + (body or b""))      # SEND_ALL
        line = read_line(r, 8192)
        if line is None:
            raise ConnectionError("Server đóng kết nối trước khi gửi phản hồi")
        status = int(line.split(" ")[1])
        hdr = read_headers(r)
        te = hdr.get("transfer-encoding", "").lower()
        if te.endswith("chunked"):                        # vd. "gzip, chunked"
            data = read_chunked(r)
        elif "content-length" in hdr:
            data = read_exact(r, int(hdr["content-length"]))
        else:
            data = read_until_close(r)
        return status, hdr, data
    finally:
        s.close()


def main():
    args = sys.argv[1:]
    host = args[0] if len(args) >= 1 else "127.0.0.1"
    port = int(args[1]) if len(args) >= 2 else 8080
    paths = args[2:] or ["/"]
    for path in paths:
        status, hdr, data = http_request(host, port, "GET", path, {"Accept": "application/json"})
        print(f"[client] GET {path} -> {status}", flush=True)
        if hdr.get("transfer-encoding", "").lower().endswith("chunked"):
            how = "Transfer-Encoding = chunked"
        elif "content-length" in hdr:
            how = f"Content-Length = {hdr['content-length']}"
        else:
            how = "đọc đến khi server đóng kết nối"
        print(f"[client] Định khung thân: {how}", flush=True)
        print(f"[client] Thân ({len(data)} byte): {data.decode('utf-8')}", flush=True)


if __name__ == "__main__":
    main()
