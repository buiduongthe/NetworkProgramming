"""Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (phía server, grpc.aio).

Cài đặt Mã giả 7.6, phần CLASS GlossaryService (Mục 7.4.3, Giáo trình Lập trình mạng):
Lookup (unary, NOT_FOUND khi không có) và Watch (luồng từ server, dừng khi client hủy).
Kế thừa: Mã giả 3.7 – nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup.
Lớp cơ sở GlossaryServicer do grpcio-tools sinh từ tra_cuu.proto.

Chuẩn bị: pip install -r requirements.txt
          python3 -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. tra_cuu.proto
Chạy:     python3 glossary_server.py [host] [port] [độ_trễ_ms]   (mặc định 0.0.0.0 5001 0)
"""
import asyncio
import sys

import grpc
import tra_cuu_pb2 as pb
import tra_cuu_pb2_grpc as pb_grpc

DICT = {"TCP": "Giao thức vận chuyển hướng kết nối", "UDP": "Giao thức vận chuyển không kết nối"}
CHANGES = ["HTTP/2", "QUIC", "HTTP/3", "gRPC", "HTTPS"]          # dòng thay đổi mô phỏng


async def changes_feed():
    """CHANGES_FEED(): mỗi 0,5 giây có một thuật ngữ mới."""
    for term in CHANGES:
        await asyncio.sleep(0.5)
        yield pb.Term(term=term, text=f"Thuật ngữ mới: {term}", tags=["moi"])


class GlossaryService(pb_grpc.GlossaryServicer):                # EXTENDS GlossaryBase
    def __init__(self, delay_ms):
        self.delay_ms = delay_ms                                  # dùng để thử hạn chót (deadline)

    async def Lookup(self, req, ctx):
        if self.delay_ms:
            try:
                await asyncio.sleep(self.delay_ms / 1000)         # server cố ý chậm
            except asyncio.CancelledError:                        # hạn chót truyền qua grpc-timeout
                print(f"[server] Lookup('{req.term}') bị hủy: client đã hết hạn chót", flush=True)
                raise
        if req.term not in DICT:
            print(f"[server] Lookup('{req.term}') -> NOT_FOUND", flush=True)
            await ctx.abort(grpc.StatusCode.NOT_FOUND, "Không có " + req.term)   # RAISE RpcError
        print(f"[server] Lookup('{req.term}') -> OK", flush=True)
        return pb.Term(term=req.term, text=DICT[req.term])

    async def Watch(self, req, ctx):                              # luồng từ server
        print(f"[server] Watch(prefix='{req.prefix}') bắt đầu", flush=True)
        async for t in changes_feed():
            if ctx.cancelled():                                   # client đã hủy
                return
            if t.term.startswith(req.prefix):
                await ctx.write(t)                                # stream.write(t)
                print(f"[server] Watch: gửi {t.term}", flush=True)
        print("[server] Watch kết thúc", flush=True)


async def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "0.0.0.0"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 5001
    delay_ms = int(sys.argv[3]) if len(sys.argv) >= 4 else 0
    server = grpc.aio.server()
    pb_grpc.add_GlossaryServicer_to_server(GlossaryService(delay_ms), server)
    server.add_insecure_port(f"{host}:{port}")                    # HTTP/2 không TLS (h2c) cho phòng thí nghiệm
    await server.start()
    print(f"[server] Dịch vụ gRPC glossary.v1.Glossary tại {host}:{port} (độ trễ Lookup {delay_ms} ms)", flush=True)
    try:
        await server.wait_for_termination()
    finally:
        await server.stop(None)                                   # dừng gọn khi nhấn Ctrl+C


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("[server] Dừng server", flush=True)
