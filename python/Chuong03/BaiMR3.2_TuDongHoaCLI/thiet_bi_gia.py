"""Bài mở rộng MR3.2 – Thiết bị mạng mô phỏng có giao diện dòng lệnh (CLI) qua TCP.

Mô phỏng một bộ định tuyến kiểu Telnet để luyện tự động hóa mà không cần thiết bị thật:
  - Đăng nhập: "Username: " / "Password: " (mật khẩu lấy từ biến môi trường DEVICE_PASSWORD).
  - Dấu nhắc theo chế độ: R1>  R1#  R1(config)#  R1(config-if)#
  - Lệnh: enable, terminal length 0, show version, show ip interface brief,
    show running-config [interface <tên>], configure terminal, interface <tên>,
    description <chữ>, ip address <ip> <mặt_nạ>, [no] shutdown, exit, end, write memory.
  - Mỗi dòng lệnh được "vọng lại" (echo) như terminal thật; dòng kết thúc bằng CRLF.
  - Khi chưa tắt phân trang, "show running-config" dừng sau 8 dòng với " --More-- ".
Trạng thái cấu hình giữ trong bộ nhớ, dùng chung cho mọi phiên (có khóa).
CẢNH BÁO: giống Telnet, mật khẩu đi dạng rõ; thiết bị thật phải dùng SSH.

Chạy:  python3 thiet_bi_gia.py [host] [port]      (mặc định 127.0.0.1 9000)
"""
import ipaddress
import itertools
import os
import socket
import sys
import threading

HOSTNAME = "R1"
USER = os.environ.get("DEVICE_USER", "admin")
PASSWORD = os.environ.get("DEVICE_PASSWORD", "lab123")
PAGE = 8                                          # số dòng mỗi trang khi bật phân trang

state_lock = threading.Lock()
interfaces = {                                    # trạng thái ban đầu của thiết bị
    "Gi0/0": {"desc": "Lien ket R2", "ip": "10.20.1.225", "mask": "255.255.255.252", "shut": False},
    "Gi0/1": {"desc": "", "ip": "", "mask": "", "shut": True},
    "Gi0/2": {"desc": "", "ip": "", "mask": "", "shut": True},
}
sessions = itertools.count(1)


def log(text):
    print(f"[thiet-bi] {text}", flush=True)


def running_config(only=None):
    """Sinh cấu hình đang chạy từ trạng thái (toàn bộ hoặc một cổng)."""
    lines = [] if only else ["!", f"hostname {HOSTNAME}", "!"]
    with state_lock:
        for name, i in interfaces.items():
            if only and name != only:
                continue
            lines.append(f"interface {name}")
            if i["desc"]:
                lines.append(f" description {i['desc']}")
            lines.append(f" ip address {i['ip']} {i['mask']}" if i["ip"] else " no ip address")
            if i["shut"]:
                lines.append(" shutdown")
            lines.append("!")
    if not only:
        lines.append("end")
    return lines


def ip_brief():
    lines = [f"{'Interface':<10}{'IP-Address':<14}{'Status':<22}Protocol"]
    with state_lock:
        for name, i in interfaces.items():
            status = "administratively down" if i["shut"] else "up"
            lines.append(f"{name:<10}{i['ip'] or 'unassigned':<14}{status:<22}{'down' if i['shut'] else 'up'}")
    return lines


class Session:
    def __init__(self, conn, sid):
        self.conn, self.sid = conn, sid
        self.rf = conn.makefile("rb")             # đọc theo dòng và theo từng byte
        self.mode, self.iface, self.paging = "user", None, True

    def send(self, text):
        self.conn.sendall(text.encode("utf-8"))

    def readline(self):
        line = self.rf.readline()
        if not line:
            raise EOFError
        return line.decode("utf-8").rstrip("\r\n")

    def prompt(self):
        suffix = {"user": ">", "enable": "#", "config": "(config)#", "config-if": "(config-if)#"}[self.mode]
        self.send(f"{HOSTNAME}{suffix}")

    def show(self, lines):
        """In nhiều dòng; nếu đang phân trang thì dừng mỗi PAGE dòng chờ một phím bất kỳ."""
        for k, line in enumerate(lines, 1):
            self.send(line + "\r\n")
            if self.paging and k % PAGE == 0 and k < len(lines):
                self.send(" --More-- ")
                if not self.rf.read(1):
                    raise EOFError
                self.send("\r          \r")            # xóa chữ --More-- như terminal thật

    def login(self):
        self.send(f"\r\n{HOSTNAME} - thiết bị mô phỏng cho bài thực hành\r\n\r\nUsername: ")
        user = self.readline()
        self.send("Password: ")
        pw = self.readline()                           # không vọng lại mật khẩu
        self.send("\r\n")
        if user != USER or pw != PASSWORD:
            log(f"phiên #{self.sid}: đăng nhập SAI (user={user!r})")
            self.send("% Authentication failed\r\n")
            return False
        log(f"phiên #{self.sid}: đăng nhập {user}")
        return True

    def run(self):
        if not self.login():
            return
        while True:
            self.prompt()
            cmd = self.readline().strip()
            self.send(cmd + "\r\n")                    # vọng lại dòng lệnh
            if not cmd:
                continue
            if not self.execute(cmd):
                break

    def invalid(self):
        self.send("% Invalid input detected at '^' marker.\r\n")

    def execute(self, cmd):
        words = cmd.split()
        m = self.mode
        if cmd == "exit" and m in ("user", "enable"):
            return False
        if m == "user":
            if cmd == "enable":
                self.mode = "enable"
            elif cmd == "show version":
                self.show(["Thiết bị mô phỏng R1, phần mềm SIM-IOS 1.0", f"{len(interfaces)} cổng GigabitEthernet"])
            else:
                self.invalid()
        elif m == "enable":
            if cmd == "terminal length 0":
                self.paging = False
            elif cmd == "show version":
                self.show(["Thiết bị mô phỏng R1, phần mềm SIM-IOS 1.0", f"{len(interfaces)} cổng GigabitEthernet"])
            elif cmd == "show ip interface brief":
                self.show(ip_brief())
            elif cmd == "show running-config":
                self.show(running_config())
            elif words[:3] == ["show", "running-config", "interface"] and len(words) == 4:
                if words[3] in interfaces:
                    self.show(running_config(words[3]))
                else:
                    self.invalid()
            elif cmd in ("configure terminal", "conf t"):
                self.send("Enter configuration commands, one per line.  End with CNTL/Z.\r\n")
                self.mode = "config"
            elif cmd in ("write memory", "wr"):
                self.send("Building configuration...\r\n[OK]\r\n")
                log(f"phiên #{self.sid}: lưu cấu hình (write memory)")
            else:
                self.invalid()
        elif cmd == "end":
            self.mode, self.iface = "enable", None
        elif m == "config":
            if words[0] == "interface" and len(words) == 2 and words[1] in interfaces:
                self.mode, self.iface = "config-if", words[1]
            elif cmd == "exit":
                self.mode = "enable"
            else:
                self.invalid()
        elif m == "config-if":
            self.config_if(cmd, words)
        return True

    def config_if(self, cmd, words):
        if cmd == "exit":
            self.mode, self.iface = "config", None
            return
        with state_lock:
            i = interfaces[self.iface]
            if words[0] == "description" and len(words) > 1:
                i["desc"] = cmd[len("description "):]
            elif cmd == "no description":
                i["desc"] = ""
            elif words[:2] == ["ip", "address"] and len(words) == 4:
                try:
                    ipaddress.ip_interface(f"{words[2]}/{words[3]}")
                except ValueError:
                    self.send("% Invalid address\r\n")
                    return
                i["ip"], i["mask"] = words[2], words[3]
            elif cmd == "shutdown":
                i["shut"] = True
            elif cmd == "no shutdown":
                i["shut"] = False
            else:
                self.invalid()
                return
        log(f"phiên #{self.sid}: {self.iface}: {cmd}")


def handle(conn, addr):
    sid = next(sessions)
    s = Session(conn, sid)
    try:
        s.run()
    except (EOFError, OSError):
        pass
    finally:
        conn.close()
        log(f"phiên #{sid} ({addr[0]}:{addr[1]}) kết thúc")


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 9000
    srv = socket.create_server((host, port))
    log(f"{HOSTNAME} lắng nghe CLI tại {host}:{port}")
    try:
        while True:
            conn, addr = srv.accept()
            threading.Thread(target=handle, args=(conn, addr), daemon=True).start()
    except KeyboardInterrupt:
        log("Dừng thiết bị")
    finally:
        srv.close()


if __name__ == "__main__":
    main()
