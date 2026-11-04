"""Bài mở rộng MR3.2 – Tự động hóa cấu hình thiết bị qua CLI (kiểu expect/Netmiko, chỉ dùng socket).

Bài mở rộng, không có trong giáo trình; vận dụng Chương 3: dữ liệu CLI là luồng byte KHÔNG có
khung thông điệp, nên client phải "đọc cho tới khi gặp dấu nhắc" (READ_UNTIL) – một dạng
định khung bằng dấu phân cách, giống READ_LINE ở Bài 3.2 nhưng dấu phân cách là biểu thức chính quy.
Quy trình:
  1. Đăng nhập, vào chế độ đặc quyền (enable), tắt phân trang (terminal length 0).
  2. Với mỗi cổng trong tệp trạng thái mong muốn: đọc cấu hình hiện tại, phân tích,
     so sánh và CHỈ sinh lệnh cho phần khác biệt (lũy đẳng – idempotent).
  3. Áp dụng lệnh (hoặc chỉ in ra với --thu), đọc lại để kiểm chứng, lưu cấu hình.
Tài khoản lấy từ biến môi trường DEVICE_USER, DEVICE_PASSWORD (không ghi cứng trong mã).

Chạy:  python3 tu_dong_hoa.py <host> <port> <muc_tieu.json> [--thu]
"""
import ipaddress
import json
import os
import re
import socket
import sys
import time

PROMPT = re.compile(r"[\w.-]+(\([\w-]+\))?[>#]$")     # R1>  R1#  R1(config)#  R1(config-if)#


class CliError(Exception):
    pass


class CliSession:
    def __init__(self, host, port, timeout=5.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.timeout = timeout
        self.buf = ""

    def read_until(self, pattern):
        """READ_UNTIL: đọc dồn vào bộ đệm cho tới khi phần cuối khớp mẫu; trả về phần đã đọc."""
        deadline = time.monotonic() + self.timeout
        while True:
            text = self.buf.rstrip(" ")
            if pattern.search(text):
                self.buf = ""
                return text
            if "% Authentication failed" in self.buf:
                raise CliError("Đăng nhập thất bại")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise CliError(f"Hết thời gian chờ dấu nhắc, đã nhận: {self.buf[-60:]!r}")
            self.sock.settimeout(remaining)
            data = self.sock.recv(4096)
            if not data:
                raise CliError("Thiết bị đóng kết nối")
            self.buf += data.decode("utf-8")

    def send_line(self, line):
        self.sock.sendall((line + "\n").encode("utf-8"))

    def login(self, user, password):
        self.read_until(re.compile(r"Username:$"))
        self.send_line(user)
        self.read_until(re.compile(r"Password:$"))
        self.send_line(password)
        self.read_until(PROMPT)

    def command(self, cmd):
        """Gửi một lệnh, chờ dấu nhắc kế tiếp, trả về phần kết quả (bỏ dòng vọng lại và dấu nhắc)."""
        self.send_line(cmd)
        out = self.read_until(PROMPT).replace("\r\n", "\n").split("\n")
        if out and out[0].strip() == cmd:
            out = out[1:]                                  # bỏ dòng vọng lại
        body = out[:-1]                                    # bỏ dòng dấu nhắc
        if any(line.startswith("%") for line in body):
            raise CliError(f"Thiết bị từ chối lệnh {cmd!r}: {body[0]}")
        return body

    def close(self):
        try:
            self.send_line("exit")
        except OSError:
            pass
        self.sock.close()


def parse_interface(lines):
    """Phân tích khối 'interface ...' thành dict cùng dạng với tệp mong muốn."""
    cur = {"mo_ta": "", "ip": "", "bat": True}
    for line in lines:
        line = line.strip()
        if line.startswith("description "):
            cur["mo_ta"] = line[len("description "):]
        elif line.startswith("ip address "):
            ip, mask = line.split()[2:4]
            cur["ip"] = str(ipaddress.ip_interface(f"{ip}/{mask}"))
        elif line == "shutdown":
            cur["bat"] = False
    return cur


def diff_commands(cur, want):
    """Chỉ sinh lệnh cho phần khác biệt giữa cấu hình hiện tại và mong muốn."""
    cmds = []
    if cur["mo_ta"] != want["mo_ta"]:
        cmds.append(f"description {want['mo_ta']}" if want["mo_ta"] else "no description")
    if cur["ip"] != want["ip"]:
        iface = ipaddress.ip_interface(want["ip"])
        cmds.append(f"ip address {iface.ip} {iface.netmask}")
    if cur["bat"] != want["bat"]:
        cmds.append("no shutdown" if want["bat"] else "shutdown")
    return cmds


def main():
    if len(sys.argv) < 4:
        sys.exit("Cách dùng: python3 tu_dong_hoa.py <host> <port> <muc_tieu.json> [--thu]")
    host, port, path = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    dry_run = "--thu" in sys.argv[4:]
    want = json.load(open(path, encoding="utf-8"))
    user = os.environ.get("DEVICE_USER", "admin")
    password = os.environ.get("DEVICE_PASSWORD", "")
    print(f"[tu-dong] Kết nối {host}:{port}, tài khoản {user}", flush=True)
    if dry_run:
        print("[tu-dong] Chế độ chạy thử: chỉ in lệnh, không áp dụng", flush=True)
    cli = CliSession(host, port)
    try:
        cli.login(user, password)
        cli.command("enable")
        cli.command("terminal length 0")                  # tắt --More-- để đọc trọn kết quả
        print(f"[tu-dong] {cli.command('show version')[0]}", flush=True)
        total = 0
        for item in want["cong"]:
            name = item["ten"]
            cur = parse_interface(cli.command(f"show running-config interface {name}"))
            cmds = diff_commands(cur, item)
            if not cmds:
                print(f"[tu-dong] {name}: đã đúng, không cần thay đổi", flush=True)
                continue
            print(f"[tu-dong] {name}: cần {len(cmds)} lệnh", flush=True)
            for c in cmds:
                print(f"           + {c}", flush=True)
            if dry_run:
                continue
            cli.command("configure terminal")
            cli.command(f"interface {name}")
            for c in cmds:
                cli.command(c)
            cli.command("end")
            after = parse_interface(cli.command(f"show running-config interface {name}"))
            if after != {k: item[k] for k in ("mo_ta", "ip", "bat")}:  # đọc lại để kiểm chứng
                raise CliError(f"{name}: cấu hình sau khi áp dụng chưa khớp: {after}")
            total += len(cmds)
        if total:
            cli.command("write memory")
            print(f"[tu-dong] Đã áp dụng và kiểm chứng {total} lệnh, đã lưu cấu hình", flush=True)
        elif not dry_run:
            print("[tu-dong] Thiết bị đã ở đúng trạng thái mong muốn (lũy đẳng)", flush=True)
        print("[tu-dong] show ip interface brief:", flush=True)
        for line in cli.command("show ip interface brief"):
            print("  " + line, flush=True)
    except (CliError, OSError) as e:
        print(f"[tu-dong] LỖI: {e}", flush=True)
        sys.exit(1)
    finally:
        cli.close()


if __name__ == "__main__":
    main()
