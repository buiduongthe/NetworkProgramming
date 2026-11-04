using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;

namespace Bai7_3;

// Kiểm thử server bằng khung tự tạo trên socket (ClientWebSocket không cho gửi khung sai đặc tả):
// 1. HTTP thường -> 400; 2. ping -> pong; 3. văn bản 300 byte (độ dài 16 bit); 4. khung không che -> 1002;
// 5. khung khai báo 2^40 byte -> 1009; 6. Sec-WebSocket-Key không phải 16 byte -> 400
static class WsKiemThu
{
    const string WsGuid = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

    public static int Run(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "127.0.0.1";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        using (var c = new TcpClient(host, port))                       // ca 1
        {
            var ns = c.GetStream();
            ns.Write(Encoding.ASCII.GetBytes($"GET / HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n"));
            var buf = new byte[4096]; int n = ns.Read(buf);
            Console.WriteLine($"[kiểm thử] 1. Yêu cầu HTTP thường (không Upgrade) -> {Encoding.ASCII.GetString(buf, 0, n).Split("\r\n")[0]}");
        }
        using (var s = Connect(host, port))                             // ca 2
        {
            Send(s, 0x9, Encoding.ASCII.GetBytes("nhip-tim"));
            var (op, data) = ReadFrame(s);
            Console.WriteLine($"[kiểm thử] 2. Ping 'nhip-tim' -> opcode 0x{op:X}, dữ liệu '{Encoding.ASCII.GetString(data)}'");
            Send(s, 0x8, [0x03, 0xE8]); ReadFrame(s);
        }
        using (var s = Connect(host, port))                             // ca 3
        {
            Send(s, 0x1, Enumerable.Repeat((byte)'A', 300).ToArray());
            var (op, data) = ReadFrame(s);
            Console.WriteLine($"[kiểm thử] 3. Văn bản 300 byte -> nhận lại {data.Length} byte (opcode 0x{op:X})");
            Send(s, 0x8, [0x03, 0xE8]); ReadFrame(s);
        }
        using (var s = Connect(host, port))                             // ca 4
        {
            Send(s, 0x1, Encoding.ASCII.GetBytes("khong che"), mask: false);
            var (_, data) = ReadFrame(s);
            Console.WriteLine($"[kiểm thử] 4. Khung không che -> khung đóng, mã {(data[0] << 8) | data[1]}");
        }
        using (var s = Connect(host, port))                             // ca 5: chỉ gửi tiêu đề khung
        {
            s.Write([0x81, 0x80 | 127, 0, 0, 0x01, 0, 0, 0, 0, 0]);    // độ dài 2^40 big-endian
            var (_, data) = ReadFrame(s);
            Console.WriteLine($"[kiểm thử] 5. Khung khai báo 2^40 byte -> khung đóng, mã {(data[0] << 8) | data[1]}");
        }
        using (var c = new TcpClient(host, port))                       // ca 6: khóa không phải 16 byte
        {
            var ns = c.GetStream();
            ns.Write(Encoding.ASCII.GetBytes($"GET / HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n" +
                "Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: c2hvcnQ=\r\n\r\n"));
            var buf = new byte[4096]; int n = ns.Read(buf);
            Console.WriteLine($"[kiểm thử] 6. Sec-WebSocket-Key 'c2hvcnQ=' (5 byte) -> {Encoding.ASCII.GetString(buf, 0, n).Split("\r\n")[0]}");
        }
        return 0;
    }

    // Gửi yêu cầu nâng cấp và kiểm tra Sec-WebSocket-Accept (phía client của Mã giả 7.3)
    static NetworkStream Connect(string host, int port)
    {
        var c = new TcpClient(host, port);
        var ns = c.GetStream(); ns.ReadTimeout = 5000;
        string key = Convert.ToBase64String(RandomNumberGenerator.GetBytes(16));
        ns.Write(Encoding.ASCII.GetBytes($"GET / HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n" +
            $"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"));
        var head = new StringBuilder();
        while (!head.ToString().EndsWith("\r\n\r\n")) head.Append((char)ns.ReadByte());   // đọc từng byte
        string expect = Convert.ToBase64String(SHA1.HashData(Encoding.ASCII.GetBytes(key + WsGuid)));
        if (!head.ToString().Contains("Sec-WebSocket-Accept: " + expect)) throw new IOException("Bắt tay thất bại");
        return ns;                                                      // NetworkStream sở hữu socket qua TcpClient
    }

    static void Send(Stream s, int opcode, byte[] data, bool mask = true)
    {
        var head = new List<byte> { (byte)(0x80 | opcode) };
        byte m = mask ? (byte)0x80 : (byte)0;
        if (data.Length < 126) head.Add((byte)(m | data.Length));
        else { head.Add((byte)(m | 126)); head.Add((byte)(data.Length >> 8)); head.Add((byte)data.Length); }
        byte[] payload = (byte[])data.Clone();
        if (mask)
        {
            byte[] key = RandomNumberGenerator.GetBytes(4);
            head.AddRange(key);
            for (int i = 0; i < payload.Length; i++) payload[i] ^= key[i % 4];
        }
        s.Write([.. head, .. payload]);
    }

    // Mã giả 7.4 với from_client = FALSE (khung từ server không che; độ dài 7 hoặc 16 bit đủ cho ca kiểm thử)
    static (int Opcode, byte[] Data) ReadFrame(Stream s)
    {
        byte[] b = new byte[2]; s.ReadExactly(b);
        int n = b[1] & 0x7F;
        if (n == 126) { byte[] e = new byte[2]; s.ReadExactly(e); n = (e[0] << 8) | e[1]; }
        byte[] data = new byte[n]; s.ReadExactly(data);
        return (b[0] & 0x0F, data);
    }
}
