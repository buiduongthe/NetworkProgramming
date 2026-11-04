using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;

namespace Bai7_3;

sealed class ProtocolException(string message) : Exception(message);

// Mã giả 7.3 (WS_ACCEPT) và Mã giả 7.4 (WS_READ_FRAME): server echo WebSocket, mỗi kết nối một tác vụ (Mã giả 6.3)
static class WsServer
{
    const string WsGuid = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    const long MaxFrame = 1048576;                                      // 1 MiB (Mã giả 3.6)

    public static async Task<int> RunAsync(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "0.0.0.0";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        while (true)
        {
            Socket conn = await srv.AcceptAsync();
            _ = Session(conn);                                          // RUN_BACKGROUND
        }
    }

    // Mã giả 7.3: kiểm tra yêu cầu nâng cấp và gửi 101 kèm Sec-WebSocket-Accept
    // BASE64_DECODE; chuỗi không phải Base64 hợp lệ cho kết quả rỗng (sẽ bị từ chối)
    static byte[] Base64Decode(string text)
    {
        try { return Convert.FromBase64String(text); }
        catch (FormatException) { return []; }
    }

    static async Task<bool> WsAccept(Stream ns, Dictionary<string, string> hdr)
    {
        string key = hdr.GetValueOrDefault("sec-websocket-key", "");    // dòng 3
        if (hdr.GetValueOrDefault("upgrade", "").ToLowerInvariant() != "websocket" ||   // dòng 4–6
            hdr.GetValueOrDefault("sec-websocket-version") != "13" ||
            Base64Decode(key).Length != 16)                             // khóa: đúng 16 byte
        {
            byte[] b = Encoding.UTF8.GetBytes("{\"type\":\"about:blank\",\"title\":\"Sai WebSocket\",\"status\":400}");
            await ns.WriteAsync((byte[])[.. Encoding.ASCII.GetBytes("HTTP/1.1 400 Bad Request\r\n" +
                "Content-Type: application/problem+json; charset=utf-8\r\n" + $"Content-Length: {b.Length}\r\n\r\n"), .. b]);
            return false;
        }
        string acc = Convert.ToBase64String(SHA1.HashData(Encoding.ASCII.GetBytes(key + WsGuid)));
        string h = "HTTP/1.1 101 Switching Protocols\r\n" + "Upgrade: websocket\r\nConnection: Upgrade\r\n" +
                   $"Sec-WebSocket-Accept: {acc}\r\n\r\n";
        await ns.WriteAsync(Encoding.ASCII.GetBytes(h));
        return true;                                                    // từ đây trao đổi bằng khung WebSocket
    }

    // Gửi một khung từ server: FIN = 1, không che
    static async Task WsSend(Stream ns, int opcode, byte[] data)
    {
        byte[] head = data.Length < 126 ? [(byte)(0x80 | opcode), (byte)data.Length]
            : data.Length < 65536 ? [(byte)(0x80 | opcode), 126, (byte)(data.Length >> 8), (byte)data.Length]
            : [(byte)(0x80 | opcode), 127, .. BitConverter.GetBytes((ulong)data.Length).Reverse()];
        await ns.WriteAsync((byte[])[.. head, .. data]);
    }

    // WS_CLOSE(s, code): khung opcode 0x8 chứa mã đóng 2 byte
    static Task WsClose(Stream ns, int code) => WsSend(ns, 0x8, [(byte)(code >> 8), (byte)code]);

    // Mã giả 7.4: đọc một khung WebSocket
    static async Task<(bool Fin, int Opcode, byte[] Data)> WsReadFrame(AsyncReader r, Stream ns, bool fromClient)
    {
        byte[] b = await r.ReadExactAsync(2);
        bool fin = (b[0] & 0x80) != 0; int opcode = b[0] & 0x0F;
        bool masked = (b[1] & 0x80) != 0; ulong n = (ulong)(b[1] & 0x7F);
        if (n == 126) n = BinaryPrimitives.ReadUInt16BigEndian(await r.ReadExactAsync(2));
        if (n == 127) n = BinaryPrimitives.ReadUInt64BigEndian(await r.ReadExactAsync(8));
        if (n > MaxFrame)                                               // kể cả khi bit cao = 1 (ulong)
        {
            await WsClose(ns, 1009);                                    // 1009: thông điệp quá lớn
            throw new ProtocolException("Khung quá lớn");
        }
        if (fromClient && !masked)
        {
            await WsClose(ns, 1002);                                    // 1002: vi phạm giao thức
            throw new ProtocolException("Khung client phải được che");
        }
        byte[]? mask = masked ? await r.ReadExactAsync(4) : null;
        byte[] data = await r.ReadExactAsync((int)n);
        if (mask != null)
            for (int i = 0; i < data.Length; i++) data[i] ^= mask[i % 4];   // XOR với khóa ở vị trí i MOD 4
        return (fin, opcode, data);
    }

    static async Task Session(Socket conn)
    {
        string addr = conn.RemoteEndPoint!.ToString()!;
        await using var ns = new NetworkStream(conn, ownsSocket: true);
        var r = new AsyncReader(ns);
        try
        {
            string line = await r.ReadLineAsync(8192) ?? throw new EndOfStreamException();
            var hdr = await r.ReadHeadersAsync();
            if (!await WsAccept(ns, hdr))
            {
                Console.WriteLine($"[server] {addr} {line} -> 400 (không phải yêu cầu WebSocket)");
                return;
            }
            Console.WriteLine($"[server] Bắt tay WebSocket thành công với {addr}");
            var message = new MemoryStream(); int msgType = 0;
            while (true)
            {
                var (fin, opcode, data) = await WsReadFrame(r, ns, fromClient: true);
                if (opcode == 0x8)                                      // đóng: gửi lại mã đóng
                {
                    int code = data.Length >= 2 ? (data[0] << 8) | data[1] : 1005;
                    Console.WriteLine($"[server] {addr} gửi khung đóng, mã {code}");
                    await WsClose(ns, code == 1005 ? 1000 : code);
                    return;
                }
                if (opcode == 0x9)                                      // ping -> pong cùng dữ liệu
                {
                    Console.WriteLine($"[server] Nhận ping: '{Encoding.UTF8.GetString(data)}' -> gửi pong");
                    await WsSend(ns, 0xA, data);
                    continue;
                }
                if (opcode is 0x1 or 0x2) { message.SetLength(0); msgType = opcode; }
                message.Write(data);                                    // opcode 0x0: khung tiếp nối
                if (!fin) continue;
                byte[] msg = message.ToArray();
                if (msgType == 0x1)
                {
                    string text = Encoding.UTF8.GetString(msg);
                    Console.WriteLine($"[server] Nhận văn bản ({msg.Length} byte): '{text[..Math.Min(40, text.Length)]}'");
                    await WsSend(ns, 0x1, Encoding.UTF8.GetBytes("echo: " + text));
                }
                else
                {
                    Console.WriteLine($"[server] Nhận nhị phân ({msg.Length} byte)");
                    await WsSend(ns, 0x2, msg);
                }
            }
        }
        catch (ProtocolException e) { Console.WriteLine($"[server] Lỗi giao thức với {addr}: {e.Message}"); }
        catch (Exception e) when (e is IOException or EndOfStreamException)
        { Console.WriteLine($"[server] Kết nối {addr} đóng đột ngột: {e.Message}"); }
        finally { Console.WriteLine($"[server] Đóng kết nối {addr}"); }
    }
}
