using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;

namespace Bai6_3;

// Mã giả 6.4 – Server echo một luồng dùng dồn kênh vào/ra (Mục 6.2.4).
// Kế thừa Mã giả 3.1: cùng chức năng echo; vòng ACCEPT/RECV chặn được thay bằng vòng Socket.Select
// trên tập socket không chặn.
static class ReactorServer
{
    const int MaxBuf = 65536;            // Mã giả 6.4 dòng 4: ngưỡng bộ đệm gửi của mỗi kết nối (64 KiB)

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9000);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // như Mã giả 3.1
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        srv.Blocking = false;                                            // SETOPT(srv, NONBLOCKING, TRUE)
        var outBuf = new Dictionary<Socket, List<byte>>();               // dòng 4: socket → dữ liệu chờ gửi
        var closing = new HashSet<Socket>();                             // dòng 5: client đã đóng chiều gửi
        var names = new Dictionary<Socket, string>();                    // chỉ để ghi nhật ký
        var echoed = new Dictionary<Socket, long>();
        var chunk = new byte[4096];
        Console.WriteLine($"[server] Reactor một luồng đang lắng nghe tại {host}:{port}, MAX_BUF = {MaxBuf}");

        void CloseConn(Socket s)
        {
            outBuf.Remove(s);                                            // REMOVE(out_buf, s)
            closing.Remove(s);                                           // REMOVE(closing, s)
            s.Close();                                                   // CLOSE(s)
            Console.WriteLine($"[server] Đóng {names[s]}: đã echo {echoed[s]} byte, còn mở {outBuf.Count}");
            names.Remove(s);
            echoed.Remove(s);
        }

        while (true)
        {
            var pending = outBuf.Where(kv => kv.Value.Count > 0).Select(kv => kv.Key).ToList();   // dòng 7
            var room = outBuf.Where(kv => !closing.Contains(kv.Key) && kv.Value.Count < MaxBuf)  // dòng 8–9
                             .Select(kv => kv.Key).ToList();
            var readable = new List<Socket> { srv };                    // {srv} ∪ room
            readable.AddRange(room);
            var writable = pending;
            Socket.Select(readable, writable.Count > 0 ? writable : null, null, -1);   // dòng 10: SELECT, chặn tại đây
            foreach (var s in readable)
            {
                if (s == srv)
                {
                    Socket conn = srv.Accept();                          // dòng 13: có kết nối mới
                    conn.Blocking = false;                               // SETOPT(conn, NONBLOCKING, TRUE)
                    outBuf[conn] = new List<byte>();                     // EMPTY_BYTES
                    names[conn] = conn.RemoteEndPoint!.ToString()!;
                    echoed[conn] = 0;
                    Console.WriteLine($"[server] Client {names[conn]} kết nối (đang mở: {outBuf.Count})");
                    continue;
                }
                int n;
                try { n = s.Receive(chunk); }                            // dòng 17: socket đã sẵn sàng, không chờ
                catch (SocketException) { CloseConn(s); continue; }      // ví dụ ConnectionReset: đóng ngay
                if (n == 0)                                              // dòng 18–19: client gửi FIN
                {
                    closing.Add(s);                                      // gửi nốt rồi mới đóng
                    Console.WriteLine($"[server] {names[s]} gửi FIN, còn {outBuf[s].Count} byte chờ gửi");
                }
                else outBuf[s].AddRange(chunk.AsSpan(0, n).ToArray());   // dòng 21: chờ gửi
            }
            foreach (var s in writable)                                  // dòng 25–28
            {
                if (!outBuf.TryGetValue(s, out var buf)) continue;       // s vừa bị đóng do lỗi
                int n;
                try { n = s.Send(CollectionsMarshal.AsSpan(buf)); }      // có thể gửi một phần
                catch (SocketException) { CloseConn(s); continue; }
                buf.RemoveRange(0, n);                                   // out_buf[s] ← out_buf[s][n .. END]
                echoed[s] += n;
            }
            foreach (var s in closing.ToList())                          // dòng 29–33: COPY(closing)
                if (outBuf[s].Count == 0) CloseConn(s);                  // đã gửi hết: đóng
        }
    }
}
