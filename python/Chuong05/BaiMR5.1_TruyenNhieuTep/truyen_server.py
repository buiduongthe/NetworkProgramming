"""Bài mở rộng MR5.1 – Server truyền nhiều tệp: mỗi kết nối phục vụ một lượt tải xuống/tải lên.

Giao thức: nhận một khung JSON điều khiển, rồi truyền luồng byte (xem khung.py).
  - download: {"op":"download","name":TÊN,"offset":OFF}
       -> trả {"ok":true,"size":TỔNG,"sha256":HEX} rồi gửi byte [OFF..hết].
  - upload:   {"op":"upload","name":TÊN,"offset":OFF,"size":TỔNG,"sha256":HEX}
       -> trả {"ok":true} rồi nhận byte, ghi nối vào TÊN.part từ OFF; xong thì kiểm SHA-256,
          đổi tên thành TÊN, trả {"ok":true,"sha256_ok":...}.
Server KHÔNG tự giới hạn tốc độ – client điều tiết, TCP tự tạo áp lực ngược.

Chạy:  python3 truyen_server.py <host> <port> [thư_mục_lưu]
"""
import os
import socket
import sys
import threading

from khung import CHUNK, doc_du, gui_khung, nhan_khung, sha256_tep


def phuc_vu(conn, kho):
    try:
        req = nhan_khung(conn)
        op, name = req.get("op"), os.path.basename(req.get("name", ""))
        if op == "download":
            path = os.path.join(kho, name)
            if not os.path.isfile(path):
                gui_khung(conn, {"ok": False, "loi": "Không có tệp " + name})
                return
            size = os.path.getsize(path)
            gui_khung(conn, {"ok": True, "size": size, "sha256": sha256_tep(path)})
            off = int(req.get("offset", 0))
            with open(path, "rb") as f:
                f.seek(off)
                while True:
                    b = f.read(CHUNK)
                    if not b:
                        break
                    conn.sendall(b)
        elif op == "upload":
            off = int(req.get("offset", 0))
            total = int(req.get("size", 0))
            part = os.path.join(kho, name + ".part")
            gui_khung(conn, {"ok": True})
            mode = "r+b" if off > 0 and os.path.exists(part) else "wb"
            f = open(part, mode)
            f.seek(off)
            nhan = off
            while nhan < total:
                b = doc_du(conn, min(CHUNK, total - nhan))
                f.write(b)
                nhan += len(b)
            f.close()
            sha_ok = sha256_tep(part) == req.get("sha256")
            if sha_ok:
                os.replace(part, os.path.join(kho, name))
            gui_khung(conn, {"ok": True, "sha256_ok": sha_ok})
        else:
            gui_khung(conn, {"ok": False, "loi": "op lạ"})
    except (ConnectionError, OSError):
        pass                              # client ngắt giữa chừng (pause/stop) là bình thường
    finally:
        conn.close()


def main():
    if len(sys.argv) < 3:
        sys.exit("Cách dùng: python3 truyen_server.py <host> <port> [thư_mục_lưu]")
    host, port = sys.argv[1], int(sys.argv[2])
    kho = sys.argv[3] if len(sys.argv) > 3 else "kho_server"
    os.makedirs(kho, exist_ok=True)
    srv = socket.create_server((host, port))
    srv.listen(16)
    print(f"[server] Nghe tại {host}:{port}, thư mục lưu: {kho}", flush=True)
    try:
        while True:
            conn, _ = srv.accept()
            threading.Thread(target=phuc_vu, args=(conn, kho), daemon=True).start()
    except KeyboardInterrupt:
        print("[server] Dừng.", flush=True)
    finally:
        srv.close()


if __name__ == "__main__":
    main()
