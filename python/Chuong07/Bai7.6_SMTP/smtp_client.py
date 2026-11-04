"""Bài 7.6 – Phiên gửi thư SMTP thủ công trên socket, có STARTTLS và AUTH PLAIN.

Cài đặt Mã giả 7.7 (Mục 7.5.1, Giáo trình Lập trình mạng): SMTP_REPLY, CMD, SEND_MAIL, DOT_STUFF.
Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.5 (READ_LINE). TLS_WRAP dùng mô-đun ssl (Mục 8.4).
Tài khoản lấy từ biến môi trường SMTP_USER, SMTP_PASS (không ghi mật khẩu trong mã nguồn).

Chạy:  python3 smtp_client.py [host] [port] [tệp_CA]
       (mặc định localhost 587 cert.pem; tệp_CA = "-" dùng kho chứng chỉ của hệ điều hành)
"""
import base64
import os
import socket
import ssl
import sys
import time


class SmtpError(Exception):
    def __init__(self, code, text):
        super().__init__(f"{code} {' '.join(text)}")
        self.code = code


class ProtocolError(Exception):
    """Phản hồi của server vi phạm cú pháp SMTP."""


class Reader:
    """Mã giả 3.5 – Reader { sock, pending } và READ_LINE."""

    def __init__(self, sock):
        self.sock, self.pending = sock, b""

    def read_line(self, max_len):
        while (i := self.pending.find(b"\n")) < 0:
            if len(self.pending) > max_len:
                raise ValueError("Dòng quá dài")
            chunk = self.sock.recv(4096)
            if not chunk:
                return None                                  # server đã đóng kết nối
            self.pending += chunk
        if i > max_len:                                  # Mã giả 3.5 dòng 5: có LF nhưng quá dài
            raise ValueError("Dòng quá dài")
        line, self.pending = self.pending[:i], self.pending[i + 1:]
        return line.rstrip(b"\r").decode("utf-8")


def smtp_reply(r):
    """Mã giả 7.7, dòng 1–11: đọc phản hồi nhiều dòng ("250-..." ... "250 ...")."""
    text = []
    while True:
        l = r.read_line(1024)
        if l is None or len(l) < 3:                   # dòng 5: server đóng hoặc dòng quá ngắn
            raise ProtocolError(f"Phản hồi SMTP không hợp lệ: {l!r}")
        print(f"[client] S: {l}", flush=True)
        text.append(l[4:])                            # dòng 6: rỗng nếu l chỉ có 3 chữ số
        if len(l) == 3 or l[3] == " ":                # dòng 7: dòng cuối
            return int(l[0:3]), text


def cmd(r, line, expect, shown=None):
    """Mã giả 7.7, dòng 12–16: gửi lệnh, kiểm tra chữ số đầu của mã phản hồi."""
    print(f"[client] C: {shown or line}", flush=True)
    r.sock.sendall((line + "\r\n").encode("utf-8"))
    code, text = smtp_reply(r)
    if code // 100 != expect:
        raise SmtpError(code, text)


def dot_stuff(msg):
    """Thêm '.' vào đầu dòng bắt đầu bằng '.', chuẩn hóa xuống dòng về CRLF (Mục 3.4.2)."""
    return "\r\n".join(("." + l if l.startswith(".") else l) for l in msg.split("\n"))


def send_mail(host, port, cafile, user, secret, sender, to, msg):
    """Mã giả 7.7, dòng 17–31."""
    s = socket.create_connection((host, port), timeout=5)       # SAFE_CONNECT(host, 587, 5000)
    r = Reader(s)
    try:
        code, _ = smtp_reply(r)                                    # 220: lời chào của server
        if code != 220:
            raise SmtpError(code, ["Không phải lời chào 220"])
        cmd(r, "EHLO client.example", 2)
        cmd(r, "STARTTLS", 2)                                      # 220: sẵn sàng TLS
        ctx = ssl.create_default_context(cafile=None if cafile == "-" else cafile)
        r = Reader(ctx.wrap_socket(s, server_hostname=host))       # TLS_WRAP(s, CLIENT, host): Reader mới
        print(f"[client] Đã thiết lập {r.sock.version()}, chứng chỉ server hợp lệ cho '{host}'", flush=True)
        cmd(r, "EHLO client.example", 2)                           # chào lại sau khi có TLS
        token = base64.b64encode(b"\0" + user.encode() + b"\0" + secret.encode()).decode("ascii")
        cmd(r, "AUTH PLAIN " + token, 2, shown="AUTH PLAIN ****")  # không in thông tin xác thực
        cmd(r, f"MAIL FROM:<{sender}>", 2)
        cmd(r, f"RCPT TO:<{to}>", 2)
        cmd(r, "DATA", 3)                                          # 354: bắt đầu nội dung
        body = dot_stuff(msg)
        cmd(r, body + "\r\n.", 2, shown=f"<nội dung thư {len(body.encode())} byte> + dòng '.'")
        cmd(r, "QUIT", 2)
    finally:
        r.sock.close()


def build_message(sender, to):
    """Thư Internet tối giản: tiêu đề (Subject mã hóa MIME UTF-8), dòng trống, thân văn bản UTF-8."""
    subject = "=?utf-8?B?" + base64.b64encode("Thư thử nghiệm Bài 7.6".encode()).decode() + "?="
    return "\n".join([
        f"From: <{sender}>", f"To: <{to}>", f"Subject: {subject}",
        "Date: " + time.strftime("%a, %d %b %Y %H:%M:%S +0000", time.gmtime()),
        f"Message-ID: <{int(time.time())}@client.example>",
        "MIME-Version: 1.0", "Content-Type: text/plain; charset=utf-8", "Content-Transfer-Encoding: 8bit",
        "", "Xin chào, đây là thư gửi bằng phiên SMTP tự cài đặt.",
        ".Dòng này bắt đầu bằng dấu chấm (được DOT_STUFF thêm một dấu chấm).", "Hết thư."])


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "localhost"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 587
    cafile = sys.argv[3] if len(sys.argv) >= 4 else "cert.pem"
    user, secret = os.environ.get("SMTP_USER", ""), os.environ.get("SMTP_PASS", "")
    sender, to = "sinhvien@example.com", "giangvien@example.com"
    try:
        send_mail(host, port, cafile, user, secret, sender, to, build_message(sender, to))
        print("[client] Gửi thư thành công", flush=True)
    except SmtpError as e:
        print(f"[client] Lỗi SMTP: {e}", flush=True)
        sys.exit(1)
    except ProtocolError as e:
        print(f"[client] Lỗi giao thức: {e}", flush=True)
        sys.exit(1)
    except ssl.SSLError as e:                                      # chứng chỉ không tin cậy hoặc sai tên máy
        print(f"[client] Lỗi TLS: {e}", flush=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
