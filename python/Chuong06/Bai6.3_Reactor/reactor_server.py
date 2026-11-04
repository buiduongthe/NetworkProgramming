"""Bài 6.3 – Server echo một luồng dùng dồn kênh vào/ra (mô hình reactor).

Cài đặt Mã giả 6.4 (Mục 6.2.4, Giáo trình Lập trình mạng). Kế thừa Mã giả 3.1: cùng chức năng echo,
vòng ACCEPT/RECV chặn được thay bằng vòng SELECT trên tập socket không chặn (mô-đun selectors).

Chạy:  python3 reactor_server.py [host] [port]      (mặc định 0.0.0.0 9000)
"""
import selectors
import socket
import sys

from netutil import parse_endpoint

MAX_BUF = 65536                     # Mã giả 6.4 dòng 4: ngưỡng bộ đệm gửi của mỗi kết nối (64 KiB)


def watch(sel, s, events):
    """Đăng ký/cập nhật/hủy mối quan tâm của s với selector (selectors không nhận events = 0)."""
    key = sel.get_map().get(s)
    if events == 0:
        if key is not None:
            sel.unregister(s)
    elif key is None:
        sel.register(s, events)
    elif key.events != events:
        sel.modify(s, events)


def reactor_server(args):
    host, port = parse_endpoint(args, "0.0.0.0", 9000)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)       # như Mã giả 3.1
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(16)
    srv.setblocking(False)                                          # SETOPT(srv, NONBLOCKING, TRUE)
    out_buf = {}                                                    # dòng 4: socket → dữ liệu chờ gửi
    closing = set()                                                 # dòng 5: client đã đóng chiều gửi
    names, echoed = {}, {}                                          # chỉ để ghi nhật ký
    sel = selectors.DefaultSelector()                               # epoll trên Linux
    print(f"[server] Reactor một luồng đang lắng nghe tại {host}:{port}, MAX_BUF = {MAX_BUF}", flush=True)

    def close_conn(s):
        watch(sel, s, 0)
        del out_buf[s]                                              # REMOVE(out_buf, s)
        closing.discard(s)                                          # REMOVE(closing, s)
        s.close()                                                   # CLOSE(s)
        print(f"[server] Đóng {names.pop(s)}: đã echo {echoed.pop(s)} byte, còn mở {len(out_buf)}",
              flush=True)

    while True:
        pending = {c for c in out_buf if out_buf[c]}                # dòng 7: còn dữ liệu chờ gửi
        room = {c for c in out_buf if c not in closing              # dòng 8–9: bộ đệm gửi chưa đầy,
                and len(out_buf[c]) < MAX_BUF}                      # không theo dõi đọc socket đã nhận FIN
        watch(sel, srv, selectors.EVENT_READ)
        for c in out_buf:
            watch(sel, c, (selectors.EVENT_READ if c in room else 0) |
                          (selectors.EVENT_WRITE if c in pending else 0))
        events = sel.select()                                       # dòng 10: SELECT({srv} ∪ room, pending)
        readable = [k.fileobj for k, ev in events if ev & selectors.EVENT_READ]
        writable = [k.fileobj for k, ev in events if ev & selectors.EVENT_WRITE]
        for s in readable:
            if s is srv:
                conn, a = srv.accept()                              # dòng 13: có kết nối mới
                conn.setblocking(False)                             # SETOPT(conn, NONBLOCKING, TRUE)
                out_buf[conn] = b""                                 # EMPTY_BYTES
                names[conn], echoed[conn] = f"{a[0]}:{a[1]}", 0
                print(f"[server] Client {names[conn]} kết nối (đang mở: {len(out_buf)})", flush=True)
            else:
                try:
                    data = s.recv(4096)                             # dòng 17: socket đã sẵn sàng, không chờ
                except OSError:                                     # ví dụ ConnectionReset: đóng ngay
                    close_conn(s)
                    continue
                if len(data) == 0:                                  # dòng 18–19: client gửi FIN
                    closing.add(s)                                  # gửi nốt rồi mới đóng
                    print(f"[server] {names[s]} gửi FIN, còn {len(out_buf[s])} byte chờ gửi", flush=True)
                else:
                    out_buf[s] += data                              # dòng 21: chờ gửi
        for s in writable:                                          # dòng 25–28
            if s not in out_buf:                                    # s vừa bị đóng do lỗi
                continue
            try:
                n = s.send(out_buf[s])                              # có thể gửi một phần
            except OSError:
                close_conn(s)
                continue
            out_buf[s] = out_buf[s][n:]
            echoed[s] += n
        for s in list(closing):                                     # dòng 29–33: COPY(closing)
            if not out_buf[s]:                                      # đã gửi hết: đóng
                close_conn(s)


def main():
    try:
        reactor_server(sys.argv[1:])
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)


if __name__ == "__main__":
    main()
