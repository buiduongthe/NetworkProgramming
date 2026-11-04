"""Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía server (RESPONDER).

Cài đặt Mã giả 6.7, thủ tục RESPONDER (Mục 6.4.2, Giáo trình Lập trình mạng): nhận yêu cầu discover,
chờ ngẫu nhiên 0–200 ms (giãn phản hồi) rồi trả lời bằng unicast kèm cổng dịch vụ.
Trong server thật, RESPONDER chạy như một tác vụ nền bên cạnh vòng chấp nhận kết nối.

Chạy:  python3 responder.py [service_port] [tên] [disc_port]   (mặc định 9200 server-chat 9999)
"""
import asyncio
import json
import random
import socket
import sys

DISC_PORT = 9999                                        # CONST DISC_PORT ← 9999


async def responder(service_port, name, disc_port=DISC_PORT):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)             # SOCKET(INET, DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)         # cho nhiều responder trên một máy
    s.bind(("0.0.0.0", disc_port))                                   # BIND(s, "0.0.0.0", DISC_PORT)
    s.setblocking(False)
    loop = asyncio.get_running_loop()
    print(f"[responder] {name}, cổng dịch vụ {service_port}, chờ discover tại cổng {disc_port}", flush=True)
    while True:
        d, addr = await loop.sock_recvfrom(s, 2048)                  # AWAIT RECVFROM_ASYNC
        try:
            q = json.loads(d.decode("utf-8"))
        except ValueError:                                           # JsonError: sai định dạng, bỏ
            continue
        if isinstance(q, dict) and q.get("type") == "discover" and q.get("service") == "chat":
            delay = random.randint(0, 200)
            print(f"[responder] discover id={q.get('id')} từ {addr[0]}:{addr[1]}, trả lời sau {delay} ms",
                  flush=True)
            await asyncio.sleep(delay / 1000)                        # AWAIT DELAY_ASYNC: giãn phản hồi
            r = {"id": q.get("id"), "port": service_port, "name": name}
            await loop.sock_sendto(s, json.dumps(r, ensure_ascii=False).encode("utf-8"), addr)  # unicast


def main():
    a = sys.argv
    service_port = int(a[1]) if len(a) > 1 else 9200
    name = a[2] if len(a) > 2 else "server-chat"
    disc_port = int(a[3]) if len(a) > 3 else DISC_PORT
    try:
        asyncio.run(responder(service_port, name, disc_port))
    except KeyboardInterrupt:
        print("[responder] Dừng", flush=True)


if __name__ == "__main__":
    main()
