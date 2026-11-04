"""Bài 7.5 – Gọi dịch vụ gRPC tra cứu thuật ngữ (phía client, grpc.aio).

Cài đặt Mã giả 7.6, phần "Phía client" (Mục 7.4.3): Lookup có hạn chót (deadline), Watch nhận luồng.
Lớp GlossaryStub do grpcio-tools sinh từ tra_cuu.proto. Lỗi được báo bằng mã trạng thái gRPC.

Chạy:  python3 glossary_client.py [host] [port] [hạn_chót_ms]   (mặc định 127.0.0.1 5001 2000)
"""
import asyncio
import sys

import grpc
import tra_cuu_pb2 as pb
import tra_cuu_pb2_grpc as pb_grpc


async def lookup(c, term, deadline_ms):
    try:
        t = await c.Lookup(pb.LookupRequest(term=term), timeout=deadline_ms / 1000)   # deadline ← 2000
        print(f"[client] Lookup('{term}') -> {t.term}: {t.text}", flush=True)
    except grpc.aio.AioRpcError as e:
        print(f"[client] Lookup('{term}') -> lỗi {e.code().name}: {e.details()}", flush=True)


async def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 5001
    deadline_ms = int(sys.argv[3]) if len(sys.argv) >= 4 else 2000
    wire = pb.LookupRequest(term="TCP").SerializeToString()        # mã hóa Protocol Buffers (Mục 7.4.2)
    print(f"[client] LookupRequest{{term: \"TCP\"}} = {wire.hex(' ').upper()} ({len(wire)} byte)", flush=True)
    async with grpc.aio.insecure_channel(f"{host}:{port}") as ch:   # GRPC_CHANNEL (h2c, không TLS)
        c = pb_grpc.GlossaryStub(ch)                                  # GlossaryClient(ch)
        print(f"[client] Gọi Lookup với hạn chót {deadline_ms} ms", flush=True)
        await lookup(c, "TCP", deadline_ms)
        await lookup(c, "XYZ", deadline_ms)
        print("[client] Gọi Watch(prefix='HTTP'), nhận luồng:", flush=True)
        try:
            async for x in c.Watch(pb.WatchRequest(prefix="HTTP")):   # FOR EACH x IN AWAIT_STREAM(...)
                print(f"[client]   {x.term} – {x.text}", flush=True)
        except grpc.aio.AioRpcError as e:
            print(f"[client] Watch -> lỗi {e.code().name}: {e.details()}", flush=True)
    print("[client] Kết thúc", flush=True)


if __name__ == "__main__":
    asyncio.run(main())
