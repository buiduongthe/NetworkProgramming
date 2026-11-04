"""Bài 7.6 – Server SMTP thử nghiệm cục bộ (aiosmtpd) cho demo client ở cả ba ngôn ngữ.

Bắt buộc STARTTLS (chứng chỉ tự ký cert.pem/key.pem, tạo bằng tao_chung_chi.sh) và AUTH sau TLS;
tài khoản thử lấy từ biến môi trường SMTP_USER, SMTP_PASS (mặc định sinhvien / matkhau-thu).
Thư nhận được chỉ được in ra màn hình, không chuyển đi đâu.

Chuẩn bị: pip install -r requirements.txt && sh tao_chung_chi.sh
Chạy:     python3 smtp_server_thu.py [host] [port]      (mặc định 127.0.0.1 587; cổng < 1024 cần quyền quản trị)
"""
import os
import signal
import ssl
import sys
import logging
import threading

from aiosmtpd.controller import Controller
from aiosmtpd.smtp import AuthResult, LoginPassword

logging.getLogger("mail.log").setLevel(logging.ERROR)     # ẩn cảnh báo nội bộ của aiosmtpd
USER = os.environ.get("SMTP_USER", "sinhvien").encode()
PASS = os.environ.get("SMTP_PASS", "matkhau-thu").encode()


def authenticator(server, session, envelope, mechanism, auth_data):
    ok = isinstance(auth_data, LoginPassword) and auth_data.login == USER and auth_data.password == PASS
    who = auth_data.login.decode() if isinstance(auth_data, LoginPassword) else "?"
    print(f"[server thử] AUTH {mechanism} của '{who}': {'thành công' if ok else 'sai thông tin'}", flush=True)
    return AuthResult(success=ok, handled=False)      # handled=False: aiosmtpd tự trả 235/535


class Handler:
    async def handle_DATA(self, server, session, envelope):
        print(f"[server thử] Thư từ {envelope.mail_from} tới {', '.join(envelope.rcpt_tos)} "
              f"({len(envelope.content)} byte):", flush=True)
        for line in envelope.content.decode("utf-8", "replace").splitlines():
            print(f"[server thử]   | {line}", flush=True)
        return "250 OK: thư đã được nhận"


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 587
    here = os.path.dirname(os.path.abspath(__file__))
    ctx = ssl.create_default_context(ssl.Purpose.CLIENT_AUTH)
    ctx.load_cert_chain(os.path.join(here, "cert.pem"), os.path.join(here, "key.pem"))
    ctl = Controller(Handler(), hostname=host, port=port, tls_context=ctx, require_starttls=True,
                     authenticator=authenticator, auth_require_tls=True, server_hostname="smtp.thu.local")
    ctl.start()
    print(f"[server thử] SMTP tại {host}:{port} (bắt buộc STARTTLS, AUTH sau TLS)", flush=True)
    stop = threading.Event()
    signal.signal(signal.SIGINT, lambda *_: stop.set())
    signal.signal(signal.SIGTERM, lambda *_: stop.set())
    stop.wait()
    ctl.stop()
    print("[server thử] Dừng server", flush=True)


if __name__ == "__main__":
    main()
